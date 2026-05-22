//! AST-based source editing for the GUI inspector.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix::to_source::stmts_to_source`].
//!
//! This module has been split into sub-modules:
//! - `apply` — core `SourceEdit` enum, `apply_edit` dispatch, shared traversal helpers
//! - `actor_edits` — property changes, actor insertion, reordering, reparenting, renaming
//! - `keyframe_edits` — keyframe insert/merge/delete/easing
//! - `scene_edits` — scene reorder/play/transition/rename/add/delete/refactor

mod apply;
mod actor_edits;
mod keyframe_edits;
mod scene_edits;

// Re-export public API
pub use apply::{apply_edit, canonical_to_source, find_actor_decl, source_to_canonical, SourceEdit};
pub(crate) use actor_edits::rename_all_references;