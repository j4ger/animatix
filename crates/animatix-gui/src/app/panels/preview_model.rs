//! View model for the preview panel.
//!
//! Constructed by the shell before each frame, consumed by the panel.

use std::collections::HashMap;
use kurbo::Rect;

use crate::app::document::active_timeline::ActiveTimelineRef;
use crate::app::preview::{DragState, ToolMode, selection::SelectionState};
use crate::app::PreviewPaneState;
use animatix::timeline::SceneDimensions;

/// Immutable view model for the preview panel.
pub struct PreviewPanelModel<'a> {
    pub scene_dimensions: SceneDimensions,
    pub preview: &'a PreviewPaneState,
    pub preview_texture_id: Option<egui::TextureId>,
    pub timeline: Option<ActiveTimelineRef<'a>>,
    pub hit_regions: &'a [(String, Rect)],
    pub pivot_offsets: &'a HashMap<String, [f32; 2]>,
    pub tool_mode: ToolMode,
    pub rotation_snap_degrees: f32,
    pub keyframe_mode: bool,
}
