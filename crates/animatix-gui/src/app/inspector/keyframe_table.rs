use std::collections::BTreeMap;

use animatix::timeline::AnimationTrack;
use egui::{Color32, Vec2};

pub(super) fn render_keyframe_table(ui: &mut egui::Ui, keyframes: &[(f64, String, String, String)], current_time_ms: u64) {
    ui.add_space(4.0);

    // Header
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("KEYFRAMES")
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(keyframes.len().to_string())
                    .size(10.0)
                    .color(Color32::from_rgb(90, 96, 110)),
            );
        });
    });

    let sep_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
    ui.painter().rect_filled(sep_rect, 0.0, Color32::from_rgb(40, 44, 52));
    ui.add_space(4.0);

    // Table header row
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
    let header_height = 16.0;
    let available = ui.available_width();
    let (hrect, _) = ui.allocate_exact_size(Vec2::new(available, header_height), egui::Sense::hover());
    let muted = Color32::from_rgb(70, 76, 90);
    ui.painter().text(
        egui::pos2(hrect.min.x + 8.0, hrect.center().y),
        egui::Align2::LEFT_CENTER,
        "Time",
        egui::TextStyle::Small.resolve(ui.style()),
        muted,
    );
    ui.painter().text(
        egui::pos2(hrect.min.x + 60.0, hrect.center().y),
        egui::Align2::LEFT_CENTER,
        "Property",
        egui::TextStyle::Small.resolve(ui.style()),
        muted,
    );
    ui.painter().text(
        egui::pos2(hrect.max.x - 6.0, hrect.center().y),
        egui::Align2::RIGHT_CENTER,
        "Value",
        egui::TextStyle::Small.resolve(ui.style()),
        muted,
    );

    // Keyframe rows
    for (time_s, property, value, _easing) in keyframes {
        let kf_time_ms = (*time_s * 1000.0) as u64;
        let is_current = kf_time_ms == current_time_ms;

        let row_height = 18.0;
        let available = ui.available_width();
        let (rect, _response) = ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

        // Current-time highlight: amber left border
        if is_current {
            let indicator = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y),
                egui::pos2(rect.min.x + 2.0, rect.max.y),
            );
            ui.painter().rect_filled(indicator, 0.0, Color32::from_rgb(255, 196, 92));

            // Subtle row background
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(255, 196, 92, 12));
        }

        let text_color = if is_current {
            Color32::from_rgb(255, 220, 120)
        } else {
            Color32::from_rgb(150, 158, 175)
        };

        ui.painter().text(
            egui::pos2(rect.min.x + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &format!("{:.2}s", time_s),
            egui::TextStyle::Small.resolve(ui.style()),
            text_color,
        );
        ui.painter().text(
            egui::pos2(rect.min.x + 60.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            property,
            egui::TextStyle::Small.resolve(ui.style()),
            text_color,
        );
        ui.painter().text(
            egui::pos2(rect.max.x - 6.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            value,
            egui::TextStyle::Small.resolve(ui.style()),
            text_color,
        );
    }
}

pub(super) fn format_num(v: f32) -> String {
    if v == v.floor() && v.abs() < 10000.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

pub(super) fn collect_keyframes(track: &AnimationTrack) -> Vec<(f64, String, String, String)> {
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
            (time_ms as f64 / 1000.0, property.to_string(), value, String::new())
        })
        .collect()
}
