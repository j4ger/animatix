//! Unified overlay toggle system for the preview canvas.
//!
//! All preview overlays (grid, guides, labels, etc.) are controlled through
//! [`PreviewOverlay`] which lives in [`PreviewPaneState`](crate::app::PreviewPaneState).

/// Toggle-able overlays for the preview canvas.
#[derive(Debug, Clone)]
pub struct PreviewOverlay {
    /// Show scene bounds outline.
    pub show_scene_bounds: bool,
    /// Show grid overlay.
    pub show_grid: bool,
    /// Show ruler guides (horizontal/vertical drag-from-ruler guides).
    pub show_guides: bool,
    /// Show actor name labels near selected actors.
    pub show_actor_labels: bool,
    /// Show snap guides during drag.
    pub show_snap_guides: bool,
    /// Show hover highlight around hovered actors.
    pub show_hover_highlight: bool,
    /// Show motion paths for position keyframes.
    pub show_motion_paths: bool,
    /// Grid size in pixels.
    pub grid_size: f32,
}

impl Default for PreviewOverlay {
    fn default() -> Self {
        Self {
            show_scene_bounds: true,
            show_grid: true,
            show_guides: true,
            show_actor_labels: false,
            show_snap_guides: true,
            show_hover_highlight: true,
            show_motion_paths: true,
            grid_size: 20.0,
        }
    }
}
