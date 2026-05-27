//! AST utility functions: searching/filtering/transforming statement trees.
//!
//! These were extracted from the app module; they operate purely on the AST
//! and belong in the source_edit crate alongside other AST-manipulation helpers.

use animatix::ast::Stmt;

// ---------------------------------------------------------------------------
// Keyframe discovery
// ---------------------------------------------------------------------------

/// Find all top-level keyframe or relative-keyframe statements whose body
/// contains an assignment targeting the given actor label.
pub fn find_keyframes_for_actor(stmts: &[Stmt], actor: &str) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } => {
                if keyframe_references_actor(stmt, actor) {
                    result.push(stmt.clone());
                }
            }
            _ => {}
        }
    }
    result
}

/// Check if a keyframe statement (or any nested child) references the given actor.
pub fn keyframe_references_actor(stmt: &Stmt, actor: &str) -> bool {
    match stmt {
        Stmt::Assignment { target, .. } => target.iter().any(|t| t == actor),
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentDef(animatix::ast::ComponentDef { body, .. }, _)
        | Stmt::ComponentAction { body, .. } => {
            body.iter().any(|child| keyframe_references_actor(child, actor))
        }
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch
                .iter()
                .any(|child| keyframe_references_actor(child, actor))
                || else_branch.as_ref().is_some_and(|eb| {
                    eb.iter()
                        .any(|child| keyframe_references_actor(child, actor))
                })
        }
        Stmt::ForLoop { body, .. } => {
            body.iter()
                .any(|child| keyframe_references_actor(child, actor))
        }
        _ => false,
    }
}

/// Shift absolute keyframe times by `offset_s` seconds. Relative keyframes
/// and non-keyframe statements are left untouched.
pub fn shift_keyframe_times(stmts: &mut [Stmt], offset_s: f64) {
    if offset_s.abs() < 0.001 {
        return;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                let t = match time {
                    animatix::ast::Time::Seconds(s) => *s,
                    animatix::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                };
                let new_t = t + offset_s;
                *time = animatix::ast::Time::Seconds(new_t);
            }
            Stmt::RelativeKeyframe { .. } => {
                // Relative keyframes keep their relative offset
            }
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(animatix::ast::ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                shift_keyframe_times(then_branch, offset_s);
                if let Some(eb) = else_branch {
                    shift_keyframe_times(eb, offset_s);
                }
            }
            Stmt::ForLoop { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            _ => {}
        }
    }
}