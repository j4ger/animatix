use egui::{Sense, Vec2};

use crate::app::design_tokens::semantic::{border, canvas, status, surface, text};
use crate::app::design_tokens::spatial::{RADIUS_M, SPACE_S, STROKE_WIDTH};

/// Draws a diamond-shaped keyframe marker.
pub fn keyframe_dot(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    is_active: bool,
) {
    let color = if is_active { text::PRIMARY } else { status::WARNING };
    let half = size * 0.5;
    let points = vec![
        center + Vec2::new(0.0, -half),
        center + Vec2::new(half, 0.0),
        center + Vec2::new(0.0, half),
        center + Vec2::new(-half, 0.0),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}

/// Draws the vertical amber playhead line.
pub fn playhead(painter: &egui::Painter, x: f32, y_range: std::ops::Range<f32>) {
    painter.line_segment(
        [egui::pos2(x, y_range.start), egui::pos2(x, y_range.end)],
        egui::Stroke::new(1.5, status::WARNING),
    );
}

/// A mini timeline strip that returns a scrub time on click/drag.
pub struct TimelineStrip<'a> {
    pub duration_s: f64,
    pub current_time_s: f64,
    pub keyframes: &'a [f64],
    pub height: f32,
}

impl<'a> TimelineStrip<'a> {
    pub fn show(self, ui: &mut egui::Ui) -> Option<f64> {
        let desired = Vec2::new(ui.available_width(), self.height);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        let track = rect.shrink2(Vec2::new(SPACE_S, 3.0));
        painter.rect_filled(track, RADIUS_M, surface::WIDGET);
        painter.rect_stroke(track, RADIUS_M, egui::Stroke::new(STROKE_WIDTH, border::DEFAULT), egui::StrokeKind::Outside);

        let sec_step = if self.duration_s > 20.0 { 5.0 } else { 1.0 };
        let mut sec = sec_step;
        while sec < self.duration_s {
            let frac = (sec / self.duration_s) as f32;
            let x = egui::lerp(track.left()..=track.right(), frac);
            painter.line_segment(
                [
                    egui::pos2(x, track.top() + 2.0),
                    egui::pos2(x, track.bottom() - 2.0),
                ],
                egui::Stroke::new(STROKE_WIDTH, canvas::grid_line()),
            );
            sec += sec_step;
        }

        for &kf in self.keyframes {
            let frac = ((kf / self.duration_s) as f32).clamp(0.0, 1.0);
            let x = egui::lerp(track.left()..=track.right(), frac);
            keyframe_dot(&painter, egui::pos2(x, track.center().y), 4.0, false);
        }

        let playhead_frac = ((self.current_time_s / self.duration_s) as f32).clamp(0.0, 1.0);
        let playhead_x = egui::lerp(track.left()..=track.right(), playhead_frac);
        playhead(&painter, playhead_x, track.top() - 1.0..track.bottom() + 1.0);

        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let frac = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
                return Some(frac * self.duration_s);
            }
        }

        None
    }
}
