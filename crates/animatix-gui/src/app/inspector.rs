use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, RichText, ScrollArea, Stroke, Vec2};
use std::collections::BTreeMap;

/// Renders the actor inspector panel.
///
/// Shows:
/// - A collapsible tree view of all actors in the timeline
/// - Selected actor's properties grouped by category
/// - Keyframe list with current-time highlighting
pub(super) fn inspector_ui(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    selected_actor: &mut Option<String>,
    current_time_s: f64,
) {
    ui.vertical(|ui| {
        // Reset selection if actor no longer exists in timeline
        let should_reset = selected_actor
            .as_ref()
            .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
        if should_reset {
            *selected_actor = None;
        }

        let Some(timeline) = timeline else {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No timeline loaded — rebuild to inspect")
                    .size(11.0)
                    .color(Color32::from_rgb(90, 96, 110)),
            );
            return;
        };

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.add_space(20.0);
            ui.label(
                RichText::new("No actors in scene")
                    .size(11.0)
                    .color(Color32::from_rgb(90, 96, 110)),
            );
            return;
        }

        // Split: actor list on top, details on bottom
        let available = ui.available_size_before_wrap();
        let list_height = (available.y * 0.35).max(120.0);

        // Actor tree
        egui::Frame::NONE.show(ui, |ui| {
            ui.set_max_height(list_height);
            ScrollArea::vertical().show(ui, |ui| {
                let actor_count = count_all_actors(timeline, root_nodes);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("ACTORS")
                            .size(10.0)
                            .color(Color32::from_rgb(90, 96, 110))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(actor_count.to_string())
                                .size(10.0)
                                .color(Color32::from_rgb(90, 96, 110)),
                        );
                    });
                });
                ui.add_space(2.0);
                // Subtle separator
                let sep_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
                ui.painter().rect_filled(
                    sep_rect,
                    0.0,
                    Color32::from_rgb(40, 44, 52),
                );
                ui.add_space(4.0);

                for root_label in root_nodes {
                    render_actor_tree(ui, timeline, root_label, selected_actor, 0);
                }
            });
        });

        ui.add_space(4.0);
        let sep_rect = ui.allocate_space(Vec2::new(ui.available_width(), 1.0)).1;
        ui.painter().rect_filled(sep_rect, 0.0, Color32::from_rgb(40, 44, 52));
        ui.add_space(4.0);

        // Selected actor details
        if let Some(sel) = selected_actor.as_ref() {
            let Some(track) = timeline.get_track(sel) else {
                ui.label(
                    RichText::new("Actor not found")
                        .size(11.0)
                        .color(Color32::from_rgb(90, 96, 110)),
                );
                return;
            };

            ScrollArea::vertical().show(ui, |ui| {
                render_actor_details(ui, track, current_time_s);
            });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Select an actor to inspect")
                        .size(11.0)
                        .color(Color32::from_rgb(90, 96, 110)),
                );
            });
        }
    });
}

fn count_all_actors(timeline: &Timeline, root_nodes: &[String]) -> usize {
    let mut count = 0;
    for root in root_nodes {
        count += 1;
        if let Some(track) = timeline.get_track(root) {
            count += count_children(timeline, &track.children);
        }
    }
    count
}

fn count_children(timeline: &Timeline, children: &[String]) -> usize {
    let mut count = 0;
    for child in children {
        count += 1;
        if let Some(track) = timeline.get_track(child) {
            count += count_children(timeline, &track.children);
        }
    }
    count
}

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actor: &mut Option<String>,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actor.as_deref() == Some(label);
    let is_anonymous = label.starts_with("__anon");

    let shape_hint = shape_type_hint(track);

    let indent = depth as f32 * 12.0;
    let height = 20.0;
    let available = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    // Background
    let bg_color = match (is_selected, response.hovered()) {
        (true, _) => Color32::from_rgb(55, 62, 75),
        (_, true) => Color32::from_rgb(32, 36, 44),
        _ => Color32::TRANSPARENT,
    };
    if bg_color != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 3.0, bg_color);
    }

    // Selected indicator (left amber bar)
    if is_selected {
        let indicator = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y + 2.0),
            egui::pos2(rect.min.x + 2.0, rect.max.y - 2.0),
        );
        ui.painter().rect_filled(indicator, 1.0, Color32::from_rgb(255, 196, 92));
    }

    // Label
    let display_label = if is_anonymous {
        format!("{} (anon)", label)
    } else {
        label.to_string()
    };

    let text_color = if is_selected {
        Color32::from_rgb(228, 232, 243)
    } else if is_anonymous {
        Color32::from_rgb(90, 96, 110)
    } else {
        Color32::from_rgb(150, 158, 175)
    };

    let text_pos = egui::pos2(rect.min.x + indent + 4.0, rect.center().y);
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        &display_label,
        egui::TextStyle::Small.resolve(ui.style()),
        text_color,
    );

    // Shape type hint (right-aligned, muted)
    if let Some(shape) = shape_hint {
        ui.painter().text(
            egui::pos2(rect.max.x - 6.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shape,
            egui::TextStyle::Small.resolve(ui.style()),
            Color32::from_rgb(90, 96, 110),
        );
    }

    if response.clicked() {
        *selected_actor = Some(label.to_string());
    }

    // Render children
    for child_label in &track.children {
        render_actor_tree(ui, timeline, child_label, selected_actor, depth + 1);
    }
}

fn shape_type_hint(track: &AnimationTrack) -> Option<&'static str> {
    track.shape_type.as_ref().map(|pt| {
        let current = pt.evaluate(0);
        match current {
            ShapeType::Rect => "Rect",
            ShapeType::Circle => "Circle",
            ShapeType::Line => "Line",
            ShapeType::Ellipse => "Ellipse",
            ShapeType::Arc => "Arc",
            ShapeType::Polygon => "Polygon",
            ShapeType::Path => "Path",
            ShapeType::Arrow => "Arrow",
            ShapeType::Graph => "Graph",
            ShapeType::Plot => "Plot",
        }
    })
}

fn render_actor_details(ui: &mut egui::Ui, track: &AnimationTrack, current_time_s: f64) {
    let current_time_ms = (current_time_s * 1000.0) as u64;

    ui.add_space(2.0);

    // Actor header: label + shape type
    ui.horizontal(|ui| {
        ui.label(RichText::new(&track.label).strong().size(14.0).color(Color32::from_rgb(228, 232, 243)));
        if let Some(shape_pt) = &track.shape_type {
            let shape = shape_pt.evaluate(current_time_ms);
            ui.label(
                RichText::new(format!("{shape:?}"))
                    .size(11.0)
                    .color(Color32::from_rgb(137, 200, 235)),
            );
        }
    });

    // First seen time
    if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
        ui.label(
            RichText::new(format!("First seen: {:.2}s", track.first_seen_ms as f64 / 1000.0))
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110)),
        );
    }

    ui.add_space(6.0);

    // Property groups
    let groups = build_property_groups(track, current_time_ms);
    for group in &groups {
        render_property_group(ui, group);
    }

    ui.add_space(8.0);

    // Keyframe table
    let keyframes = collect_keyframes(track);
    if !keyframes.is_empty() {
        render_keyframe_table(ui, &keyframes, current_time_ms);
    } else {
        ui.label(
            RichText::new("No keyframes — default values only")
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110)),
        );
    }
}

// ─── Property Groups ───────────────────────────────────────────────────────

struct PropertyGroup {
    name: &'static str,
    properties: Vec<(String, PropertyDisplayValue)>,
}

enum PropertyDisplayValue {
    Scalar(String),
    Vec2(String, String),
    Color([f32; 4]),
    Text(String),
}

fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let mut groups = Vec::new();

    // Transform group
    let mut transform = PropertyGroup { name: "Transform", properties: Vec::new() };
    if let Some(pt) = &track.position {
        let v = pt.evaluate(time_ms);
        transform.properties.push(("position".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.motion_offset {
        let v = pt.evaluate(time_ms);
        transform.properties.push(("motion_offset".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.rotation {
        let v = pt.evaluate(time_ms);
        transform.properties.push(("rotation".into(), PropertyDisplayValue::Scalar(format!("{:.1}°", v.to_degrees()))));
    }
    if let Some(pt) = &track.scale {
        let v = pt.evaluate(time_ms);
        transform.properties.push(("scale".into(), PropertyDisplayValue::Scalar(format!("{:.2}", v))));
    }
    if !transform.properties.is_empty() {
        groups.push(transform);
    }

    // Shape group
    let mut shape = PropertyGroup { name: "Shape", properties: Vec::new() };
    if let Some(pt) = &track.shape_type {
        let v = pt.evaluate(time_ms);
        shape.properties.push(("shape_type".into(), PropertyDisplayValue::Text(format!("{v:?}"))));
    }
    if let Some(pt) = &track.line_from {
        let v = pt.evaluate(time_ms);
        shape.properties.push(("line_from".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.line_to {
        let v = pt.evaluate(time_ms);
        shape.properties.push(("line_to".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.arc_angles {
        let v = pt.evaluate(time_ms);
        shape.properties.push(("arc_angles".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.points {
        let v = pt.evaluate(time_ms);
        shape.properties.push(("points".into(), PropertyDisplayValue::Text(format!("{} pts", v.len()))));
    }
    if !shape.properties.is_empty() {
        groups.push(shape);
    }

    // Style group
    let mut style = PropertyGroup { name: "Style", properties: Vec::new() };
    if let Some(pt) = &track.color {
        let v = pt.evaluate(time_ms);
        style.properties.push(("color".into(), PropertyDisplayValue::Color(v)));
    }
    if let Some(pt) = &track.opacity {
        let v = pt.evaluate(time_ms);
        style.properties.push(("opacity".into(), PropertyDisplayValue::Scalar(format!("{:.2}", v))));
    }
    if let Some(pt) = &track.stroke_width {
        let v = pt.evaluate(time_ms);
        style.properties.push(("stroke_width".into(), PropertyDisplayValue::Scalar(format!("{:.1}", v))));
    }
    if let Some(pt) = &track.stroke_color {
        let v = pt.evaluate(time_ms);
        style.properties.push(("stroke_color".into(), PropertyDisplayValue::Color(v)));
    }
    if let Some(pt) = &track.stroke_progress {
        let v = pt.evaluate(time_ms);
        style.properties.push(("stroke_progress".into(), PropertyDisplayValue::Scalar(format!("{:.2}", v))));
    }
    if let Some(pt) = &track.fill_opacity {
        let v = pt.evaluate(time_ms);
        style.properties.push(("fill_opacity".into(), PropertyDisplayValue::Scalar(format!("{:.2}", v))));
    }
    if !style.properties.is_empty() {
        groups.push(style);
    }

    // Content group
    let mut content = PropertyGroup { name: "Content", properties: Vec::new() };
    if let Some(pt) = &track.text_content {
        let v = pt.evaluate(time_ms);
        let display = if v.len() > 30 { format!("{}…", &v[..30]) } else { v.clone() };
        content.properties.push(("text_content".into(), PropertyDisplayValue::Text(display)));
    }
    if let Some(pt) = &track.text_paths {
        let v = pt.evaluate(time_ms);
        content.properties.push(("text_paths".into(), PropertyDisplayValue::Text(format!("{} paths", v.len()))));
    }
    if let Some(pt) = &track.vector_paths {
        let v = pt.evaluate(time_ms);
        content.properties.push(("vector_paths".into(), PropertyDisplayValue::Text(format!("{} paths", v.len()))));
    }
    if let Some(pt) = &track.image {
        let v = pt.evaluate(time_ms);
        content.properties.push(("image".into(), PropertyDisplayValue::Text(if v.is_some() { "loaded".into() } else { "none".into() })));
    }
    if !content.properties.is_empty() {
        groups.push(content);
    }

    // Layout group
    let mut layout = PropertyGroup { name: "Layout", properties: Vec::new() };
    if let Some(pt) = &track.size {
        let v = pt.evaluate(time_ms);
        layout.properties.push(("size".into(), PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1]))));
    }
    if let Some(pt) = &track.placement_mode {
        let v = pt.evaluate(time_ms);
        layout.properties.push(("placement_mode".into(), PropertyDisplayValue::Text(format!("{v:?}"))));
    }
    if let Some(pt) = &track.position_binding {
        let v = pt.evaluate(time_ms);
        layout.properties.push(("position_binding".into(), PropertyDisplayValue::Text(format!("{v:?}"))));
    }
    if !layout.properties.is_empty() {
        groups.push(layout);
    }

    groups
}

fn render_property_group(ui: &mut egui::Ui, group: &PropertyGroup) {
    let count = group.properties.len();
    let header_text = format!("{}  ({})", group.name, count);

    egui::CollapsingHeader::new(
        RichText::new(&header_text)
            .size(11.0)
            .color(Color32::from_rgb(150, 158, 175))
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
        for (name, value) in &group.properties {
            render_property_row(ui, name, value);
        }
    });
}

fn render_property_row(ui: &mut egui::Ui, name: &str, value: &PropertyDisplayValue) {
    let row_height = 18.0;
    let available = ui.available_width();
    let (rect, _response) = ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    // Property name
    ui.painter().text(
        egui::pos2(rect.min.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::TextStyle::Small.resolve(ui.style()),
        Color32::from_rgb(110, 118, 135),
    );

    // Value (right-aligned)
    match value {
        PropertyDisplayValue::Scalar(s) => {
            ui.painter().text(
                egui::pos2(rect.max.x - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                s,
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::from_rgb(200, 206, 220),
            );
        }
        PropertyDisplayValue::Vec2(x, y) => {
            let text = format!("({}, {})", x, y);
            ui.painter().text(
                egui::pos2(rect.max.x - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                &text,
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::from_rgb(200, 206, 220),
            );
        }
        PropertyDisplayValue::Color(rgba) => {
            // Color swatch + hex
            let hex = color_to_hex(rgba);
            let swatch_size = 10.0;
            let swatch_x = rect.max.x - 6.0 - 60.0;
            let swatch_rect = egui::Rect::from_center_size(
                egui::pos2(swatch_x, rect.center().y),
                Vec2::new(swatch_size, swatch_size),
            );
            let color = Color32::from_rgba_premultiplied(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
                (rgba[3] * 255.0) as u8,
            );
            ui.painter().rect_filled(swatch_rect, 2.0, color);
            ui.painter().rect_stroke(
                swatch_rect,
                2.0,
                Stroke::new(1.0, Color32::from_rgb(60, 65, 78)),
                egui::StrokeKind::Outside,
            );

            ui.painter().text(
                egui::pos2(rect.max.x - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                &hex,
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::from_rgb(200, 206, 220),
            );
        }
        PropertyDisplayValue::Text(s) => {
            ui.painter().text(
                egui::pos2(rect.max.x - 6.0, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                s,
                egui::TextStyle::Small.resolve(ui.style()),
                Color32::from_rgb(137, 200, 235),
            );
        }
    }
}

// ─── Keyframe Table ────────────────────────────────────────────────────────

fn render_keyframe_table(ui: &mut egui::Ui, keyframes: &[(f64, String, String, String)], current_time_ms: u64) {
    ui.add_space(4.0);

    // Header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("KEYFRAMES")
                .size(10.0)
                .color(Color32::from_rgb(90, 96, 110))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(keyframes.len().to_string())
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

// ─── Helpers ───────────────────────────────────────────────────────────────

fn format_num(v: f32) -> String {
    if v == v.floor() && v.abs() < 10000.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

fn color_to_hex(rgba: &[f32; 4]) -> String {
    let r = (rgba[0] * 255.0).round() as u8;
    let g = (rgba[1] * 255.0).round() as u8;
    let b = (rgba[2] * 255.0).round() as u8;
    if rgba[3] >= 0.99 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        let a = (rgba[3] * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    }
}

fn collect_keyframes(track: &AnimationTrack) -> Vec<(f64, String, String, String)> {
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
