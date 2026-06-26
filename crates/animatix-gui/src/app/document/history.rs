//! UI state snapshot for undo/redo.
//!
//! Captures the user's selection, viewport, playback, and tool state
//! alongside source text snapshots.

use std::collections::HashSet;

/// Snapshot of UI state that should be restored on undo/redo.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields beyond active_scene/selected_actors/selected_keyframes/timeline_scroll/tool_mode are still unused
pub struct UiSnapshot {
    pub active_scene: Option<String>,
    pub selected_actors: HashSet<String>,
    pub selected_keyframes: Vec<(String, String, u64)>, // (actor, property, time_ms)
    pub playhead_time_s: f64,
    pub loop_start_s: Option<f64>,
    pub loop_end_s: Option<f64>,
    pub timeline_scroll_offset: f32,
    pub tool_mode: crate::app::preview::ToolMode,
}

impl UiSnapshot {
    pub fn default_with_tool(tool: crate::app::preview::ToolMode) -> Self {
        Self {
            active_scene: None,
            selected_actors: HashSet::new(),
            selected_keyframes: Vec::new(),
            playhead_time_s: 0.0,
            loop_start_s: None,
            loop_end_s: None,
            timeline_scroll_offset: 0.0,
            tool_mode: tool,
        }
    }
}
