//! Edits related to keyframes: insert, merge, delete, and easing updates.

use animatix::ast::{ComponentDef, Expr, Stmt, Time};

use super::apply::{find_assignment_mut, time_to_seconds};
use super::apply::canonical_to_source;

// ---------------------------------------------------------------------------
// MergeKeyframe
// ---------------------------------------------------------------------------

pub(super) fn merge_keyframe(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
) -> bool {
    let source_prop = canonical_to_source(property);
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment(body, actor, source_prop, value);
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment(body, actor, source_prop, value);
                }
            }
            _ => {}
        }
    }

    false
}

fn update_assignment(body: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, value: val, .. }
                if target.iter().any(|t| t == actor) && prop == property =>
            {
                *val = value;
                return true;
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if update_assignment(body, actor, property, value.clone()) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if update_assignment(then_branch, actor, property, value.clone()) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if update_assignment(else_b, actor, property, value.clone()) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if update_assignment(body, actor, property, value.clone()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// SetKeyframeEasing
// ---------------------------------------------------------------------------

pub(super) fn set_keyframe_easing(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    time_s: f64,
    easing: animatix::easing::Easing,
) -> bool {
    let source_prop = canonical_to_source(property);
    let easing_name = match easing {
        animatix::easing::Easing::Linear => "linear",
        animatix::easing::Easing::EaseIn => "easein",
        animatix::easing::Easing::EaseOut => "easeout",
        animatix::easing::Easing::EaseInOut => "easeinout",
        animatix::easing::Easing::Bounce => "bounce",
        animatix::easing::Easing::Elastic => "elastic",
        animatix::easing::Easing::Back => "back",
        animatix::easing::Easing::Expo => "expo",
    };
    let easing_expr = animatix::ast::Expr::Ident(easing_name.to_string());

    // Walk through keyframes looking for the match
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            }
            _ => {}
        }
    }

    false
}

/// Walk into an assignment at the given time and set its easing modifier.
fn update_assignment_easing(
    body: &mut [Stmt],
    actor: &str,
    property: &str,
    easing_expr: &animatix::ast::Expr,
) -> bool {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, modifiers, .. }
                if target.iter().any(|t| t == actor) && prop == property =>
            {
                if let Some(existing) = modifiers.iter_mut().find(|m| m.name.as_deref() == Some("ease")) {
                    existing.value = easing_expr.clone();
                } else {
                    modifiers.push(animatix::ast::Modifier {
                        name: Some("ease".into()),
                        value: easing_expr.clone(),
                    });
                }
                return true;
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if update_assignment_easing(body, actor, property, easing_expr) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if update_assignment_easing(then_branch, actor, property, easing_expr) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if update_assignment_easing(else_b, actor, property, easing_expr) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if update_assignment_easing(body, actor, property, easing_expr) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// DeleteKeyframe
// ---------------------------------------------------------------------------

pub(super) fn delete_keyframe(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
    time_s: f64,
) -> bool {
    let source_prop = canonical_to_source(property);
    let mut current_time = 0.0f64;

    for i in 0..stmts.len() {
        let (is_match, is_empty_after) = match &mut stmts[i] {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    remove_assignment_from_body(body, actor, source_prop);
                    (true, body.is_empty())
                } else {
                    (false, false)
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    remove_assignment_from_body(body, actor, source_prop);
                    (true, body.is_empty())
                } else {
                    (false, false)
                }
            }
            _ => (false, false),
        };

        if is_match {
            if is_empty_after {
                stmts.remove(i);
            }
            return true;
        }
    }

    false
}

fn remove_assignment_from_body(
    body: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
) {
    body.retain(|stmt| !matches!(stmt,
        Stmt::Assignment { target, property: prop, .. }
            if target.iter().any(|t| t == actor) && prop == property
    ));
}

// ---------------------------------------------------------------------------
// InsertKeyframe
// ---------------------------------------------------------------------------

pub(super) fn insert_keyframe(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
    prev_time_s: f64,
) -> bool {
    let delta_s = time_s - prev_time_s;
    if delta_s < 0.001 {
        return false;
    }

    let source_prop = canonical_to_source(property);

    // Format the time offset.
    let offset = if delta_s < 1.0 {
        Time::Milliseconds((delta_s * 1000.0).round() as u64)
    } else {
        Time::Seconds(delta_s)
    };

    let assignment = Stmt::Assignment {
        target: vec![actor.into()],
        property: source_prop.into(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    };

    let keyframe = Stmt::RelativeKeyframe {
        offset,
        body: vec![assignment],
        span: None,
    };

    // Insert after the keyframe that contains prev_time_s, or at the end.
    let mut insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // If there are no keyframes before the insertion point and prev_time_s is ~0,
    // wrap any leading top-level declarations in a #0s keyframe so they don't
    // get shifted to a later time by the new relative keyframe.
    if insert_idx == 0 && prev_time_s < 0.001 && !stmts.is_empty() {
        let first_is_keyframe = matches!(
            stmts[0],
            Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }
        );
        if !first_is_keyframe {
            let decl_end = stmts
                .iter()
                .position(|s| matches!(s, Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }))
                .unwrap_or(stmts.len());
            if decl_end > 0 {
                let decls: Vec<Stmt> = stmts.drain(0..decl_end).collect();
                let zero_kf = Stmt::Keyframe {
                    time: Time::Seconds(0.0),
                    body: decls,
                    span: None,
                };
                stmts.insert(0, zero_kf);
                insert_idx = 1;
            }
        }
    }

    // If the next statement is a RelativeKeyframe, subtract delta_s from its
    // offset so that subsequent keyframes keep their original absolute times.
    if insert_idx < stmts.len() {
        if let Stmt::RelativeKeyframe { offset: ref mut next_offset, .. } = stmts[insert_idx] {
            let next_delta_s = time_to_seconds(next_offset);
            let new_next_delta_s = next_delta_s - delta_s;
            if new_next_delta_s >= 0.001 {
                *next_offset = if new_next_delta_s < 1.0 {
                    Time::Milliseconds((new_next_delta_s * 1000.0).round() as u64)
                } else {
                    Time::Seconds(new_next_delta_s)
                };
            }
        }
    }

    stmts.insert(insert_idx, keyframe);
    true
}

/// Find the index after which a new keyframe at `time_s` should be inserted.
fn find_keyframe_insertion_point(stmts: &[Stmt], time_s: f64) -> usize {
    let mut last_kf_idx = 0usize;
    let mut current_time = 0.0f64;

    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            _ => {}
        }
    }

    last_kf_idx
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{find_actor_decl_mut, find_assignment_mut, find_prop_mut};
    use super::super::apply::{SourceEdit, apply_edit};
    use animatix::ast::{Expr, Stmt, Time};
    use animatix::parser::parser;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser().parse(source).into_result().expect("failed to parse test source")
    }

    #[test]
    fn insert_keyframe_block() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#);
        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
            time_s: 3.0,
            prev_time_s: 2.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should have 3 top-level keyframes now
        assert_eq!(stmts.len(), 3);

        // The new keyframe should be a RelativeKeyframe after the #2s one
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Seconds(1.0));
            assert_eq!(body.len(), 1);
            if let Stmt::Assignment { target, property, .. } = &body[0] {
                assert_eq!(target, &vec!["btn".to_string()]);
                assert_eq!(property, "color");
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected RelativeKeyframe");
        }
    }

    #[test]
    fn insert_keyframe_wraps_declarations_in_zero_keyframe() {
        // No keyframes at all — inserting a relative keyframe must wrap the
        // top-level declarations in #0s so they don't get shifted.
        let mut stmts = parse(r#"btn: Rect, size: (100, 200)
circle: Ellipse, radius: 50"#);

        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
            time_s: 0.5,
            prev_time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should now be: #0s, #0s, #+500ms (parser wraps each top-level decl in #0s)
        assert_eq!(stmts.len(), 3);

        // First two statements are #0s wrapping each declaration (parser behavior)
        if let Stmt::Keyframe { time, body, .. } = &stmts[0] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe at index 0, got {:?}", stmts[0]);
        }
        if let Stmt::Keyframe { time, body, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe at index 1, got {:?}", stmts[1]);
        }

        // Third statement is the new relative keyframe
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn insert_keyframe_adjusts_subsequent_relative_offset() {
        // Inserting between #0s and #+1s should adjust the #+1s offset to #+500ms
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#+1s
btn.color = red"#);

        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
            time_s: 0.5,
            prev_time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should have 3 top-level statements
        assert_eq!(stmts.len(), 3);

        // New keyframe at index 1
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[1] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 1");
        }

        // Existing keyframe at index 2 — offset should be reduced
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn merge_keyframe_updates_existing_assignment() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.color = red
btn.position = (10, 20)"#);

        let edit = SourceEdit::MergeKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
            time_s: 1.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        let mut found = false;
        if let Stmt::Keyframe { body, .. } = &stmts[1] {
            for stmt in body {
                if let Stmt::Assignment { property, value, .. } = stmt {
                    if property == "color" {
                        assert_eq!(*value, Expr::Ident("blue".into()));
                        found = true;
                    }
                }
            }
        }
        assert!(found);
    }

    #[test]
    fn merge_keyframe_uses_relative_time() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#+500ms
btn.color = red"#);

        let edit = SourceEdit::MergeKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("green".into()),
            time_s: 0.5,
        };
        assert!(apply_edit(&mut stmts, edit));

        if let Stmt::RelativeKeyframe { body, .. } = &stmts[1] {
            if let Stmt::Assignment { value, .. } = &body[0] {
                assert_eq!(*value, Expr::Ident("green".into()));
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected RelativeKeyframe");
        }
    }
}