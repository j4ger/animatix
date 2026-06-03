//! Graph Editor (F-Curve)
//!
//! Multi-property graph editor with color-coded curves.
//! Supports float, Vec2 (X/Y components), and Color (RGBA channels).

use crate::app::commands::ActionQueue;
use crate::app::design_tokens::*;
use animatix_syntax::easing::Easing;
use animatix::timeline::{AnimationTrack, property_keyframe_times, read_property_value, property_keyframe_easing, ValueType};
use egui::{FontId, Pos2, Sense, Stroke, Vec2, Color32};

/// Information about a single curve to render.
#[derive(Debug, Clone)]
struct CurveInfo {
    label: String,
    color: Color32,
    points: Vec<(f64, f32)>,
    field: animatix::timeline::ActorField,
}

/// Render a multi-property F-curve graph.
pub fn render_multi_fcurve(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    duration_s: f64,
    current_time_s: f64,
    _commands: &mut ActionQueue,
) {
    let available = ui.available_width();
    let height = 160.0f32;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available, height), Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, RADIUS_M, BG_BASE);
    painter.rect_stroke(rect, RADIUS_M, Stroke::new(STROKE_WIDTH, BORDER), egui::StrokeKind::Outside);

    // Collect all animated properties
    let indices = animatix::timeline::allowed_property_indices(track.kind);
    let mut curves: Vec<CurveInfo> = Vec::new();

    for &idx in &indices {
        let schema = &animatix::timeline::PROPERTY_REGISTRY[idx];
        if !animatix::timeline::property_has_keyframes(track, schema.field) {
            continue;
        }

        let kf_times = property_keyframe_times(track, schema.field);
        if kf_times.len() < 2 {
            continue;
        }

        match schema.value_type {
            ValueType::F32 => {
                let mut points: Vec<(f64, f32)> = Vec::new();
                for time_ms in &kf_times {
                    if let Some(animatix::timeline::PropertyValue::F32(v)) =
                        read_property_value(track, schema.field, *time_ms)
                    {
                        points.push((*time_ms as f64 / 1000.0, v));
                    }
                }
                if points.len() >= 2 {
                    curves.push(CurveInfo {
                        label: schema.name.to_string(),
                        color: ACCENT_BLUE,
                        points,
                        field: schema.field,
                    });
                }
            }
            ValueType::Vec2 => {
                let mut x_points: Vec<(f64, f32)> = Vec::new();
                let mut y_points: Vec<(f64, f32)> = Vec::new();
                for time_ms in &kf_times {
                    if let Some(animatix::timeline::PropertyValue::Vec2(v)) =
                        read_property_value(track, schema.field, *time_ms)
                    {
                        x_points.push((*time_ms as f64 / 1000.0, v[0]));
                        y_points.push((*time_ms as f64 / 1000.0, v[1]));
                    }
                }
                if x_points.len() >= 2 {
                    curves.push(CurveInfo {
                        label: format!("{}.X", schema.name),
                        color: Color32::from_rgb(255, 100, 100),
                        points: x_points,
                        field: schema.field,
                    });
                    curves.push(CurveInfo {
                        label: format!("{}.Y", schema.name),
                        color: Color32::from_rgb(100, 255, 100),
                        points: y_points,
                        field: schema.field,
                    });
                }
            }
            ValueType::Vec4 | ValueType::Color => {
                let mut r_points: Vec<(f64, f32)> = Vec::new();
                let mut g_points: Vec<(f64, f32)> = Vec::new();
                let mut b_points: Vec<(f64, f32)> = Vec::new();
                let mut a_points: Vec<(f64, f32)> = Vec::new();
                for time_ms in &kf_times {
                    let val = match read_property_value(track, schema.field, *time_ms) {
                        Some(animatix::timeline::PropertyValue::Vec4(v)) => Some(v),
                        Some(animatix::timeline::PropertyValue::Color(v)) => Some(v),
                        _ => None,
                    };
                    if let Some(v) = val {
                        r_points.push((*time_ms as f64 / 1000.0, v[0]));
                        g_points.push((*time_ms as f64 / 1000.0, v[1]));
                        b_points.push((*time_ms as f64 / 1000.0, v[2]));
                        a_points.push((*time_ms as f64 / 1000.0, v[3]));
                    }
                }
                if r_points.len() >= 2 {
                    curves.push(CurveInfo {
                        label: format!("{}.R", schema.name),
                        color: Color32::from_rgb(255, 80, 80),
                        points: r_points,
                        field: schema.field,
                    });
                    curves.push(CurveInfo {
                        label: format!("{}.G", schema.name),
                        color: Color32::from_rgb(80, 255, 80),
                        points: g_points,
                        field: schema.field,
                    });
                    curves.push(CurveInfo {
                        label: format!("{}.B", schema.name),
                        color: Color32::from_rgb(80, 140, 255),
                        points: b_points,
                        field: schema.field,
                    });
                    curves.push(CurveInfo {
                        label: format!("{}.A", schema.name),
                        color: Color32::from_rgb(200, 200, 200),
                        points: a_points,
                        field: schema.field,
                    });
                }
            }
            _ => {}
        }
    }

    if curves.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No keyframes to graph",
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        return;
    }

    // Legend with visibility toggles
    let visibility_id = ui.id().with("graph_visibility");
    let mut visibility: std::collections::HashMap<String, bool> = ui.data(|d| {
        d.get_temp(visibility_id).unwrap_or_default()
    });
    for curve in &curves {
        visibility.entry(curve.label.clone()).or_insert(true);
    }

    let legend_height = 18.0f32;
    let legend_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + SPACE_M, rect.min.y + SPACE_S),
        egui::pos2(rect.max.x - SPACE_M, rect.min.y + SPACE_S + legend_height),
    );

    let mut legend_x = legend_rect.min.x;
    for curve in &curves {
        let is_visible = *visibility.get(&curve.label).unwrap_or(&true);
        let item_width = 50.0f32;
        let item_rect = egui::Rect::from_min_size(
            egui::pos2(legend_x, legend_rect.min.y),
            Vec2::new(item_width, legend_height),
        );
        if ui.rect_contains_pointer(item_rect) {
            ui.painter().rect_filled(item_rect, RADIUS_S, BG_HOVER);
        }
        let color_dot = if is_visible { curve.color } else { TEXT_DISABLED };
        ui.painter().circle_filled(
            egui::pos2(item_rect.min.x + 6.0, item_rect.center().y),
            3.0,
            color_dot,
        );
        ui.painter().text(
            egui::pos2(item_rect.min.x + 14.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &curve.label,
            FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED },
        );

        // Click to toggle visibility
        let item_response = ui.interact(item_rect, ui.id().with(("legend", &curve.label)), Sense::click());
        if item_response.clicked() {
            visibility.insert(curve.label.clone(), !is_visible);
        }
        legend_x += item_width + SPACE_S;
    }

    ui.data_mut(|d| d.insert_temp(visibility_id, visibility.clone()));

    // Plot area
    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + SPACE_M, legend_rect.max.y + SPACE_S),
        egui::pos2(rect.max.x - SPACE_M, rect.max.y - SPACE_S),
    );

    // Find global value range across all visible curves
    let visible_curves: Vec<&CurveInfo> = curves.iter()
        .filter(|c| *visibility.get(&c.label).unwrap_or(&true))
        .collect();

    if visible_curves.is_empty() {
        painter.text(
            plot_rect.center(),
            egui::Align2::CENTER_CENTER,
            "All curves hidden",
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        return;
    }

    let all_values: Vec<f32> = visible_curves.iter().flat_map(|c| c.points.iter().map(|(_, v)| *v)).collect();
    let min_val = all_values.iter().copied().fold(f32::INFINITY, f32::min);
    let max_val = all_values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let val_range = (max_val - min_val).max(0.001);
    let val_mid = (min_val + max_val) / 2.0;

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
            Stroke::new(STROKE_WIDTH, grid_line()),
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

    // Draw each visible curve
    for curve in &visible_curves {
        for i in 0..curve.points.len().saturating_sub(1) {
            let (t0, v0) = curve.points[i];
            let (t1, v1) = curve.points[i + 1];

            let easing = property_keyframe_easing(track, curve.field, (t1 * 1000.0) as u64)
                .unwrap_or(Easing::Linear);

            let segments = 20;
            let mut prev = map_point(t0, v0);
            for s in 1..=segments {
                let progress = s as f32 / segments as f32;
                let eased = animatix_syntax::easing::apply_easing(progress, easing);
                let time_s = t0 + (t1 - t0) * eased as f64;
                let val = v0 + (v1 - v0) * eased;
                let curr = map_point(time_s, val);
                painter.line_segment([prev, curr], Stroke::new(2.0, curve.color));
                prev = curr;
            }
        }

        // Draw keyframe dots
        for (time_s, val) in &curve.points {
            let p = map_point(*time_s, *val);
            let is_current = (*time_s - current_time_s).abs() < 0.05;
            let size = if is_current { 4.0 } else { 2.5 };
            painter.circle_filled(p, size, curve.color);
            if is_current {
                painter.circle_stroke(p, size + 2.0, Stroke::new(STROKE_WIDTH, AMBER));
            }
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


