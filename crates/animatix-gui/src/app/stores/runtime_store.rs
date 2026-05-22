use crate::preview_surface::PreviewSurface;
use std::time::{Duration, Instant};

/// Owns the preview pane state, the WGPU render surface, and the timing
/// infrastructure (playback, scrubbing, pending rebuilds).
pub struct RuntimeStore {
    pub preview: crate::app::PreviewPaneState,
    pub preview_surface: PreviewSurface,
    pub preview_dirty: bool,
    pub last_reload_time: Option<Instant>,
    pub pending_rebuild_at: Option<Instant>,
    pub rebuild_debounce_ms: u64,
}
