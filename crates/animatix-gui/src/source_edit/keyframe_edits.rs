//! Edits related to keyframes: insert, merge, delete, and easing updates.

use animatix_syntax::ast::{ComponentDef, Expr, Stmt, Time};

use super::apply::time_to_seconds;
use super::apply::canonical_to_source;
use super::ast_utils::{
    adjust_following_relative_keyframe, find_keyframe_insertion_point,
    wrap_leading_decls_in_zero_keyframe,
};
use super::SourceEditError;

// ---------------------------------------------------------------------------
// MergeKeyframe
// ---------------------------------------------------------------------------

pub(super) fn merge_keyframe(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
) -> Result<(), SourceEditError> {
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

    Err(SourceEditError::KeyframeNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
        time_s,
    })
}

fn update_assignment(body: &mut [Stmt], actor: &str, property: &str, value: Expr) -> Result<(), SourceEditError> {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, value: val, .. }
                if target.iter().any(|t| t.label_str() == actor) && prop == property =>
            {
                *val = value;
                return Ok(());
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Ok(()) = update_assignment(body, actor, property, value.clone()) {
                    return Ok(());
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Ok(()) = update_assignment(then_branch, actor, property, value.clone()) {
                    return Ok(());
                }
                if let Some(else_b) = else_branch {
                    if let Ok(()) = update_assignment(else_b, actor, property, value.clone()) {
                        return Ok(());
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Ok(()) = update_assignment(body, actor, property, value.clone()) {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Err(SourceEditError::PropertyNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
    })
}

// ---------------------------------------------------------------------------
// SetKeyframeEasing
// ---------------------------------------------------------------------------

pub(super) fn set_keyframe_easing(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    time_s: f64,
    easing: animatix_syntax::easing::Easing,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);
    let easing_name = match easing {
        animatix_syntax::easing::Easing::Linear => "linear",
        animatix_syntax::easing::Easing::EaseIn => "easein",
        animatix_syntax::easing::Easing::EaseOut => "easeout",
        animatix_syntax::easing::Easing::EaseInOut => "easeinout",
        animatix_syntax::easing::Easing::Bounce => "bounce",
        animatix_syntax::easing::Easing::Elastic => "elastic",
        animatix_syntax::easing::Easing::Back => "back",
        animatix_syntax::easing::Easing::Expo => "expo",
        animatix_syntax::easing::Easing::CubicBezier(cp) => {
            // Serialize custom easing as a special ident that the parser
            // will handle; for now fall back to linear in source edits.
            let _ = cp;
            "linear"
        }
    };
    let easing_expr = animatix_syntax::ast::Expr::Ident(easing_name.to_string());

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

    Err(SourceEditError::KeyframeNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
        time_s,
    })
}

/// Walk into an assignment at the given time and set its easing modifier.
fn update_assignment_easing(
    body: &mut [Stmt],
    actor: &str,
    property: &str,
    easing_expr: &animatix_syntax::ast::Expr,
) -> Result<(), SourceEditError> {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, modifiers, .. }
                if target.iter().any(|t| t.label_str() == actor) && prop == property =>
            {
                if let Some(existing) = modifiers.iter_mut().find(|m| m.name.as_deref() == Some("ease")) {
                    existing.value = easing_expr.clone();
                } else {
                    modifiers.push(animatix_syntax::ast::Modifier {
                        name: Some("ease".into()),
                        value: easing_expr.clone(),
                    });
                }
                return Ok(());
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Ok(()) = update_assignment_easing(body, actor, property, easing_expr) {
                    return Ok(());
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Ok(()) = update_assignment_easing(then_branch, actor, property, easing_expr) {
                    return Ok(());
                }
                if let Some(else_b) = else_branch {
                    if let Ok(()) = update_assignment_easing(else_b, actor, property, easing_expr) {
                        return Ok(());
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Ok(()) = update_assignment_easing(body, actor, property, easing_expr) {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Err(SourceEditError::PropertyNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
    })
}

// ---------------------------------------------------------------------------
// DeleteKeyframe
// ---------------------------------------------------------------------------

pub(super) fn delete_keyframe(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
    time_s: f64,
) -> Result<(), SourceEditError> {
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
            return Ok(());
        }
    }

    Err(SourceEditError::KeyframeNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
        time_s,
    })
}

fn remove_assignment_from_body(
    body: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
) {
    body.retain(|stmt| !matches!(stmt,
        Stmt::Assignment { target, property: prop, .. }
            if target.iter().any(|t| t.label_str() == actor) && prop == property
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
) -> Result<(), SourceEditError> {
    let delta_s = time_s - prev_time_s;
    if delta_s < 0.001 {
        return Err(SourceEditError::InvalidKeyframeTime { time_s });
    }

    let source_prop = canonical_to_source(property);

    // Format the time offset.
    let offset = if delta_s < 1.0 {
        Time::Milliseconds((delta_s * 1000.0).round() as u64)
    } else {
        Time::Seconds(delta_s)
    };

    let assignment = Stmt::Assignment {
        target: vec![animatix_syntax::ast::TargetSegment::Static(actor.to_string())],
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
    let insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // Wrap leading declarations in #0s if needed
    let insert_idx = wrap_leading_decls_in_zero_keyframe(stmts, insert_idx, prev_time_s);

    // Adjust next relative keyframe to preserve its absolute time
    adjust_following_relative_keyframe(stmts, insert_idx, delta_s);

    stmts.insert(insert_idx, keyframe);
    Ok(())
}


// ---------------------------------------------------------------------------
// MoveKeyframeTime
// ---------------------------------------------------------------------------

pub(super) fn move_keyframe_time(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    old_time_s: f64,
    new_time_s: f64,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);
    let mut current_time = 0.0f64;
    let mut found_idx: Option<usize> = None;
    let mut found_old_time = 0.0f64;

    // First pass: find the keyframe index and compute absolute times
    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - old_time_s).abs() < 0.001
                    && contains_assignment(body, actor, source_prop)
                {
                    found_idx = Some(i);
                    found_old_time = current_time;
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - old_time_s).abs() < 0.001
                    && contains_assignment(body, actor, source_prop)
                {
                    found_idx = Some(i);
                    found_old_time = current_time;
                }
            }
            _ => {}
        }
    }

    let idx = match found_idx {
        Some(i) => i,
        None => return Err(SourceEditError::KeyframeNotFound {
            actor: actor.to_string(),
            property: property.to_string(),
            time_s: old_time_s,
        }),
    };

    let delta_s = new_time_s - found_old_time;
    if delta_s.abs() < 0.001 {
        return Ok(()); // No change needed
    }

    // Pre-compute flash indices before any mutation.
    let mut flash_indices = Vec::new();
    if matches!(stmts[idx], Stmt::RelativeKeyframe { .. })
        && idx + 1 < stmts.len() && matches!(stmts[idx + 1], Stmt::RelativeKeyframe { .. })
    {
        flash_indices.push(idx + 1);
    }
    if matches!(stmts[idx], Stmt::Keyframe { .. }) {
        for (j, stmt) in stmts[(idx + 1)..].iter().enumerate() {
            if matches!(stmt, Stmt::RelativeKeyframe { .. }) {
                flash_indices.push(idx + 1 + j);
                break;
            }
            if matches!(stmt, Stmt::Keyframe { .. }) {
                break;
            }
        }
    }
    for fi in &flash_indices {
        let t = super::ast_utils::compute_keyframe_abs_time(stmts, *fi);
        super::ast_utils::push_adjust_flash_time(t);
    }

    // Update the found keyframe's time
    match &mut stmts[idx] {
        Stmt::Keyframe { time, .. } => {
            *time = if new_time_s < 1.0 {
                Time::Milliseconds((new_time_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(new_time_s)
            };
        }
        Stmt::RelativeKeyframe { offset, .. } => {
            // For relative keyframes, adjust this offset by delta_s
            let new_offset_s = time_to_seconds(offset) + delta_s;
            if new_offset_s < 0.001 {
                return Err(SourceEditError::InvalidKeyframeTime { time_s: new_time_s });
            }
            *offset = if new_offset_s < 1.0 {
                Time::Milliseconds((new_offset_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(new_offset_s)
            };
            // Adjust next relative keyframe's offset to compensate
            if idx + 1 < stmts.len() {
                if let Stmt::RelativeKeyframe { offset: next_offset, .. } = &mut stmts[idx + 1] {
                    let next_offset_s = time_to_seconds(next_offset) - delta_s;
                    if next_offset_s >= 0.001 {
                        *next_offset = if next_offset_s < 1.0 {
                            Time::Milliseconds((next_offset_s * 1000.0).round() as u64)
                        } else {
                            Time::Seconds(next_offset_s)
                        };
                    }
                }
            }
        }
        _ => return Err(SourceEditError::InvalidKeyframeTime { time_s: new_time_s }),
    }

    // If we changed an absolute keyframe, adjust the first subsequent
    // relative keyframe's offset so its absolute time stays the same.
    if matches!(stmts[idx], Stmt::Keyframe { .. }) {
        for stmt in stmts[(idx + 1)..].iter_mut() {
            if let Stmt::RelativeKeyframe { offset, .. } = stmt {
                let offset_s = time_to_seconds(offset) - delta_s;
                if offset_s >= 0.001 {
                    *offset = if offset_s < 1.0 {
                        Time::Milliseconds((offset_s * 1000.0).round() as u64)
                    } else {
                        Time::Seconds(offset_s)
                    };
                }
                break; // Only adjust the first subsequent relative keyframe
            }
            if matches!(stmt, Stmt::Keyframe { .. }) {
                break; // Stop at next absolute keyframe
            }
        }
    }

    Ok(())
}

/// Check if any assignment in the statement tree matches the given actor + property.
fn contains_assignment(body: &[Stmt], actor: &str, property: &str) -> bool {
    for stmt in body {
        match stmt {
            Stmt::Assignment { target, property: prop, .. }
                if target.iter().any(|t| t.label_str() == actor) && prop == property =>
            {
                return true;
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if contains_assignment(body, actor, property) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if contains_assignment(then_branch, actor, property) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if contains_assignment(else_b, actor, property) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if contains_assignment(body, actor, property) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{SourceEdit, apply_edit};
    use animatix_syntax::ast::{Expr, Stmt, Time};
    use animatix_syntax::parser::parser_simple;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser_simple().parse(source).into_result().expect("failed to parse test source")
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
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // Should have 3 top-level keyframes now
        assert_eq!(stmts.len(), 3);

        // The new keyframe should be a RelativeKeyframe after the #2s one
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Seconds(1.0));
            assert_eq!(body.len(), 1);
            if let Stmt::Assignment { target, property, .. } = &body[0] {
                assert_eq!(
                    target,
                    &vec![animatix_syntax::ast::TargetSegment::Static("btn".to_string())]
                );
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
        // top-level declarations in a single #0s so they don't get shifted.
        let mut stmts = parse(r#"btn: Rect, size: (100, 200)
circle: Ellipse, radius: 50"#);

        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
            time_s: 0.5,
            prev_time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // All leading declarations are wrapped in one #0s, then the new relative keyframe
        assert_eq!(stmts.len(), 2);

        if let Stmt::Keyframe { time, body, .. } = &stmts[0] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected Keyframe at index 0, got {:?}", stmts[0]);
        }

        // Second statement is the new relative keyframe
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[1] {
            assert_eq!(*offset, Time::Milliseconds(500));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected RelativeKeyframe at index 1");
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
        assert!(apply_edit(&mut stmts, edit).is_ok());

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
        assert!(apply_edit(&mut stmts, edit).is_ok());

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
        assert!(apply_edit(&mut stmts, edit).is_ok());

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

    #[test]
    fn move_keyframe_time_absolute() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red

#+2s
btn.color = blue"#);

        let edit = SourceEdit::MoveKeyframeTime {
            actor: "btn".into(),
            property: "color".into(),
            old_time_s: 2.0,
            new_time_s: 3.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // First keyframe should now be at 3s
        if let Stmt::Keyframe { time, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(3.0));
        } else {
            panic!("Expected Keyframe at index 1");
        }

        // The relative keyframe after it should have its offset adjusted
        // Old: 2s + 2s = 4s. New: 3s + offset = 4s → offset = 1s
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Seconds(1.0));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn move_keyframe_time_relative() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#+2s
btn.color = red

#+3s
btn.color = blue"#);

        let edit = SourceEdit::MoveKeyframeTime {
            actor: "btn".into(),
            property: "color".into(),
            old_time_s: 2.0, // 0s + 2s
            new_time_s: 3.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // First relative keyframe should now have offset 3s
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[1] {
            assert_eq!(*offset, Time::Seconds(3.0));
        } else {
            panic!("Expected RelativeKeyframe at index 1");
        }

        // Second relative keyframe: old total was 0+2+3=5s.
        // New: first is at 3s, so second should stay at 5s → offset = 2s
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Seconds(2.0));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn move_keyframe_time_no_change() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#);

        let edit = SourceEdit::MoveKeyframeTime {
            actor: "btn".into(),
            property: "color".into(),
            old_time_s: 2.0,
            new_time_s: 2.0, // same time
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // Should still be at 2s
        if let Stmt::Keyframe { time, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(2.0));
        } else {
            panic!("Expected Keyframe at index 1");
        }
    }

    #[test]
    fn move_keyframe_time_not_found() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#);

        let edit = SourceEdit::MoveKeyframeTime {
            actor: "btn".into(),
            property: "color".into(),
            old_time_s: 999.0, // doesn't exist
            new_time_s: 3.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_err());
    }

    #[test]
    fn move_keyframe_time_wrong_actor() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#);

        let edit = SourceEdit::MoveKeyframeTime {
            actor: "other".into(),
            property: "color".into(),
            old_time_s: 2.0,
            new_time_s: 3.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_err());
    }
}
