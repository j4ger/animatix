//! UI state snapshot for undo/redo.
//!
//! Captures the user's selection, viewport, and playback state
//! alongside source text snapshots.

use std::collections::HashSet;

use crate::app::document::timeline_diff::KeyframeId;

/// Snapshot of UI state that should be restored on undo/redo.
#[derive(Debug, Clone, Default)]
pub struct UiSnapshot {
    pub active_scene: Option<String>,
    pub selected_actors: HashSet<String>,
    pub selected_keyframes: Vec<KeyframeId>,
    pub playhead_time_s: f64,
    pub loop_start_s: Option<f64>,
    pub loop_end_s: Option<f64>,
    pub timeline_scroll_offset: f64,
}
