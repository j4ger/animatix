//! Time Lens — T-Drag HUD
//!
//! Hold `T` → circular time lens appears at cursor.
//! Ring shows keyframe dots. Drag horizontally to scrub time.
//! Scroll wheel zooms time range. Release `T` → lens vanishes.

use crate::app::design_tokens::semantic::accent::PRIMARY as ACCENT_BLUE;
use crate::app::design_tokens::semantic::status::WARNING as AMBER;
use crate::app::design_tokens::semantic::surface::PANEL as BG_PANEL;
use crate::app::design_tokens::semantic::border::DEFAULT as BORDER;
use crate::app::design_tokens::semantic::text::MUTED as TEXT_MUTED;
use crate::app::design_tokens::semantic::text::PRIMARY as TEXT_PRIMARY;
use crate::app::design_tokens::semantic::canvas::grid_line;
use crate::app::design_tokens::semantic::overlay::backdrop as overlay_backdrop;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::typography::{TextRole};
use egui::{Pos2, Stroke};

/// Radius of the time lens ring.
const LENS_RADIUS: f32 = 60.0;
/// Inner radius of the ring (hollow center).
const LENS_INNER_RADIUS: f32 = 40.0;

/// State for the time lens HUD.
pub struct TimeLens {
    /// Is the `T` key currently held?
    pub active: bool,
    /// Cursor position when lens activated.
    pub origin: Pos2,
    /// Time offset from drag start.
    pub drag_offset_s: f64,
    /// Time range visible in the ring (seconds total).
    pub visible_range_s: f64,
}

impl Default for TimeLens {
    fn default() -> Self {
        Self {
            active: false,
            origin: Pos2::ZERO,
            drag_offset_s: 0.0,
            visible_range_s: 5.0,
        }
    }
}

impl TimeLens {
    /// Process input and render the lens. Returns a new scrub time if user dragged.
    pub fn update_and_show(
        &mut self,
        ui: &mut egui::Ui,
        current_time_s: f64,
        duration_s: f64,
        keyframe_times: &[f64],
    ) -> Option<f64> {
        let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
        let t_held = !wants_keyboard
            && ui.input(|i| i.key_pressed(egui::Key::T) || i.key_down(egui::Key::T));

        // Activate on T press
        if t_held && !self.active {
            self.active = true;
            self.origin = ui.ctx().input(|i| i.pointer.latest_pos()).unwrap_or(Pos2::ZERO);
            self.drag_offset_s = 0.0;
        }

        // Deactivate on T release
        if !t_held && self.active {
            self.active = false;
            return None;
        }

        if !self.active {
            return None;
        }

        // Update origin to follow cursor
        if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
            self.origin = cursor;
        }

        // Scroll to zoom visible range
        let scroll = ui.input(|i| i.smooth_scroll_delta);
        if scroll.y != 0.0 {
            let factor = 1.0 + scroll.y * 0.001;
            self.visible_range_s = (self.visible_range_s * factor as f64).clamp(0.5, duration_s.max(1.0));
        }

        // Drag to scrub
        let pointer_down = ui.input(|i| i.pointer.any_down());
        if pointer_down {
            let delta = ui.input(|i| i.pointer.delta());
            // Horizontal drag scrubs time: 1px ≈ visible_range / (2 * radius * π)
            let px_to_time = self.visible_range_s / (LENS_RADIUS as f64 * 2.0 * std::f64::consts::PI);
            self.drag_offset_s += delta.x as f64 * px_to_time;
        }

        let center_time = (current_time_s + self.drag_offset_s)
            .clamp(0.0, duration_s.max(0.0));

        self.render(ui, center_time, duration_s, keyframe_times);

        // Return new time if user is dragging
        if pointer_down && self.drag_offset_s.abs() > 0.001 {
            Some(center_time)
        } else {
            None
        }
    }

    fn render(
        &self,
        ui: &mut egui::Ui,
        center_time: f64,
        duration_s: f64,
        keyframe_times: &[f64],
    ) {
        let center = self.origin;
        let painter = ui.painter();

        // Backdrop dim
        let screen_rect = ui.ctx().viewport_rect();
        painter.rect_filled(screen_rect, 0.0, overlay_backdrop());

        // Outer ring background
        painter.circle_filled(center, LENS_RADIUS, BG_PANEL);
        painter.circle_stroke(center, LENS_RADIUS, Stroke::new(1.5, BORDER));
        painter.circle_stroke(center, LENS_INNER_RADIUS, Stroke::new(STROKE_WIDTH, BORDER));

        // Time range on ring: center_time ± visible_range/2
        let range_start = (center_time - self.visible_range_s / 2.0).max(0.0);
        let range_end = (center_time + self.visible_range_s / 2.0).min(duration_s.max(0.1));

        // Draw tick marks on ring
        let tick_step = if self.visible_range_s > 20.0 { 5.0 } else { 1.0 };
        let mut tick_time = (range_start / tick_step).ceil() * tick_step;
        while tick_time <= range_end {
            let frac = (tick_time - range_start) / (range_end - range_start);
            let angle = frac as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let inner = Pos2::new(
                center.x + angle.cos() * LENS_INNER_RADIUS,
                center.y + angle.sin() * LENS_INNER_RADIUS,
            );
            let outer = Pos2::new(
                center.x + angle.cos() * (LENS_RADIUS - 2.0),
                center.y + angle.sin() * (LENS_RADIUS - 2.0),
            );
            painter.line_segment([inner, outer], Stroke::new(STROKE_WIDTH, grid_line()));
            tick_time += tick_step;
        }

        // Draw keyframe dots on ring
        for &kf in keyframe_times {
            if kf < range_start || kf > range_end {
                continue;
            }
            let frac = ((kf - range_start) / (range_end - range_start)) as f32;
            let angle = frac * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let dot_pos = Pos2::new(
                center.x + angle.cos() * ((LENS_RADIUS + LENS_INNER_RADIUS) / 2.0),
                center.y + angle.sin() * ((LENS_RADIUS + LENS_INNER_RADIUS) / 2.0),
            );
            let is_current = (kf - center_time).abs() < 0.05;
            let color = if is_current { AMBER } else { ACCENT_BLUE };
            let size = if is_current { 4.5 } else { 3.0 };
            painter.circle_filled(dot_pos, size, color);
        }

        // Center timecode
        let time_text = format!("{:.2}s", center_time);
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            &time_text,
            TextRole::Title.font_id(),
            TEXT_PRIMARY,
        );

        // Range indicator below center
        let range_text = format!("±{:.1}s", self.visible_range_s / 2.0);
        painter.text(
            Pos2::new(center.x, center.y + 16.0),
            egui::Align2::CENTER_CENTER,
            &range_text,
            TextRole::Micro.font_id(),
            TEXT_MUTED,
        );

        // Current playhead indicator on ring
        let playhead_angle = 0.0f32 - std::f32::consts::FRAC_PI_2; // top of ring = center_time
        let ph_inner = Pos2::new(
            center.x + playhead_angle.cos() * (LENS_INNER_RADIUS - 2.0),
            center.y + playhead_angle.sin() * (LENS_INNER_RADIUS - 2.0),
        );
        let ph_outer = Pos2::new(
            center.x + playhead_angle.cos() * (LENS_RADIUS + 4.0),
            center.y + playhead_angle.sin() * (LENS_RADIUS + 4.0),
        );
        painter.line_segment([ph_inner, ph_outer], Stroke::new(2.0, AMBER));
    }
}
