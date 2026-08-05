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

mod action_edits;
mod actor_edits;
mod apply;
mod ast_utils;
mod config_edits;
mod error;
mod keyframe_edits;
mod scene_edits;

// Re-export public API
pub(crate) use actor_edits::rename_all_references;
pub use apply::{
    SourceEdit, apply_edit, canonical_to_source, find_actor_decl, source_to_canonical,
};
pub use ast_utils::{
    KeyframeStyle, adjust_following_relative_keyframe, append_to_keyframe_at_time,
    clear_adjust_flash_queue, compute_keyframe_abs_time, drain_adjust_flash_queue,
    find_keyframe_insertion_point, find_keyframes_for_actor, find_prev_keyframe_time,
    keyframe_references_actor, keyframe_style_before, push_adjust_flash_time, shift_keyframe_times,
    wrap_leading_decls_in_zero_keyframe,
};
pub use error::SourceEditError;
