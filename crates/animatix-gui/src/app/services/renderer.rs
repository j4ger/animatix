//! Preview renderer service trait.
//!
//! Implemented by `PreviewSurface` (Vello/WGPU) for the eframe runtime.
//! Mock implementations enable testing without GPU hardware.

use crate::app::document::snapshot::DocumentSnapshot;
use animatix::timeline::SceneDimensions;

/// A render request for the preview surface.
#[allow(dead_code)] // Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
/// Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
pub struct RenderRequest<'a> {
    pub snapshot: &'a DocumentSnapshot,
    pub active_scene: Option<&'a str>,
    pub time_s: f64,
    pub dimensions: SceneDimensions,
}

/// Result of a render operation.
#[allow(dead_code)] // Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
/// Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
pub struct RenderResult {
    pub hit_regions: Vec<(String, kurbo::Rect)>,
    pub frame_pixels: Option<Vec<u8>>,
}

/// Error from the renderer.
#[allow(dead_code)] // Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
/// Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
#[derive(Debug)]
pub enum RenderServiceError {
    SurfaceLost,
    RenderFailed(String),
    UnsupportedDimensions,
}

/// Trait for rendering preview frames.
///
/// The shell calls `render()` each frame with the current document snapshot
/// and playback state. The renderer produces hit regions and (optionally)
/// pixel data for screenshot/export.
#[allow(dead_code)] // Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
/// Service traits decouple shell from WGPU; implementations are in preview_surface.rs.
pub trait PreviewRenderer {
    /// Render a frame. Returns hit regions for the rendered frame.
    fn render(&mut self, request: RenderRequest<'_>) -> Result<RenderResult, RenderServiceError>;

    /// Get the egui texture ID for the last rendered frame.
    fn texture_id(&self) -> Option<egui::TextureId>;

    /// Resize the render surface for new document dimensions.
    fn resize(&mut self, width: u32, height: u32);

    /// Return the current surface dimensions.
    fn dimensions(&self) -> (u32, u32);
}
