//! Shared keyframe-target resolution for the keyframe edits.

use animatix_syntax::ast::{Stmt, TargetSegment};

use crate::source_edit::SourceEditError;

/// Decompose a property key for assignment matching. `"stage.param"` addresses
/// an effect stage parameter: the assignment target is `[scope, stage]` and the
/// assignment property is `param`.
pub(super) fn split_property(property: &str) -> (Option<&str>, &str) {
    match property.rsplit_once('.') {
        Some((stage, param)) => (Some(stage), param),
        None => (None, property),
    }
}

/// Does an assignment with `target`/`prop` match the `actor` + `property` key?
pub(super) fn assignment_matches(
    target: &[TargetSegment],
    prop: &str,
    actor: &str,
    property: &str,
) -> bool {
    let (stage, param) = split_property(property);
    let actor_ok = target.iter().any(|t| t.label_str() == actor);
    match stage {
        Some(stage) => actor_ok && target.iter().any(|t| t.label_str() == stage) && prop == param,
        None => actor_ok && prop == property,
    }
}

/// Build the assignment target segments and property for `actor` + `property`.
pub(super) fn assignment_target(actor: &str, property: &str) -> (Vec<TargetSegment>, String) {
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
        if let Stmt::Scene { name, body, .. } = stmt
            && name == scene_name
        {
            return Some(body);
        }
    }
    None
}

/// The shared "this actor/property has no keyframe at that time" error.
pub(super) fn keyframe_not_found(actor: &str, property: &str, time_s: f64) -> SourceEditError {
    SourceEditError::KeyframeNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
        time_s,
    }
}

/// Resolve the statement body a keyframe edit applies to: the named scene's
/// body when a scene is given, otherwise the top-level statements.
pub(super) fn resolve_body<'a>(
    stmts: &'a mut Vec<Stmt>,
    scene: Option<&str>,
    actor: &str,
    property: &str,
    time_s: f64,
) -> Result<&'a mut Vec<Stmt>, SourceEditError> {
    match scene {
        Some(name) => {
            scene_body_mut(stmts, name).ok_or_else(|| keyframe_not_found(actor, property, time_s))
        },
        None => Ok(stmts),
    }
}
