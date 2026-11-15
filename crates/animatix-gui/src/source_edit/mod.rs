//! Semantic AST edits, serialized with [`animatix_syntax::to_source::stmts_to_source`].
//! Retained for internal callers; the workbench blocks inspector/canvas mutations.

mod action_edits;
mod actor_edits;
mod apply;
mod ast_utils;
mod config_edits;
mod error;
mod keyframe_edits;
mod scene_edits;

#[cfg(test)]
mod test_support;

pub(crate) use apply::find_actor_decl;
pub use apply::{SourceEdit, apply_edit};
pub(crate) use ast_utils::{clear_adjust_flash_queue, drain_adjust_flash_queue};
pub use error::SourceEditError;
