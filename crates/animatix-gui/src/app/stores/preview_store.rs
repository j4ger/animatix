use crate::app::PreviewPaneState;
use crate::app::PanelState;
use std::time::Instant;

/// Owns the preview pane state, playback timing, and pending rebuild scheduling.
pub struct PreviewStore {
    pub preview: PreviewPaneState,
    pub panel_state: PanelState,
    pub preview_dirty: bool,
    pub pending_rebuild_at: Option<Instant>,
    pub last_frame_at: Instant,
}

impl PreviewStore {
    pub fn new(preview: PreviewPaneState) -> Self {
        Self {
            preview,
            panel_state: PanelState::default(),
            preview_dirty: true,
            pending_rebuild_at: None,
            last_frame_at: Instant::now(),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.preview.is_playing
    }

    pub fn has_pending_rebuild(&self) -> bool {
        self.pending_rebuild_at.is_some()
    }
}