//! Graph Editor (F-Curve)
//!
//! Simplified first pass: single float properties (position.x, rotation).
//! View toggle in Inspector keyframe area: List | Curve | Strip.

use crate::app::commands::CommandQueue;
use crate::app::design_tokens::*;
use animatix::easing::Easing;
use animatix::timeline::{AnimationTrack, property_keyframe_times, read_property_value, property_keyframe_easing, lookup_property};
use egui::{FontId, Pos2, Sense, Stroke, Vec2};

/// Render a simple F-curve graph for a single float property.
pub fn render_fcurve(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    property_name: &str,
    duration_s: f64,
    current_time_s: f64,
    _commands: &mut CommandQueue,
) {
    let available = ui.available_width();
    let height = 120.0f32;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available, height), Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, RADIUS_M, BG_BASE);
    painter.rect_stroke(rect, RADIUS_M, Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);

    // Collect keyframes for this property
    let field = match lookup_property(property_name) {
        Some(schema) => schema.field,
        None => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Unknown property",
                FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                TEXT_MUTED,
            );
            return;
        }
    };
    let kf_times: Vec<u64> = property_keyframe_times(track, field);
    if kf_times.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No keyframes",
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        return;
    }

    // Get values
    let mut points: Vec<(f64, f32)> = Vec::new();
    for time_ms in &kf_times {
        if let Some(animatix::timeline::PropertyValue::F32(v)) =
            read_property_value(track, field, *time_ms)
        {
            points.push((*time_ms as f64 / 1000.0, v));
        }
    }

    if points.len() < 2 {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Need ≥2 keyframes",
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        return;
    }

    // Find value range
    let min_val = points.iter().map(|(_, v)| *v).fold(f32::INFINITY, f32::min);
    let max_val = points.iter().map(|(_, v)| *v).fold(f32::NEG_INFINITY, f32::max);
    let val_range = (max_val - min_val).max(0.001);
    let val_mid = (min_val + max_val) / 2.0;

    // Map time → x, value → y
    let plot_rect = rect.shrink2(Vec2::new(SPACE_M, SPACE_S));
    let map_point = |time_s: f64, val: f32| -> Pos2 {
        let tx = ((time_s / duration_s.max(0.1)) as f32).clamp(0.0, 1.0);
        let ty = 1.0 - ((val - min_val) / val_range).clamp(0.0, 1.0);
        Pos2::new(
            egui::lerp(plot_rect.left()..=plot_rect.right(), tx),
            egui::lerp(plot_rect.top()..=plot_rect.bottom(), ty),
        )
    };

    // Draw grid lines
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let y = egui::lerp(plot_rect.top()..=plot_rect.bottom(), t);
        painter.line_segment(
            [Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)],
            Stroke::new(1.0, grid_line()),
        );
        let val_label = format!("{:.1}", max_val - t * val_range);
        painter.text(
            Pos2::new(plot_rect.left() + 2.0, y),
            egui::Align2::LEFT_CENTER,
            val_label,
            FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
    }

    // Draw curve segments between keyframes
    for i in 0..points.len().saturating_sub(1) {
        let (t0, v0) = points[i];
        let (t1, v1) = points[i + 1];

        // Get easing for this segment
        let easing = property_keyframe_easing(track, field, (t1 * 1000.0) as u64)
            .unwrap_or(Easing::Linear);

        // Sample the easing curve with enough points for smooth rendering
        let segments = 20;
        let mut curve_points: Vec<Pos2> = Vec::with_capacity(segments + 1);
        for s in 0..=segments {
            let progress = s as f32 / segments as f32;
            let eased = animatix::easing::apply_easing(progress, easing);
            let time_s = t0 + (t1 - t0) * eased as f64;
            let val = v0 + (v1 - v0) * eased;
            curve_points.push(map_point(time_s, val));
        }

        // Draw the sampled curve as connected line segments
        for w in curve_points.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(2.0, ACCENT_BLUE));
        }
    }

    // Draw keyframe dots
    for (time_s, val) in &points {
        let p = map_point(*time_s, *val);
        let is_current = (*time_s - current_time_s).abs() < 0.05;
        let color = if is_current { AMBER } else { TEXT_PRIMARY };
        let size = if is_current { 5.0 } else { 3.5 };
        painter.circle_filled(p, size, color);

        if is_current {
            painter.circle_stroke(p, size + 2.0, Stroke::new(1.0, AMBER));
        }
    }

    // Current time playhead line
    let current_x = map_point(current_time_s, val_mid).x;
    if current_x >= plot_rect.left() && current_x <= plot_rect.right() {
        painter.line_segment(
            [Pos2::new(current_x, plot_rect.top()), Pos2::new(current_x, plot_rect.bottom())],
            Stroke::new(1.5, AMBER),
        );
    }

    // Hover: change cursor
    if response.hovered() {
        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
    }
}
