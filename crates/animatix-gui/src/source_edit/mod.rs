//! AST-based source editing for the GUI inspector.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix_syntax::to_source::stmts_to_source`].
//!
//! This module has been split into sub-modules:
//! - `apply` — core `SourceEdit` enum, `apply_edit` dispatch, shared traversal helpers
//! - `actor_edits` — property changes, actor insertion, reordering, reparenting, renaming
//! - `keyframe_edits` — keyframe insert/merge/delete/easing
//! - `scene_edits` — scene reorder/play/transition/rename/add/delete/refactor

mod apply;
mod actor_edits;
mod ast_utils;
mod keyframe_edits;
mod scene_edits;
mod action_edits;
mod config_edits;
mod error;

// Re-export public API
pub use apply::{apply_edit, canonical_to_source, find_actor_decl, source_to_canonical, SourceEdit};
pub use error::SourceEditError;
pub(crate) use actor_edits::rename_all_references;
pub use ast_utils::{
    find_keyframes_for_actor, keyframe_references_actor, shift_keyframe_times,
    KeyframeStyle, keyframe_style_before, find_keyframe_insertion_point,
    find_prev_keyframe_time, wrap_leading_decls_in_zero_keyframe,
    adjust_following_relative_keyframe, append_to_keyframe_at_time,
};