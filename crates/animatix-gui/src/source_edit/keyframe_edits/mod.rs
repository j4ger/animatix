//! Edits related to keyframes: insert, merge, delete, and easing updates.

use animatix_syntax::ast::{ComponentDef, Expr, Stmt, TargetSegment, Time};

use super::SourceEditError;
use super::apply::{canonical_to_source, time_to_seconds};
use crate::source_edit::ast_utils::{
    adjust_following_relative_keyframe, find_keyframe_insertion_point,
    wrap_leading_decls_in_zero_keyframe,
};
use update::merge_keyframe_inner;

/// Decompose a property key for assignment matching. `"stage.param"` addresses
/// an effect stage parameter: the assignment target is `[scope, stage]` and the
/// assignment property is `param`.
fn split_property(property: &str) -> (Option<&str>, &str) {
    match property.rsplit_once('.') {
        Some((stage, param)) => (Some(stage), param),
        None => (None, property),
    }
}

/// Does an assignment with `target`/`prop` match the `actor` + `property` key?
fn assignment_matches(target: &[TargetSegment], prop: &str, actor: &str, property: &str) -> bool {
    let (stage, param) = split_property(property);
    let actor_ok = target.iter().any(|t| t.label_str() == actor);
    match stage {
        Some(stage) => actor_ok && target.iter().any(|t| t.label_str() == stage) && prop == param,
        None => actor_ok && prop == property,
    }
}

/// Build the assignment target segments and property for `actor` + `property`.
fn assignment_target(actor: &str, property: &str) -> (Vec<TargetSegment>, String) {
    let (stage, param) = split_property(property);
    let mut target = vec![TargetSegment::Static(actor.to_string())];
    if let Some(stage) = stage {
        target.push(TargetSegment::Static(stage.to_string()));
    }
    (target, param.to_string())
}

/// Return the body of a named composition scene, or `None` when absent.
fn scene_body_mut<'a>(stmts: &'a mut [Stmt], scene_name: &str) -> Option<&'a mut Vec<Stmt>> {
    for stmt in stmts {
        if let Stmt::Scene { name, body, .. } = stmt {
            if name == scene_name {
                return Some(body);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// MergeKeyframe
// ---------------------------------------------------------------------------

pub(super) fn merge_keyframe(
    stmts: &mut Vec<Stmt>,
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
    merge_keyframe_inner(target, actor, property, value, time_s)
}

mod easing;
mod insert;
#[cfg(test)]
mod tests;
mod update;

pub(crate) use easing::{delete_keyframe, insert_keyframe};
pub(crate) use insert::move_keyframe_time;
pub(crate) use update::{set_keyframe_easing, set_keyframe_value};
