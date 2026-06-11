use crate::app::document::rebuild::RebuildToken;
use crate::app::PreviewPaneState;
use std::time::Instant;

/// Owns the preview pane state, playback timing, and pending rebuild scheduling.
pub struct PreviewStore {
    pub preview: PreviewPaneState,
    pub preview_dirty: bool,
    pub pending_rebuild_at: Option<Instant>,
    pub last_frame_at: Instant,
    /// True while a timeline rebuild is running. Shown in preview status (Phase 6.5).
    pub rebuild_in_progress: bool,
    /// Token of the latest in-flight background rebuild, if any.
    pub in_flight_rebuild: Option<RebuildToken>,
}

impl PreviewStore {
    pub fn new(preview: PreviewPaneState) -> Self {
        Self {
            preview,
            preview_dirty: true,
            pending_rebuild_at: None,
            last_frame_at: Instant::now(),
            rebuild_in_progress: false,
            in_flight_rebuild: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.preview.playback.is_playing
    }

    pub fn has_pending_rebuild(&self) -> bool {
        self.pending_rebuild_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix::timeline::SceneDimensions;

    #[test]
    fn preview_store_new_creates_valid_store() {
        let preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        let store = PreviewStore::new(preview);

        assert!(store.preview_dirty);
        assert!(store.pending_rebuild_at.is_none());
    }

    #[test]
    fn preview_pane_state_defaults_current_time_s() {
        let preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        assert_eq!(preview.playback.current_time_s(), 0.0);
    }

    #[test]
    fn preview_pane_state_defaults_is_playing_false() {
        let preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        assert!(!preview.playback.is_playing);
    }

    #[test]
    fn preview_pane_state_defaults_playback_speed() {
        let preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        assert_eq!(preview.playback.playback_speed, 1.0);
    }

    #[test]
    fn preview_store_is_playing_delegates_to_preview() {
        let preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        let store = PreviewStore::new(preview);
        assert!(!store.is_playing());

        let mut playing_preview = PreviewPaneState::new(5.0, SceneDimensions { width: 1920, height: 1080 });
        playing_preview.playback.is_playing = true;
        let store = PreviewStore::new(playing_preview);
        assert!(store.is_playing());
    }
}