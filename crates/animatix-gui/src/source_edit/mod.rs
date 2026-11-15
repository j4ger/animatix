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

pub use apply::{SourceEdit, apply_edit};
pub use error::SourceEditError;
