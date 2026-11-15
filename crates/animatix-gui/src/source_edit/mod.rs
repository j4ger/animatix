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
