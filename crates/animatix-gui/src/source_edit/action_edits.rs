//! Edits related to action insertion at exact keyframe times.

use animatix_syntax::ast::{Action, Expr, Modifier, Stmt, Time};

use super::ast_utils::{
    adjust_following_relative_keyframe, append_to_keyframe_at_time,
    find_keyframe_insertion_point, find_prev_keyframe_time, keyframe_style_before,
    wrap_leading_decls_in_zero_keyframe, KeyframeStyle,
};
use super::SourceEditError;

/// Tolerance for matching an existing keyframe (50ms).
const TIME_EPSILON_S: f64 = 0.05;

/// Insert an action statement at the exact keyframe for `time_s`.
///
/// Semantics:
/// 1. If a keyframe exists within ε (50ms) of `time_s`, append to it.
/// 2. Otherwise create a new keyframe at `time_s`.
/// 3. Existing keyframes' absolute times are NEVER shifted.
pub(super) fn insert_action(
    stmts: &mut Vec<Stmt>,
    verb: &str,
    targets: &[String],
    args: &[animatix_syntax::ast::Expr],
    modifiers: &[animatix_syntax::ast::Modifier],
    time_s: f64,
) -> Result<(), SourceEditError> {
    let action = Stmt::Action(
        Action {
            verb: verb.into(),
            targets: targets.to_vec(),
            args: args.to_vec(),
            modifiers: modifiers.to_vec(),
            byte_span: None,
        },
        None,
    );

    // ── 1. Exact match: find keyframe within ε of time_s ──
    if append_to_keyframe_at_time(stmts, time_s, action.clone()) {
        return Ok(());
    }

    // ── 2. No match — create keyframe at time_s ──
    let prev_time_s = find_prev_keyframe_time(stmts, time_s);
    let delta_s = time_s - prev_time_s;

    // If we're essentially on top of the previous keyframe, append there
    // to avoid micro-fragmentation.
    if delta_s < TIME_EPSILON_S && append_to_keyframe_at_time(stmts, prev_time_s, action.clone()) {
        return Ok(());
    }

    // ── 3. Choose keyframe style: inherit from preceding keyframe ──
    let style = keyframe_style_before(stmts, time_s);
    let insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // Wrap leading declarations in #0s if inserting before any keyframe
    let insert_idx = wrap_leading_decls_in_zero_keyframe(stmts, insert_idx, prev_time_s);

    match style {
        KeyframeStyle::Absolute => {
            stmts.insert(
                insert_idx,
                Stmt::Keyframe {
                    time: Time::Seconds(time_s),
                    body: vec![action],
                    span: None,
                },
            );
            // Following relative keyframes must be adjusted because
            // their base time changed.
            adjust_following_relative_keyframe(stmts, insert_idx + 1, delta_s);
        }
        KeyframeStyle::Relative => {
            let offset = if delta_s < 1.0 {
                Time::Milliseconds((delta_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(delta_s)
            };
            // Adjust next relative keyframe to preserve its absolute time
            adjust_following_relative_keyframe(stmts, insert_idx, delta_s);
            stmts.insert(
                insert_idx,
                Stmt::RelativeKeyframe {
                    offset,
                    body: vec![action],
                    span: None,
                },
            );
        }
    }
    Ok(())
}

/// Resize an action block's duration (right-edge drag).
///
/// Finds the action matching `verb` + `targets` within the keyframe at `old_start_s`
/// and updates its unnamed duration modifier to `new_duration_s`.
pub(super) fn resize_action(
    stmts: &mut Vec<Stmt>,
    verb: &str,
    targets: &[String],
    old_start_s: f64,
    _new_start_s: f64,
    new_duration_s: f64,
) -> Result<(), SourceEditError> {
    const EPSILON_S: f64 = 0.05;
    let mut current_time = 0.0f64;

    for kf in stmts.iter_mut() {
        match kf {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
            }
            _ => continue,
        }

        if (current_time - old_start_s).abs() >= EPSILON_S {
            continue;
        }

        // Find matching action in this keyframe's body
        let body = match kf {
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => body,
            _ => unreachable!(),
        };

        for stmt in body.iter_mut() {
            if let Stmt::Action(action, _) = stmt {
                if action.verb != verb || !action.targets.iter().any(|t| targets.contains(t)) {
                    continue;
                }

                // Format the new duration as a literal string
                let duration_str = if new_duration_s < 1.0 {
                    format!("{}ms", (new_duration_s * 1000.0).round())
                } else {
                    format!("{}s", new_duration_s)
                };

                // Update or insert the unnamed duration modifier
                if let Some(modifier) = action.modifiers.iter_mut().find(|m| m.name.is_none()) {
                    modifier.value = Expr::Ident(duration_str);
                } else {
                    action.modifiers.insert(
                        0,
                        Modifier {
                            name: None,
                            value: Expr::Ident(duration_str),
                        },
                    );
                }
                return Ok(());
            }
        }

        return Err(SourceEditError::Generic(format!(
            "No action '{verb}' with matching targets at {:.2}s",
            old_start_s
        )));
    }

    Err(SourceEditError::Generic(format!(
        "No keyframe at {:.2}s to resize action '{verb}'",
        old_start_s
    )))
}

fn time_to_seconds(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{apply_edit, SourceEdit};
    use animatix_syntax::ast::{Expr, Modifier, Stmt, Time};
    use animatix_syntax::parser::parser;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser()
            .parse(source)
            .into_result()
            .expect("failed to parse test source")
    }

    fn make_edit(verb: &str, targets: &[&str], time_s: f64) -> SourceEdit {
        SourceEdit::InsertAction {
            verb: verb.into(),
            targets: targets.iter().map(|s| s.to_string()).collect(),
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("1s".into()),
            }],
            time_s,
        }
    }

    // ── Example A: Between absolute keyframes ──
    #[test]
    fn insert_action_between_absolute_keyframes() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#2s
box.color = red"#,
        );
        assert!(apply_edit(&mut stmts, make_edit("fade-in", &["box"], 0.5)).is_ok());

        assert_eq!(stmts.len(), 3);
        // stmts[0] = #0s, stmts[1] = #0.5s, stmts[2] = #2s
        if let Stmt::Keyframe { time, body, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(0.5));
            assert_eq!(body.len(), 1);
            assert!(matches!(&body[0],
                Stmt::Action(action, _) if action.verb == "fade-in"
            ));
        } else {
            panic!("Expected Keyframe at index 1");
        }
        // Existing #2s should be untouched
        if let Stmt::Keyframe { time, .. } = &stmts[2] {
            assert_eq!(*time, Time::Seconds(2.0));
        } else {
            panic!("Expected Keyframe at index 2");
        }
    }

    // ── Example B: After absolute, before relative ──
    #[test]
    fn insert_action_after_absolute_before_relative() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#+2s
box.color = red"#,
        );
        assert!(apply_edit(&mut stmts, make_edit("fade-in", &["box"], 1.5)).is_ok());

        assert_eq!(stmts.len(), 3);
        // New absolute keyframe at #1.5s
        if let Stmt::Keyframe { time, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(1.5));
        } else {
            panic!("Expected Keyframe at index 1");
        }
        // Existing relative keyframe should be adjusted to #+500ms
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    // ── Example C: Extending a relative chain ──
    #[test]
    fn insert_action_extending_relative_chain() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#+2s
box.color = red"#,
        );
        assert!(apply_edit(&mut stmts, make_edit("fade-out", &["box"], 2.5)).is_ok());

        assert_eq!(stmts.len(), 3);
        // New relative keyframe after #+2s
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    // ── Micro-fragmentation: within ε of existing keyframe ──
    #[test]
    fn insert_action_within_epsilon_appends_to_existing() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#2s
box.color = red"#,
        );
        // Insert at 2.02s (within 50ms of #2s)
        assert!(apply_edit(
            &mut stmts,
            make_edit("fade-out", &["box"], 2.02)
        ).is_ok());

        // Should NOT create a new keyframe
        assert_eq!(stmts.len(), 2);
        if let Stmt::Keyframe { body, .. } = &stmts[1] {
            assert_eq!(body.len(), 2);
            assert!(matches!(&body[1],
                Stmt::Action(action, _) if action.verb == "fade-out"
            ));
        } else {
            panic!("Expected Keyframe at index 1");
        }
    }

    // ── Style inheritance: relative after relative ──
    #[test]
    fn insert_action_inherits_relative_style() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#+1s
box.color = red

#+1s
box.size = (50, 50)"#,
        );
        // Insert at 1.5s (between #+1s and #+1s)
        assert!(apply_edit(
            &mut stmts,
            make_edit("fade-in", &["box"], 1.5)
        ).is_ok());

        // The new keyframe should be relative because the one before it was relative
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
        // The following relative keyframe should be adjusted
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[3] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 3");
        }
    }

    // ── Appending to existing keyframe body ──
    #[test]
    fn insert_action_appends_to_exact_match() {
        let mut stmts = parse(
            r#"#0s
box: Rect

#1s
box.color = red"#,
        );
        assert!(apply_edit(&mut stmts, make_edit("fade-in", &["box"], 1.0)).is_ok());

        assert_eq!(stmts.len(), 2);
        if let Stmt::Keyframe { body, .. } = &stmts[1] {
            assert_eq!(body.len(), 2);
            assert!(matches!(&body[0],
                Stmt::Assignment { .. }
            ));
            assert!(matches!(
                &body[1],
                Stmt::Action(action, _) if action.verb == "fade-in"
            ));
        } else {
            panic!("Expected Keyframe at index 1");
        }
    }

    // ── Leading declarations wrapped in #0s ──
    #[test]
    fn insert_action_wraps_leading_decls() {
        let mut stmts = parse("box: Rect\ncircle: Ellipse");
        assert!(apply_edit(&mut stmts, make_edit("fade-in", &["box"], 0.5)).is_ok());

        // Should have 2 statements: #0s with decls, then #+500ms with action
        assert_eq!(stmts.len(), 2);
        if let Stmt::Keyframe { time, body, .. } = &stmts[0] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 2); // both decls
        } else {
            panic!("Expected Keyframe at index 0");
        }
        // No existing keyframes → default style is Absolute
        if let Stmt::Keyframe { time, body, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(0.5));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe at index 1");
        }
    }
}
