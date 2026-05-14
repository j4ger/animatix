use animatix::timeline::{
    ActorField, AnimationTrack, PropertyValue, ShapeType, ValueType,
    read_property_value, property_has_keyframes,
    allowed_property_indices, PROPERTY_REGISTRY,
};
use egui::{Color32, Vec2};

use crate::app::components;
use crate::app::theme::*;
use crate::app::panels::{PropertyEdit, PropertyValue as GuiPropertyValue, UiActions};

// ─── Data Structures ──────────────────────────────────────────────────────

pub(super) struct PropertyGroup {
    pub name: &'static str,
    pub icon: &'static str,
    pub properties: Vec<PropertyEntry>,
}

pub(super) struct PropertyEntry {
    pub name: &'static str,
    pub kind: PropertyKind,
    pub has_keyframes: bool,
    pub has_keyframe_at_current_time: bool,
}

pub(super) enum PropertyKind {
    Vec2 { x: f32, y: f32 },
    Float(f32),
    Color([f32; 4]),
    Text(String),
}

// ─── Group Builder (generic via registry) ─────────────────────────────────

pub(super) fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let indices = allowed_property_indices(track.kind);

    let mut geometry = Vec::new();
    let mut style = Vec::new();
    let mut shape = Vec::new();
    let mut text = Vec::new();
    let mut media = Vec::new();

    for &idx in &indices {
        let schema = &PROPERTY_REGISTRY[idx];
        // Skip group-resolution fields and build-time-only properties
        if schema.value_type == ValueType::BuildTimeOnly
            || schema.value_type == ValueType::CommandList
            || schema.value_type == ValueType::PointList
            || matches!(
                schema.field,
                ActorField::PositionBindingGroup
                    | ActorField::VectorShapeGroup
                    | ActorField::PlotDomainGroup
                    | ActorField::ContainerLayoutGroup
            )
        {
            continue;
        }

        let value = read_property_value(track, schema.field, time_ms);
        let has_kf = property_has_keyframes(track, schema.field);
        let has_kf_now = animatix::timeline::property_has_keyframe_at(track, schema.field, time_ms);

        let Some(value) = value else { continue };
        let value = convert_for_display(value, schema.name, track.kind);
        let kind = value_to_kind(value, schema.value_type, &schema.name);
        let entry = PropertyEntry {
            name: schema.name,
            kind,
            has_keyframes: has_kf,
            has_keyframe_at_current_time: has_kf_now,
        };

        match schema.field {
            ActorField::Position
            | ActorField::MotionOffset
            | ActorField::Size
            | ActorField::LayoutSize
            | ActorField::Rotation
            | ActorField::Scale
            | ActorField::PlacementMode
            | ActorField::PositionBinding => geometry.push(entry),
            ActorField::Color
            | ActorField::Opacity
            | ActorField::StrokeWidth
            | ActorField::StrokeColor
            | ActorField::StrokeProgress
            | ActorField::FillOpacity
            | ActorField::MorphOptions => style.push(entry),
            ActorField::ShapeType
            | ActorField::LineFrom
            | ActorField::LineTo
            | ActorField::ArcAngles
            | ActorField::Points
            | ActorField::VectorPaths => shape.push(entry),
            ActorField::TextContent
            | ActorField::FontFamily
            | ActorField::FontSize
            | ActorField::TextPaths => text.push(entry),
            ActorField::ImageData | ActorField::SvgPaths => media.push(entry),
            _ => {}
        }
    }

    let mut groups = Vec::new();
    if !geometry.is_empty() {
        groups.push(PropertyGroup {
            name: "Transform",
            icon: egui_phosphor::regular::ARROWS_OUT_CARDINAL,
            properties: geometry,
        });
    }
    if !style.is_empty() {
        groups.push(PropertyGroup {
            name: "Style",
            icon: egui_phosphor::regular::PAINT_BRUSH,
            properties: style,
        });
    }
    if !shape.is_empty() {
        groups.push(PropertyGroup {
            name: "Shape",
            icon: egui_phosphor::regular::SHAPES,
            properties: shape,
        });
    }
    if !text.is_empty() {
        groups.push(PropertyGroup {
            name: "Text",
            icon: egui_phosphor::regular::TEXT_T,
            properties: text,
        });
    }
    if !media.is_empty() {
        groups.push(PropertyGroup {
            name: "Media",
            icon: egui_phosphor::regular::FILM_STRIP,
            properties: media,
        });
    }

    groups
}

/// Convert a stored property value to a display value.
///
/// Several properties are stored on shared fields (e.g. `radius` is stored
/// in the `size` Vec2 track) or use half-extent internally while the UI
/// shows full dimensions.  This function normalises those cases.
fn convert_for_display(
    value: PropertyValue,
    name: &str,
    _kind: animatix::timeline::ActorKindId,
) -> PropertyValue {
    match name {
        // Radius properties are stored as the x or y component of `size`.
        "radius" | "radius_x" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[0])
            } else {
                value
            }
        }
        "radius_y" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[1])
            } else {
                value
            }
        }
        // `size` is stored as half-extents; the inspector shows full dimensions.
        "size" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::Vec2([v[0] * 2.0, v[1] * 2.0])
            } else {
                value
            }
        }
        // Angle properties are stored as components of `arc_angles`.
        "start_angle" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[0])
            } else {
                value
            }
        }
        "sweep_angle" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[1])
            } else {
                value
            }
        }
        _ => value,
    }
}

fn value_to_kind(value: PropertyValue, ty: ValueType, name: &str) -> PropertyKind {
    match (value, ty) {
        (PropertyValue::Vec2(v), _) => PropertyKind::Vec2 { x: v[0], y: v[1] },
        (PropertyValue::F32(v), _) => {
            if name == "rotation" {
                PropertyKind::Float(v.to_degrees())
            } else {
                PropertyKind::Float(v)
            }
        }
        (PropertyValue::Color(v), _) => PropertyKind::Color(v),
        (PropertyValue::String(v), _) => PropertyKind::Text(v),
        (PropertyValue::Vec4(v), ValueType::Color) => PropertyKind::Color(v),
        (PropertyValue::Vec4(v), _) => PropertyKind::Vec2 { x: v[0], y: v[1] },
        (PropertyValue::U32(v), ValueType::ShapeType) => {
            PropertyKind::Text(ShapeType::from(v).to_string())
        }
        (PropertyValue::U32(v), _) => PropertyKind::Float(v as f32),
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────

pub(super) fn render_property_group(
    ui: &mut egui::Ui,
    group: &PropertyGroup,
    actor_label: &str,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    let group_id = ui.id().with(("prop_group", group.name));
    let mut expanded = ui.data(|d| d.get_temp::<bool>(group_id)).unwrap_or(true);

    let header = components::Row::new(group.name)
        .height(ROW_M)
        .icon(Some(group.icon))
        .has_children(true)
        .expanded(expanded)
        .right(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(group.properties.len().to_string())
                        .size(FONT_SIZE_XS)
                        .color(TEXT_MUTED),
                )
                .selectable(false),
            );
        });
    let response = header.show(ui, group_id.with("header"));

    if response.row_clicked || response.chevron_clicked {
        expanded = !expanded;
        ui.data_mut(|d| d.insert_temp(group_id, expanded));
    }

    if expanded {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
        for entry in &group.properties {
            render_property_row(ui, actor_label, entry, actions, keyframe_mode);
        }
        ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_S);
    }
    ui.add_space(SPACE_S);
}

fn render_property_row(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    let row_height = ROW_S;
    let available = ui.available_width();
    let (row_rect, _response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if _response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, BG_HOVER);
    }

    let label_x = row_rect.min.x + SPACE_L;
    let baseline_y = row_rect.center().y;

    // Keyframe dot
    let dot_x = label_x;
    if entry.has_keyframe_at_current_time {
        let dot = egui::Rect::from_center_size(
            egui::pos2(dot_x, baseline_y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot, 1.5, AMBER);
    } else if entry.has_keyframes {
        let dot = egui::Rect::from_center_size(
            egui::pos2(dot_x, baseline_y),
            Vec2::new(4.0, 4.0),
        );
        ui.painter().rect_filled(dot, 2.0, TEXT_MUTED);
    }

    // Property label
    ui.painter().text(
        egui::pos2(label_x + 12.0, baseline_y),
        egui::Align2::LEFT_CENTER,
        entry.name,
        egui::TextStyle::Small.resolve(ui.style()),
        TEXT_SECONDARY,
    );

    // Input widget (right side)
    let input_width = 110.0_f32.min(available * 0.45);
    let input_rect = egui::Rect::from_min_size(
        egui::pos2(row_rect.max.x - input_width - SPACE_S, row_rect.min.y),
        Vec2::new(input_width, row_height),
    );

    match &entry.kind {
        PropertyKind::Vec2 { x, y } => {
            let mut nx = *x;
            let mut ny = *y;
            ui.scope_builder(egui::UiBuilder::new().max_rect(input_rect), |ui| {
                components::field(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                        let rx = ui.add(
                            egui::DragValue::new(&mut nx)
                                .speed(0.5)
                                .max_decimals(1),
                        );
                        let ry = ui.add(
                            egui::DragValue::new(&mut ny)
                                .speed(0.5)
                                .max_decimals(1),
                        );
                        if rx.drag_started() || ry.drag_started() {
                            actions.inspector_input_drag_started = true;
                        }
                        if rx.drag_stopped() || ry.drag_stopped() {
                            actions.inspector_input_drag_ended = true;
                        }
                        if rx.changed() || ry.changed() {
                            actions.property_edits.push(PropertyEdit {
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Vec2([nx, ny]),
                                create_keyframe: keyframe_mode,
                            });
                        }
                    });
                });
            });
        }
        PropertyKind::Float(v) => {
            let mut nv = *v;
            let is_angle = entry.name == "rotation";
            let is_01 = matches!(
                entry.name,
                "opacity" | "fill_opacity" | "stroke_progress"
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(input_rect), |ui| {
                components::field(ui, |ui| {
                    if is_01 {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                            let slider = ui.add(
                                egui::Slider::new(&mut nv, 0.0..=1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{:.2}", nv))
                                        .monospace()
                                        .size(FONT_SIZE_XS)
                                        .color(TEXT_PRIMARY),
                                )
                                .selectable(false),
                            );
                            if slider.drag_started() {
                                actions.inspector_input_drag_started = true;
                            }
                            if slider.drag_stopped() {
                                actions.inspector_input_drag_ended = true;
                            }
                            if slider.changed() {
                                actions.property_edits.push(PropertyEdit {
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::Float(nv),
                                    create_keyframe: keyframe_mode,
                                });
                            }
                        });
                    } else {
                        let response = ui.add(
                            egui::DragValue::new(&mut nv)
                                .speed(if is_angle { 0.5 } else { 0.1 })
                                .suffix(if is_angle { "°" } else { "" })
                                .max_decimals(if is_angle { 1 } else { 2 }),
                        );
                        if response.drag_started() {
                            actions.inspector_input_drag_started = true;
                        }
                        if response.drag_stopped() {
                            actions.inspector_input_drag_ended = true;
                        }
                        if response.changed() {
                            let out_val = if is_angle { nv.to_radians() } else { nv };
                            actions.property_edits.push(PropertyEdit {
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Float(out_val),
                                create_keyframe: keyframe_mode,
                            });
                        }
                    }
                });
            });
        }
        PropertyKind::Color(rgba) => {
            let mut color = Color32::from_rgba_premultiplied(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
                (rgba[3] * 255.0) as u8,
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(input_rect), |ui| {
                components::field(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                        let btn = ui.color_edit_button_srgba(&mut color);
                        if btn.changed() {
                            let [r, g, b, a] = color.to_array();
                            actions.property_edits.push(PropertyEdit {
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Color([
                                    r as f32 / 255.0,
                                    g as f32 / 255.0,
                                    b as f32 / 255.0,
                                    a as f32 / 255.0,
                                ]),
                                create_keyframe: keyframe_mode,
                            });
                        }
                    });
                });
            });
        }
        PropertyKind::Text(text) => {
            let mut buf = text.clone();
            ui.scope_builder(egui::UiBuilder::new().max_rect(input_rect), |ui| {
                components::field(ui, |ui| {
                    if entry.name == "shape_type" {
                        let variants: Vec<&str> = [
                            ShapeType::Rect, ShapeType::Circle, ShapeType::Line,
                            ShapeType::Ellipse, ShapeType::Arc, ShapeType::Polygon,
                            ShapeType::Path, ShapeType::Arrow, ShapeType::Graph, ShapeType::Plot,
                        ]
                        .iter()
                        .map(|st| st.as_str())
                        .collect();
                        egui::ComboBox::from_id_salt(ui.id().with(("enum", entry.name)))
                            .selected_text(text.as_str())
                            .width(input_width)
                            .show_ui(ui, |ui| {
                                for v in variants {
                                    if ui.selectable_label(v == text, v).clicked() {
                                        actions.property_edits.push(PropertyEdit {
                                            actor: actor_label.to_string(),
                                            property: entry.name.to_string(),
                                            value: GuiPropertyValue::Text(v.to_string()),
                                            create_keyframe: keyframe_mode,
                                        });
                                    }
                                }
                            });
                    } else if entry.name == "text_content" || entry.name == "text" {
                        let edit = egui::TextEdit::singleline(&mut buf)
                            .font(egui::TextStyle::Small)
                            .desired_width(input_width);
                        let response = ui.add(edit);
                        if response.changed() {
                            actions.property_edits.push(PropertyEdit {
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Text(buf),
                                create_keyframe: keyframe_mode,
                            });
                        }
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text.as_str())
                                    .size(FONT_SIZE_M)
                                    .color(TEXT_MUTED),
                            )
                            .selectable(false),
                        );
                    }
                });
            });
        }
    }
}
