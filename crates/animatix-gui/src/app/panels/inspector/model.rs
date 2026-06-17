//! View model for the inspector panel.

use std::collections::HashMap;

use crate::app::PreviewPaneState;
use crate::app::panels::inspector::{KeyframeViewMode, PropertyViewMode};
use animatix::timeline::SceneDimensions;

/// Immutable view model for the inspector panel.
#[allow(dead_code)] // View model for panel migration (R7); panels still use mutable context.
/// View model for panel migration (R7); panels still use mutable context.
pub struct InspectorModel<'a> {
    pub preview: &'a PreviewPaneState,
    pub timeline: Option<&'a animatix::timeline::Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a std::collections::HashSet<String>,
    pub keyframe_mode: bool,
    pub scene_dimensions: SceneDimensions,
    pub pivot_offsets: &'a HashMap<String, [f32; 2]>,
    pub property_view_mode: PropertyViewMode,
    pub keyframe_view_mode: KeyframeViewMode,
}
