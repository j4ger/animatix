use animatix::timeline::AnimationTrack;
use egui::{Color32, RichText, Vec2};

use crate::app::widgets;
use crate::app::workspace::{PropertyEdit, PropertyValue, UiActions};

use super::keyframe_table::format_num;

pub(super) struct PropertyGroup {
    name: &'static str,
    properties: Vec<PropertyEntry>,
}

pub(super) struct PropertyEntry {
    name: String,
    value: PropertyDisplayValue,
    has_keyframes: bool,
}

pub(super) enum PropertyDisplayValue {
    Scalar(String),
    Vec2(String, String),
    Color([f32; 4]),
    Text(String),
}

pub(super) fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let mut groups = Vec::new();

    // Transform group
    let mut transform = PropertyGroup { name: "Transform", properties: Vec::new() };
    if let Some(pt) = &track.position {
        let v = pt.evaluate(time_ms);
        transform.properties.push(PropertyEntry {
            name: "position".into(),
            value: PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1])),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.motion_offset {
        let v = pt.evaluate(time_ms);
        transform.properties.push(PropertyEntry {
            name: "motion_offset".into(),
            value: PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1])),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.rotation {
        let v = pt.evaluate(time_ms);
        transform.properties.push(PropertyEntry {
            name: "rotation".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.1}°", v.to_degrees())),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.scale {
        let v = pt.evaluate(time_ms);
        transform.properties.push(PropertyEntry {
            name: "scale".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.2}", v)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if !transform.properties.is_empty() {
        groups.push(transform);
    }

    // Shape group
    let mut shape = PropertyGroup { name: "Shape", properties: Vec::new() };
    if let Some(pt) = &track.shape_type {
        let v = pt.evaluate(time_ms);
        shape.properties.push(PropertyEntry {
            name: "shape_type".into(),
            value: PropertyDisplayValue::Text(format!("{v:?}")),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.line_from {
        let v = pt.evaluate(time_ms);
        shape.properties.push(PropertyEntry {
            name: "line_from".into(),
            value: PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1])),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.line_to {
        let v = pt.evaluate(time_ms);
        shape.properties.push(PropertyEntry {
            name: "line_to".into(),
            value: PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1])),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.arc_angles {
        let v = pt.evaluate(time_ms);
        shape.properties.push(PropertyEntry {
            name: "arc_angles".into(),
            value: PropertyDisplayValue::Vec2(format_num(v[0]), format_num(v[1])),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.points {
        let v = pt.evaluate(time_ms);
        shape.properties.push(PropertyEntry {
            name: "points".into(),
            value: PropertyDisplayValue::Text(format!("{} pts", v.len())),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if !shape.properties.is_empty() {
        groups.push(shape);
    }

    // Style group
    let mut style = PropertyGroup { name: "Style", properties: Vec::new() };
    if let Some(pt) = &track.color {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "color".into(),
            value: PropertyDisplayValue::Color(v),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.opacity {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "opacity".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.2}", v)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.stroke_width {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "stroke_width".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.1}", v)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.stroke_color {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "stroke_color".into(),
            value: PropertyDisplayValue::Color(v),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.stroke_progress {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "stroke_progress".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.2}", v)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.fill_opacity {
        let v = pt.evaluate(time_ms);
        style.properties.push(PropertyEntry {
            name: "fill_opacity".into(),
            value: PropertyDisplayValue::Scalar(format!("{:.2}", v)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if !style.properties.is_empty() {
        groups.push(style);
    }

    // Content group
    let mut content = PropertyGroup { name: "Content", properties: Vec::new() };
    if let Some(pt) = &track.text_content {
        let v = pt.evaluate(time_ms);
        let display = if v.len() > 30 { format!("{}…", &v[..30]) } else { v.clone() };
        content.properties.push(PropertyEntry {
            name: "text_content".into(),
            value: PropertyDisplayValue::Text(display),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.text_paths {
        let v = pt.evaluate(time_ms);
        content.properties.push(PropertyEntry {
            name: "text_paths".into(),
            value: PropertyDisplayValue::Text(format!("{} paths", v.len())),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.vector_paths {
        let v = pt.evaluate(time_ms);
        content.properties.push(PropertyEntry {
            name: "vector_paths".into(),
            value: PropertyDisplayValue::Text(format!("{} paths", v.len())),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.image {
        let v = pt.evaluate(time_ms);
        content.properties.push(PropertyEntry {
            name: "image".into(),
            value: PropertyDisplayValue::Text(if v.is_some() { "loaded".into() } else { "none".into() }),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if !content.properties.is_empty() {
        groups.push(content);
    }

    // Layout group
    let mut layout = PropertyGroup { name: "Layout", properties: Vec::new() };
    if let Some(pt) = &track.size {
        let v = pt.evaluate(time_ms);
        // The size track stores half‑extents; display full size.
        let display_w = v[0] * 2.0;
        let display_h = v[1] * 2.0;
        layout.properties.push(PropertyEntry {
            name: "size".into(),
            value: PropertyDisplayValue::Vec2(format_num(display_w), format_num(display_h)),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.placement_mode {
        let v = pt.evaluate(time_ms);
        layout.properties.push(PropertyEntry {
            name: "placement_mode".into(),
            value: PropertyDisplayValue::Text(format!("{v:?}")),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if let Some(pt) = &track.position_binding {
        let v = pt.evaluate(time_ms);
        layout.properties.push(PropertyEntry {
            name: "position_binding".into(),
            value: PropertyDisplayValue::Text(format!("{v:?}")),
            has_keyframes: !pt.keyframes.is_empty(),
        });
    }
    if !layout.properties.is_empty() {
        groups.push(layout);
    }

    groups
}

pub(super) fn render_property_group(
    ui: &mut egui::Ui,
    group: &PropertyGroup,
    actor_label: &str,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
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
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
        for entry in &group.properties {
            render_editable_property_row(ui, actor_label, entry, actions, keyframe_mode);
        }
    });
}

/// Renders a single editable property row, dispatching to the appropriate widget.
pub(super) fn render_editable_property_row(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    let name = &entry.name;
    let has_kf = entry.has_keyframes;

    match &entry.value {
        PropertyDisplayValue::Vec2(x_str, y_str) => {
            // Parse current values for the widget
            let x: f32 = x_str.parse().unwrap_or(0.0);
            let y: f32 = y_str.parse().unwrap_or(0.0);

            if let Some((new_x, new_y)) = widgets::vec2_input(ui, name, x, y, has_kf) {
                actions.property_edits.push(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: name.clone(),
                    value: PropertyValue::Vec2([new_x, new_y]),
                create_keyframe: keyframe_mode,
                });
            }
        }
        PropertyDisplayValue::Color(rgba) => {
            if let Some(new_rgba) = widgets::color_input(ui, name, *rgba, has_kf) {
                actions.property_edits.push(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: name.clone(),
                    value: PropertyValue::Color(new_rgba),
                create_keyframe: keyframe_mode,
                });
            }
        }
        PropertyDisplayValue::Scalar(s) => {
            // Determine widget type based on property name
            match name.as_str() {
                "opacity" | "fill_opacity" | "stroke_progress" => {
                    let val: f32 = s.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
                        .parse()
                        .unwrap_or(0.0);
                    if let Some(new_val) = widgets::slider_input(ui, name, val, 0.0, 1.0, has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Float(new_val),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                "rotation" => {
                    // Rotation is stored in radians, displayed in degrees
                    let val_deg: f32 = s.trim_end_matches('°').parse().unwrap_or(0.0);
                    if let Some(new_deg) = widgets::float_input(ui, name, val_deg, "°", has_kf) {
                        // Convert back to radians for the action
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Float(new_deg.to_radians()),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                "scale" => {
                    let val: f32 = s.parse().unwrap_or(1.0);
                    if let Some(new_val) = widgets::float_input(ui, name, val, "", has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Float(new_val),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                "stroke_width" => {
                    let val: f32 = s.parse().unwrap_or(0.0);
                    if let Some(new_val) = widgets::float_input(ui, name, val, "px", has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Float(new_val),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                _ => {
                    // Generic scalar: try float_input
                    let val: f32 = s.parse().unwrap_or(0.0);
                    if let Some(new_val) = widgets::float_input(ui, name, val, "", has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Float(new_val),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
            }
        }
        PropertyDisplayValue::Text(s) => {
            // Determine widget type based on property name
            match name.as_str() {
                "shape_type" => {
                    let variants = &[
                        "Rect", "Circle", "Line", "Ellipse", "Arc",
                        "Polygon", "Path", "Arrow", "Graph", "Plot",
                    ];
                    if let Some(new_val) = widgets::enum_selector(ui, name, s, variants, has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Text(new_val),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                "text_content" => {
                    if let Some(new_text) = widgets::text_input(ui, name, s, has_kf) {
                        actions.property_edits.push(PropertyEdit {
                            actor: actor_label.to_string(),
                            property: name.clone(),
                            value: PropertyValue::Text(new_text),
                        create_keyframe: keyframe_mode,
                        });
                    }
                }
                _ => {
                    // Read-only fallback for complex types (points, paths, etc.)
                    widgets::readonly_row(ui, name, s, has_kf);
                }
            }
        }
    }
}
