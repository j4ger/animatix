use super::*;

impl PreviewPaneState {
    pub(super) fn new(duration_s: f64, dimensions: SceneDimensions) -> Self {
        Self {
            playback: PlaybackController {
                current_time_s: 0.0,
                duration_s,
                is_playing: false,
                playback_speed: 1.0,
                loop_start_s: None,
                loop_end_s: None,
                ping_pong: false,
                ping_pong_direction: 1,
                fps: 60.0,
                presentation_mode: false,
                pause_points_s: Vec::new(),
                paused_at_step: false,
            },
            viewport: ViewportState {
                preview_zoom: 1.0,
                preview_pan: Vec2::new(
                    dimensions.width as f32 / 2.0,
                    dimensions.height as f32 / 2.0,
                ),
            },
            guides: GuideState {
                horizontal_guides: vec![],
                vertical_guides: vec![],
            },
            snap: SnapState {
                snap_lines_h: vec![],
                snap_lines_v: vec![],
                snap_line_color: None,
                snap_enabled: true,
                snap_threshold: 10.0,
                snap_hud_label: None,
            },
            status: "Loaded file".to_string(),
            status_severity: StatusSeverity::Info,
            error: None,
            dimensions,
            time_lens: crate::app::preview::time_lens::TimeLens::default(),
            overlay: crate::app::preview::overlay::PreviewOverlay::default(),
            timeline_zoom: 1.0,
            timeline_scroll_offset: 0.0,
            fit_zoom_requested: false,
            flashed_keyframe_times: Vec::new(),
            inline_edit: None,
        }
    }

    pub fn set_status_info(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_severity = StatusSeverity::Info;
    }
    pub fn set_status_error(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_severity = StatusSeverity::Error;
    }
}
