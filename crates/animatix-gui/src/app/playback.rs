use super::*;

impl PlaybackController {
    /// Read the current playback time in seconds.
    pub(crate) fn current_time_s(&self) -> f64 {
        self.current_time_s
    }

    /// Jump to an absolute time (clamped to [0, duration]).
    pub(crate) fn scrub_to(&mut self, time_s: f64) {
        self.current_time_s = time_s.clamp(0.0, self.duration_s.max(0.1));
        self.is_playing = false;
    }

    /// Restore playback state captured by an undo/redo snapshot.
    pub(crate) fn restore_snapshot_state(
        &mut self,
        time_s: f64,
        loop_start_s: Option<f64>,
        loop_end_s: Option<f64>,
    ) {
        self.current_time_s = time_s.clamp(0.0, self.duration_s.max(0.1));
        self.loop_start_s = loop_start_s;
        self.loop_end_s = loop_end_s;
        self.is_playing = false;
    }

    pub(super) fn clamp_time(&mut self) {
        let max_duration = self.duration_s.max(0.1);
        self.current_time_s = self.current_time_s.clamp(0.0, max_duration);
    }

    /// Advance by one frame at the given fps. Stops playback.
    pub(crate) fn frame_step_forward(&mut self, fps: f32) {
        let step = 1.0 / fps.max(1.0) as f64;
        self.current_time_s = (self.current_time_s + step).min(self.duration_s);
        self.is_playing = false;
    }

    /// Rewind by one frame at the given fps. Stops playback.
    pub(crate) fn frame_step_backward(&mut self, fps: f32) {
        let step = 1.0 / fps.max(1.0) as f64;
        self.current_time_s = (self.current_time_s - step).max(0.0);
        self.is_playing = false;
    }

    /// Return the current time formatted as HH:MM:SS:FF at the stored fps.
    pub(crate) fn timecode_string(&self) -> String {
        let total_seconds = self.current_time_s.max(0.0);
        let hours = (total_seconds / 3600.0).floor() as u32;
        let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u32;
        let seconds = (total_seconds % 60.0).floor() as u32;
        let frame = ((total_seconds % 1.0) * self.fps as f64).floor() as u32;
        format!("{:02}:{:02}:{:02}:{:02}", hours, minutes, seconds, frame)
    }

    pub(super) fn go_to_next_keyframe(&mut self, keyframes: &[f64]) {
        if keyframes.is_empty() {
            return;
        }
        let next = keyframes
            .iter()
            .find(|&&t| t > self.current_time_s)
            .copied()
            .unwrap_or(self.duration_s);
        self.current_time_s = next;
        self.clamp_time();
        self.is_playing = false;
    }

    pub(super) fn go_to_previous_keyframe(&mut self, keyframes: &[f64]) {
        if keyframes.is_empty() {
            return;
        }
        let prev = keyframes
            .iter()
            .rev()
            .find(|&&t| t < self.current_time_s)
            .copied()
            .unwrap_or(0.0);
        self.current_time_s = prev;
        self.clamp_time();
        self.is_playing = false;
    }

    pub(super) fn toggle_playback(&mut self) {
        if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
        }
        self.is_playing = !self.is_playing;
    }

    pub(super) fn tick(&mut self, delta: std::time::Duration) {
        if !self.is_playing {
            return;
        }

        let dt = delta.as_secs_f64() * self.playback_speed as f64 * self.ping_pong_direction as f64;
        let old_time = self.current_time_s;
        let next_time = old_time + dt;

        if self.presentation_mode
            && dt > 0.0
            && let Some(&p) = self
                .pause_points_s
                .iter()
                .find(|&&p| p > old_time + 0.0001 && p <= next_time + 0.0001)
        {
            self.current_time_s = p;
            self.is_playing = false;
            self.paused_at_step = true;
            return;
        }

        self.current_time_s = next_time;

        // Loop region: if A and B are set, handle boundaries.
        if let (Some(start), Some(end)) = (self.loop_start_s, self.loop_end_s)
            && end > start
        {
            if self.ping_pong {
                if self.current_time_s >= end && self.ping_pong_direction > 0 {
                    self.ping_pong_direction = -1;
                    self.current_time_s = end;
                    return;
                }
                if self.current_time_s <= start && self.ping_pong_direction < 0 {
                    self.ping_pong_direction = 1;
                    self.current_time_s = start;
                    return;
                }
            } else if self.current_time_s >= end {
                self.current_time_s = start;
                // Looping takes priority over end-of-timeline stop.
                return;
            }
        }

        // Natural boundaries (0 and duration_s)
        if self.current_time_s >= self.duration_s {
            if self.ping_pong {
                self.ping_pong_direction = -1;
                self.current_time_s = self.duration_s;
            } else {
                self.current_time_s = self.duration_s;
                self.is_playing = false;
            }
        } else if self.current_time_s <= 0.0 {
            if self.ping_pong {
                self.ping_pong_direction = 1;
                self.current_time_s = 0.0;
            } else {
                self.current_time_s = 0.0;
                self.is_playing = false;
            }
        }
    }

    pub(crate) fn presentation_step_next(&mut self) {
        if let Some(&next_p) =
            self.pause_points_s.iter().find(|&&p| p > self.current_time_s + 0.001)
        {
            if self.is_playing {
                self.current_time_s = next_p;
                self.is_playing = false;
                self.paused_at_step = true;
            } else {
                self.is_playing = true;
                self.paused_at_step = false;
            }
        } else if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
            self.is_playing = true;
            self.paused_at_step = false;
        } else {
            self.is_playing = true;
            self.paused_at_step = false;
        }
    }

    pub(crate) fn presentation_step_prev(&mut self) {
        if let Some(&prev_p) =
            self.pause_points_s.iter().rev().find(|&&p| p < self.current_time_s - 0.001)
        {
            self.current_time_s = prev_p;
            self.is_playing = false;
            self.paused_at_step = true;
        } else {
            self.current_time_s = 0.0;
            self.is_playing = false;
            self.paused_at_step = true;
        }
    }

    pub(crate) fn toggle_presentation_mode(&mut self) {
        self.presentation_mode = !self.presentation_mode;
        if self.presentation_mode {
            self.is_playing = false;
            self.paused_at_step = true;
        }
    }
}
