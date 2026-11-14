pub mod context;
pub mod gesture;
pub mod gesture_router;
pub mod gestures;
pub mod overlay;
pub mod overlay_ops;
pub mod performance;
pub mod selection;
pub mod time_lens;

use std::collections::HashSet;

use animatix::timeline::SceneDimensions;
use egui::Vec2;

use super::DEFAULT_PREVIEW_SIZE;

// ─── Preview Transform ──────────────────────────────────────────────────────

/// Coordinate-space conversion state for the preview canvas, built once and
/// passed by reference instead of threading `zoom`/`pan`/`preview_rect` through
/// every helper.
#[derive(Clone, Copy, Debug)]
pub struct PreviewTransform {
    pub scene_dimensions: SceneDimensions,
    pub preview_rect: egui::Rect,
    pub zoom: f32,
    pub pan: Vec2,
}

// ─── Drag State ─────────────────────────────────────────────────────────────

/// Active tool mode; handles work in every mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolMode {
    /// Auto-detect interaction based on cursor position (default).
    #[default]
    Select,
    Move,
    Scale,
    Rotate,
    Vertex,
    Pivot,
}

mod handles;
mod impls;
#[cfg(test)]
mod tests;

pub(crate) use handles::*;
