use std::collections::BTreeMap;

use animatix::timeline::AnimationTrack;
use egui::{Color32, Vec2};

// ─── Local Palette ──────────────────────────────────────────────────────────

const BG_SURFACE: Color32 = Color32::from_rgb(24, 27, 33);
const BG_WIDGET: Color32 = Color32::from_rgb(32, 36, 44);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 232, 243);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 158, 175);
const TEXT_MUTED: Color32 = Color32::from_rgb(90, 96, 110);
const AMBER: Color32 = Color32::from_rgb(255, 196, 92);

// ─── Compact Keyframe List ──────────────────────────────────────────────────

pub(super) fn render_keyframe_table(
    ui: &mut egui::Ui,
    keyframes: &[(f64, String, String, String)],
    current_time_ms: u64,
) {
    if keyframes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new(egui_phosphor::regular::FILM_STRIP)
                    .size(22.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("No keyframes — default values only")
                    .size(10.0)
                    .color(TEXT_MUTED),
            );
        });
        return;
    }

    // Header with count badge
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("KEYFRAMES")
                .size(9.0)
                .color(TEXT_MUTED)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::Frame::new()
                .fill(BG_WIDGET)
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(keyframes.len().to_string())
                            .size(9.0)
                            .color(TEXT_SECONDARY),
                    );
                });
        });
    });

    ui.add_space(4.0);
    let sep = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
    ui.painter().rect_filled(sep, 0.0, BG_WIDGET);
    ui.add_space(2.0);

    // Compact rows
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

    for (time_s, property, value, _easing) in keyframes {
        let kf_time_ms = (*time_s * 1000.0) as u64;
        let is_current = kf_time_ms == current_time_ms;

        let row_height = 18.0;
        let available = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

        // Hover background
        if response.hovered() && !is_current {
            ui.painter().rect_filled(rect, 0.0, BG_SURFACE);
        }

        // Current indicator: amber dot
        if is_current {
            let dot = egui::Rect::from_center_size(
                egui::pos2(rect.min.x + 6.0, rect.center().y),
                Vec2::new(4.0, 4.0),
            );
            ui.painter().rect_filled(dot, 2.0, AMBER);
        }

        let text_color = if is_current { AMBER } else { TEXT_SECONDARY };
        let font = egui::TextStyle::Small.resolve(ui.style());
        let time_x = if is_current { 14.0 } else { 8.0 };

        // Time
        ui.painter().text(
            egui::pos2(rect.min.x + time_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &format!("{:.2}s", time_s),
            font.clone(),
            text_color,
        );

        // Property name
        ui.painter().text(
            egui::pos2(rect.min.x + 56.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            property,
            font.clone(),
            text_color,
        );

        // Value (truncated)
        ui.painter().text(
            egui::pos2(rect.max.x - 6.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            truncate_value(value),
            font,
            if is_current { AMBER } else { TEXT_MUTED },
        );
    }
}

fn truncate_value(value: &str) -> String {
    if value.len() > 24 {
        format!("{}…", &value[..24])
    } else {
        value.to_string()
    }
}

// ─── Utilities ──────────────────────────────────────────────────────────────

pub(super) fn format_num(v: f32) -> String {
    if v == v.floor() && v.abs() < 10000.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

pub(super) fn collect_keyframes(
    track: &AnimationTrack,
) -> Vec<(f64, String, String, String)> {
    let mut all: Vec<(u64, &str, String)> = Vec::new();

    fn push_keyframes_from<V: std::fmt::Debug, E>(
        all: &mut Vec<(u64, &str, String)>,
        name: &'static str,
        keyframes: &BTreeMap<u64, (V, E)>,
    ) {
        for (&time_ms, (value, _)) in keyframes {
            all.push((time_ms, name, format!("{value:?}")));
        }
    }

    if let Some(pt) = &track.position {
        push_keyframes_from(&mut all, "position", &pt.keyframes);
    }
    if let Some(pt) = &track.size {
        push_keyframes_from(&mut all, "size", &pt.keyframes);
    }
    if let Some(pt) = &track.scale {
        push_keyframes_from(&mut all, "scale", &pt.keyframes);
    }
    if let Some(pt) = &track.rotation {
        push_keyframes_from(&mut all, "rotation", &pt.keyframes);
    }
    if let Some(pt) = &track.opacity {
        push_keyframes_from(&mut all, "opacity", &pt.keyframes);
    }
    if let Some(pt) = &track.color {
        push_keyframes_from(&mut all, "color", &pt.keyframes);
    }
    if let Some(pt) = &track.stroke_width {
        push_keyframes_from(&mut all, "stroke_width", &pt.keyframes);
    }
    if let Some(pt) = &track.stroke_color {
        push_keyframes_from(&mut all, "stroke_color", &pt.keyframes);
    }
    if let Some(pt) = &track.fill_opacity {
        push_keyframes_from(&mut all, "fill_opacity", &pt.keyframes);
    }
    if let Some(pt) = &track.text_content {
        push_keyframes_from(&mut all, "text_content", &pt.keyframes);
    }
    if let Some(pt) = &track.motion_offset {
        push_keyframes_from(&mut all, "motion_offset", &pt.keyframes);
    }
    if let Some(pt) = &track.line_from {
        push_keyframes_from(&mut all, "line_from", &pt.keyframes);
    }
    if let Some(pt) = &track.line_to {
        push_keyframes_from(&mut all, "line_to", &pt.keyframes);
    }
    if let Some(pt) = &track.arc_angles {
        push_keyframes_from(&mut all, "arc_angles", &pt.keyframes);
    }
    if let Some(pt) = &track.stroke_progress {
        push_keyframes_from(&mut all, "stroke_progress", &pt.keyframes);
    }
    if let Some(pt) = &track.points {
        push_keyframes_from(&mut all, "points", &pt.keyframes);
    }

    all.sort_by_key(|(time, _, _)| *time);

    all.into_iter()
        .map(|(time_ms, property, value)| {
            (
                time_ms as f64 / 1000.0,
                property.to_string(),
                value,
                String::new(),
            )
        })
        .collect()
}
