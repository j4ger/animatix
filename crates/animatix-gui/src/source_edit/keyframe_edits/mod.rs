//! Edits related to keyframes: insert, merge, delete, and easing updates.

use animatix_syntax::ast::{ComponentDef, Expr, Stmt, Time};

use super::SourceEditError;
use super::apply::{canonical_to_source, time_to_seconds};
use crate::source_edit::ast_utils::{
    adjust_following_relative_keyframe, find_keyframe_insertion_point,
    wrap_leading_decls_in_zero_keyframe,
};
use update::merge_keyframe_inner;

use resolve::*;

/// Merge a value into `actor.property` at the keyframe block for `time_s`.
pub(super) fn merge_keyframe(
    stmts: &mut Vec<Stmt>,
    scene: Option<&str>,
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
) -> Result<(), SourceEditError> {
    let target = resolve_body(stmts, scene, actor, property, time_s)?;
    merge_keyframe_inner(target, actor, property, value, time_s)
}

mod easing;
mod insert;
mod resolve;
#[cfg(test)]
mod tests;
mod update;

pub(crate) use easing::{delete_keyframe, insert_keyframe};
pub(crate) use insert::move_keyframe_time;
pub(crate) use update::{set_keyframe_easing, set_keyframe_value};
