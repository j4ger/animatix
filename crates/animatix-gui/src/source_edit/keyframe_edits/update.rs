use super::easing::set_keyframe_easing_inner;
use super::*;

pub(super) fn merge_keyframe_inner(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
) -> Result<(), SourceEditError> {
    with_keyframe_block(stmts, actor, property, time_s, move |body| {
        update_assignment(body, actor, canonical_to_source(property), value)
    })
}

/// Run `f` against the body of the keyframe block whose time is `time_s`,
/// exactly.
///
/// Unlike the `keyframe_merge_window_s` merge heuristic used by property edits,
/// this targets the requested time precisely: the drag of an existing keyframe
/// must edit that keyframe's block, never a neighbour within the merge window.
fn with_keyframe_block<T>(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    time_s: f64,
    f: impl FnOnce(&mut [Stmt]) -> Result<T, SourceEditError>,
) -> Result<T, SourceEditError> {
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return f(body);
                }
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return f(body);
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

// ---------------------------------------------------------------------------
// SetKeyframeValue
// ---------------------------------------------------------------------------

/// Replace the RHS of `actor.property` inside the keyframe block at `time_s`.
///
/// This is an exact edit: it does not insert, move, or merge any keyframe. It
/// is the graph editor's vertical-drag write-back.
pub(crate) fn set_keyframe_value(
    stmts: &mut [Stmt],
    scene: Option<&str>,
    actor: &str,
    property: &str,
    value: Expr,
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
    with_keyframe_block(target, actor, property, time_s, move |body| {
        update_assignment(body, actor, canonical_to_source(property), value)
    })
}

fn update_assignment(
    body: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
) -> Result<(), SourceEditError> {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment {
                target,
                property: prop,
                value: val,
                ..
            } if assignment_matches(target, prop, actor, property) => {
                *val = value;
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
                if let Ok(()) = update_assignment(body, actor, property, value.clone()) {
                    return Ok(());
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                if let Ok(()) = update_assignment(then_branch, actor, property, value.clone()) {
                    return Ok(());
                }
                if let Some(else_b) = else_branch {
                    if let Ok(()) = update_assignment(else_b, actor, property, value.clone()) {
                        return Ok(());
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                if let Ok(()) = update_assignment(body, actor, property, value.clone()) {
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
// SetKeyframeEasing
// ---------------------------------------------------------------------------

pub(crate) fn set_keyframe_easing(
    stmts: &mut [Stmt],
    scene: Option<&str>,
    actor: &str,
    property: &str,
    time_s: f64,
    easing: animatix_syntax::easing::Easing,
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
    set_keyframe_easing_inner(target, actor, property, time_s, easing)
}
