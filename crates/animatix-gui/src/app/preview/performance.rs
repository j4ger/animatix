//! Performance metrics tracking for the preview HUD.

use std::collections::VecDeque;
use std::time::Instant;

/// Rolling performance metrics for the HUD overlay.
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Current FPS (rolling average of last 60 frame times).
    pub fps: f64,
    /// Last timeline rebuild time in milliseconds.
    pub rebuild_time_ms: f64,
    /// Last render time in milliseconds.
    pub render_time_ms: f64,
    /// Estimated GPU texture memory in MB.
    pub gpu_memory_mb: f64,
    /// Whether the preview is stale (showing cached frame).
    pub is_stale: bool,
    /// Frame time history for sparkline (up to 30 samples).
    pub fps_history: VecDeque<f64>,

    last_frame_time: Option<Instant>,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            fps: 60.0,
            rebuild_time_ms: 0.0,
            render_time_ms: 0.0,
            gpu_memory_mb: 0.0,
            is_stale: false,
            fps_history: VecDeque::with_capacity(30),
            last_frame_time: None,
        }
    }

    /// Record a frame tick. Call once per frame.
    pub fn record_tick(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_frame_time {
            let dt = now.duration_since(last).as_secs_f64();
            if dt > 0.0 {
                let instant_fps = 1.0 / dt;
                // Exponential moving average with alpha=0.1
                self.fps = self.fps * 0.9 + instant_fps * 0.1;

                // Append to history
                if self.fps_history.len() >= 30 {
                    self.fps_history.pop_front();
                }
                self.fps_history.push_back(self.fps);
            }
        }
        self.last_frame_time = Some(now);
    }

    /// Record a rebuild time.
    pub fn record_rebuild(&mut self, duration_ms: f64) {
        self.rebuild_time_ms = duration_ms;
    }

    /// Record a render time.
    pub fn record_render(&mut self, duration_ms: f64) {
        self.render_time_ms = duration_ms;
    }

    /// Set GPU memory estimate.
    pub fn set_gpu_memory(&mut self, mb: f64) {
        self.gpu_memory_mb = mb;
    }

    /// Set stale flag.
    pub fn set_stale(&mut self, stale: bool) {
        self.is_stale = stale;
    }
}