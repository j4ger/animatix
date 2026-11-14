//! PreviewContext: shared state and helper methods for the preview canvas.
//!
//! The struct lives here so it can be imported by both the main preview panel
//! UI function (`preview_panel_ui`) and the gesture handlers.

mod camera;
mod handles;
mod overlay;
mod selection;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use animatix::timeline::{SceneDimensions, Timeline};

use crate::app::PreviewPaneState;
use crate::app::commands::ActionQueue;
use crate::app::preview::performance::PerformanceMetrics;
use crate::app::preview::{self, selection as preview_selection};

pub(crate) struct PreviewContext<'a> {
    pub scene_dimensions: SceneDimensions,
    pub preview: &'a mut PreviewPaneState,
    pub preview_texture_id: Option<egui::TextureId>,
    pub commands: &'a mut ActionQueue,
    pub selection: &'a mut preview_selection::SelectionState,
    pub selected_actors: &'a mut HashSet<String>,
    pub hit_regions: &'a [(String, kurbo::Rect)],
    pub timeline: Option<&'a Timeline>,
    pub tool_mode: &'a mut preview::ToolMode,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    /// Performance metrics for the HUD overlay.
    pub performance_metrics: &'a mut PerformanceMetrics,
    /// Show layout debug overlay (container labels, slot outlines, sizes).
    pub debug_layout: bool,
    /// Show padding/gap regions as overlay.
    pub debug_spacing: bool,
    /// True while a timeline rebuild is running.
    pub rebuild_in_progress: bool,
    /// True when there is an active non-empty scene to preview.
    pub has_scene: bool,
    /// Current source text of the document for editor jumps.
    pub source_text: Option<&'a str>,
}
