use super::insert::insert_keyframe_inner;
use super::*;

pub(super) fn set_keyframe_easing_inner(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    time_s: f64,
    easing: animatix_syntax::easing::Easing,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);
    // The name table this used to carry was a fourth copy of the easing names,
    // in unhyphenated forms the hand-written corpus never uses, and its
    // `CubicBezier` arm overwrote the user's curve with `linear`.
    let easing_expr = animatix_syntax::easing::easing_to_expr(easing);

    // Walk through keyframes looking for the match
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            },
            _ => {},
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
            Stmt::Assignment {
                target,
                property: prop,
                modifiers,
                ..
            } if assignment_matches(target, prop, actor, property) => {
                if let Some(existing) =
                    modifiers.iter_mut().find(|m| m.name.as_deref() == Some("ease"))
                {
                    existing.value = easing_expr.clone();
                } else {
                    modifiers.push(animatix_syntax::ast::Modifier {
                        name: Some("ease".into()),
                        value: easing_expr.clone(),
                    });
                }
                return Ok(());
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::FnDecl { body, .. }
            | Stmt::Block { body, .. } => {
                if let Ok(()) = update_assignment_easing(body, actor, property, easing_expr) {
                    return Ok(());
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                if let Ok(()) = update_assignment_easing(then_branch, actor, property, easing_expr)
                {
                    return Ok(());
                }
                if let Some(else_b) = else_branch {
                    if let Ok(()) = update_assignment_easing(else_b, actor, property, easing_expr) {
                        return Ok(());
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                if let Ok(()) = update_assignment_easing(body, actor, property, easing_expr) {
                    return Ok(());
                }
            },
            _ => {},
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

pub(crate) fn delete_keyframe(
    stmts: &mut Vec<Stmt>,
    scene: Option<&str>,
    actor: &str,
    property: &str,
    time_s: f64,
) -> Result<(), SourceEditError> {
    let target = match scene {
        Some(name) => match scene_body_mut(stmts, name) {
            Some(body) => body,
            None => {
                return Err(SourceEditError::KeyframeNotFound {
                    actor: actor.to_string(),
                    property: property.to_string(),
                    time_s,
                });
            },
        },
        None => stmts,
    };
    delete_keyframe_inner(target, actor, property, time_s)
}

fn delete_keyframe_inner(
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
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    remove_assignment_from_body(body, actor, source_prop);
                    (true, body.is_empty())
                } else {
                    (false, false)
                }
            },
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

fn remove_assignment_from_body(body: &mut Vec<Stmt>, actor: &str, property: &str) {
    body.retain(|stmt| {
        !matches!(stmt,
            Stmt::Assignment { target, property: prop, .. }
                if assignment_matches(target, prop, actor, property)
        )
    });
}

// ---------------------------------------------------------------------------
// InsertKeyframe
// ---------------------------------------------------------------------------

pub(crate) fn insert_keyframe(
    stmts: &mut Vec<Stmt>,
    scene: Option<&str>,
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
    prev_time_s: f64,
) -> Result<(), SourceEditError> {
    let target = match scene {
        Some(name) => match scene_body_mut(stmts, name) {
            Some(body) => body,
            None => {
                return Err(SourceEditError::KeyframeNotFound {
                    actor: actor.to_string(),
                    property: property.to_string(),
                    time_s,
                });
            },
        },
        None => stmts,
    };
    insert_keyframe_inner(target, actor, property, value, time_s, prev_time_s)
}
