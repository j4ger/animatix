use animatix::timeline::{
    allowed_property_indices, PROPERTY_REGISTRY, read_property_value_or_default,
    AnimationTrack, ValueType, ShapeType, ActorField,
    property_has_keyframes,
    PropertyValue,
};
use egui::{Color32, Stroke, Vec2};

use crate::app::components::row;
use crate::app::design_tokens::*;
use crate::app::commands::{ActionQueue, Command, DragEvent, ShellAction, PropertyEdit, PropertyValue as GuiPropertyValue};

// ─── Data Structures ──────────────────────────────────────────────────────

pub(crate) struct PropertyGroup {
    pub name: &'static str,
    pub icon: &'static str,
    pub properties: Vec<PropertyEntry>,
}

pub(crate) struct PropertyEntry {
    pub name: &'static str,
    pub kind: PropertyKind,
    pub has_keyframes: bool,
    pub has_keyframe_at_current_time: bool,
    pub keyframe_count: usize,
}

pub(crate) enum PropertyKind {
    Vec2 { x: f32, y: f32 },
    Float(f32),
    U32(u32),
    Color([f32; 4]),
    Text(String),
}

// ─── Group Builder (generic via registry) ─────────────────────────────────

pub(crate) fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let indices = allowed_property_indices(track.kind);

    let mut geometry = Vec::new();
    let mut style = Vec::new();
    let mut shape = Vec::new();
    let mut text = Vec::new();
    let mut media = Vec::new();
    let mut effects = Vec::new();
    let mut audio = Vec::new();

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

        let value = read_property_value_or_default(track, schema.field, time_ms, track.kind);
        let has_kf = property_has_keyframes(track, schema.field);
        let has_kf_now = animatix::timeline::property_has_keyframe_at(track, schema.field, time_ms);
        let kf_count = animatix::timeline::property_keyframe_count(track, schema.field);

        let value = convert_for_display(value, schema.name, track.kind);
        let kind = value_to_kind(value, schema.value_type, schema.name);
        let entry = PropertyEntry {
            name: schema.name,
            kind,
            has_keyframes: has_kf,
            has_keyframe_at_current_time: has_kf_now,
            keyframe_count: kf_count,
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
            ActorField::FilterBlur
            | ActorField::FilterBrightness
            | ActorField::FilterContrast
            | ActorField::FilterSaturate
            | ActorField::FilterHueRotate
            | ActorField::FilterSepia => effects.push(entry),
            ActorField::AudioSource | ActorField::AudioVolume => audio.push(entry),
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
    if !effects.is_empty() {
        groups.push(PropertyGroup {
            name: "Effects",
            icon: egui_phosphor::regular::MAGIC_WAND,
            properties: effects,
        });
    }
    if !audio.is_empty() {
        groups.push(PropertyGroup {
            name: "Audio",
            icon: egui_phosphor::regular::SPEAKER_HIGH,
            properties: audio,
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
        (PropertyValue::U32(v), _) => PropertyKind::U32(v),
        (PropertyValue::PointList(v), _) => {
            PropertyKind::Text(format!("[{} pts]", v.len()))
        }
        (PropertyValue::CommandList(v), _) => PropertyKind::Text(v),
        (PropertyValue::PlacementMode(v), _) => PropertyKind::Text(format!("{:?}", v)),
        (PropertyValue::MorphOptions(v), _) => PropertyKind::Text(format!("{:?}", v)),
        (PropertyValue::Transform(v), _) => PropertyKind::Text(format!("[{:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}]", v[0], v[1], v[2], v[3], v[4], v[5])),
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────

pub(crate) fn render_property_group(
    ui: &mut egui::Ui,
    group: &PropertyGroup,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
) {
    let group_id = ui.id().with(("prop_group", group.name));
    let mut expanded = ui.data(|d| d.get_temp::<bool>(group_id)).unwrap_or(true);

    let header = row::Row::new(group.name)
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
        // Cache flat widget style once for all rows in this group
        let flat_style = {
            let mut s = (**ui.style()).clone();
            s.visuals.extreme_bg_color = Color32::TRANSPARENT;
            s.visuals.widgets.inactive.bg_fill = BG_WIDGET;
            s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            s.visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            s.visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.active.bg_stroke = Stroke::NONE;
            s.visuals.widgets.open.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.open.bg_stroke = Stroke::NONE;
            s
        };
        ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_XS);
        for entry in &group.properties {
            render_property_row(ui, actor_label, entry, commands, keyframe_mode, current_time_s, &flat_style);
        }
        ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_S);
    }
    ui.add_space(SPACE_L);
}

pub(crate) fn render_property_row(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    flat_style: &egui::Style,
) {
    let row_height = INSPECTOR_ROW_HEIGHT;
    let available = ui.available_width();
    let (row_rect, row_response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if row_response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, BG_HOVER);
    }

    let baseline_y = row_rect.center().y;

    // ── Column layout ──
    // [KF dot:14px] [Label: ~42%] [Gap:6px] [Input area: flex] [Gap:4px] [KF btn:14px]
    let kf_col_right = row_rect.min.x + INSPECTOR_KF_COL_WIDTH;
    let label_width = (available * INSPECTOR_LABEL_WIDTH_FRAC)
        .clamp(INSPECTOR_LABEL_MIN_WIDTH, INSPECTOR_LABEL_MAX_WIDTH);
    let label_col_right = kf_col_right + label_width;
    let input_col_left = label_col_right + INSPECTOR_COL_GAP;
    let kf_btn_right = row_rect.max.x - SPACE_S;
    let kf_btn_left = kf_btn_right - INSPECTOR_KF_BTN_WIDTH;
    let input_col_right = kf_btn_left - SPACE_S;

    // ── Keyframe dot (centered in KF column) ──
    let dot_center = egui::pos2(row_rect.min.x + INSPECTOR_KF_COL_WIDTH / 2.0, baseline_y);
    if entry.has_keyframe_at_current_time {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(6.0, 6.0));
        ui.painter().rect_filled(dot, 2.0, AMBER);
    } else if entry.has_keyframes {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(5.0, 5.0));
        ui.painter().rect_filled(dot, 2.5, TEXT_MUTED);
    }

    // ── Property label (truncated, vertically centered) ──
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(kf_col_right + SPACE_S, row_rect.min.y),
        egui::pos2(label_col_right, row_rect.max.y),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(entry.name)
                            .size(FONT_SIZE_S)
                            .color(TEXT_SECONDARY),
                    )
                    .truncate()
                    .selectable(false),
                );
            },
        );
    });

    // ── Input area (flat, no extra frame) ──
    let input_rect = egui::Rect::from_min_max(
        egui::pos2(input_col_left, row_rect.min.y),
        egui::pos2(input_col_right, row_rect.max.y),
    );

    // Subtle background on hover for the input area
    let input_hover = ui.rect_contains_pointer(input_rect);
    if input_hover {
        ui.painter().rect_filled(input_rect, RADIUS_S, BG_WIDGET);
    }

    // Flat widget styling passed from the group renderer (cached once per group).

    // ── Keyframe toggle button (far right) ──
    let kf_btn_rect = egui::Rect::from_min_size(
        egui::pos2(kf_btn_left, row_rect.min.y + (row_height - INSPECTOR_KF_BTN_WIDTH) * 0.5),
        Vec2::new(INSPECTOR_KF_BTN_WIDTH, INSPECTOR_KF_BTN_WIDTH),
    );
    let kf_btn_resp = ui.interact(kf_btn_rect, ui.id().with(("kf_btn", entry.name)), egui::Sense::click());

    // Draw diamond icon — dimmed when keyframe_mode is off and no keyframe exists
    let kf_color = if entry.has_keyframe_at_current_time {
        AMBER
    } else if entry.has_keyframes {
        if kf_btn_resp.hovered() { AMBER } else { TEXT_MUTED }
    } else if !keyframe_mode {
        // Show faint outline when keyframe mode is off
        if kf_btn_resp.hovered() { TEXT_DISABLED } else { Color32::TRANSPARENT }
    } else if kf_btn_resp.hovered() { TEXT_SECONDARY } else { Color32::TRANSPARENT };
    if kf_color != Color32::TRANSPARENT {
        let center = kf_btn_rect.center();
        let size = if entry.has_keyframe_at_current_time { 5.5 } else { 4.5 };
        let half = size * 0.5;
        let points = vec![
            center + Vec2::new(0.0, -half),
            center + Vec2::new(half, 0.0),
            center + Vec2::new(0.0, half),
            center + Vec2::new(-half, 0.0),
        ];
        if entry.has_keyframe_at_current_time {
            ui.painter().add(egui::Shape::convex_polygon(points, kf_color, Stroke::NONE));
        } else {
            ui.painter().add(egui::Shape::convex_polygon(points, Color32::TRANSPARENT, Stroke::new(STROKE_WIDTH, kf_color)));
        }
    }
    // Click keyframe button to create a keyframe (when not already present)
    if kf_btn_resp.clicked() && keyframe_mode && !entry.has_keyframe_at_current_time {
        if let Some(value) = entry_to_gui_value(entry) {
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                actor: actor_label.to_string(),
                property: entry.name.to_string(),
                value,
                create_keyframe: true,
            })));
        }
    }

    // Context menu on filled diamond: delete keyframe or change easing
    if entry.has_keyframe_at_current_time {
        kf_btn_resp.context_menu(|ui| {
            ui.set_min_width(140.0);
            ui.strong(format!("Keyframe @ {:.2}s", current_time_s));
            ui.separator();
            if ui.button(format!("{} Delete", egui_phosphor::regular::TRASH)).clicked() {
                commands.push_back(ShellAction::Command(Command::DeleteKeyframe {
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    time_s: current_time_s,
                }));
                ui.close();
            }
            ui.menu_button(format!("{} Easing", egui_phosphor::regular::WAVEFORM), |ui| {
                for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                    let variant = animatix_syntax::easing::parse_easing_name(id_str).unwrap_or(animatix_syntax::easing::Easing::Linear);
                    if ui.selectable_label(false, display_name).clicked() {
                        commands.push_back(ShellAction::Command(Command::SetKeyframeEasing {
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            time_s: current_time_s,
                            easing: variant,
                        }));
                        ui.close();
                    }
                }
            });
        });
    }

    // ── Input widget (inside input_rect, no extra frame) ──
    match &entry.kind {
        PropertyKind::Vec2 { x, y } => {
            let mut nx = *x;
            let mut ny = *y;
            let (a_label, b_label) = vec2_labels(entry.name);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);
                            let half_w = ui.available_width() / 2.0 - 2.0;
                            let rx = ui.add_sized(
                                Vec2::new(half_w.max(30.0), row_height - SPACE_S),
                                egui::DragValue::new(&mut nx)
                                    .speed(0.5)
                                    .max_decimals(1)
                                    .prefix(a_label),
                            );
                            let ry = ui.add_sized(
                                Vec2::new(half_w.max(30.0), row_height - SPACE_S),
                                egui::DragValue::new(&mut ny)
                                    .speed(0.5)
                                    .max_decimals(1)
                                    .prefix(b_label),
                            );
                            if rx.drag_started() || ry.drag_started() {
                                commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                            }
                            if rx.drag_stopped() || ry.drag_stopped() {
                                commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                            }
                            if rx.changed() || ry.changed() {
                                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::Vec2([nx, ny]),
                                    create_keyframe: keyframe_mode,
                                })));

                                }
                            },
                        );
                    },
                );
            }
            PropertyKind::Float(v) => {
            let mut nv = *v;
            let is_01 = matches!(
                entry.name,
                "opacity" | "fill_opacity" | "stroke_progress" | "sepia" | "volume"
            );
            let is_angle = entry.name == "rotation" || entry.name == "hue_rotate";
            let unit = unit_suffix(entry.name);
            if is_01 {
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                    |ui| {
                        *ui.style_mut() = flat_style.clone();
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                            |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);
                                let slider_w = ui.available_width() * 0.55;
                                let slider = ui.add_sized(
                                    Vec2::new(slider_w.max(40.0), row_height - SPACE_S),
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
                                    commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                                }
                                if slider.drag_stopped() {
                                    commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                                }
                                if slider.changed() {
                                    commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Float(nv),
                                        create_keyframe: keyframe_mode,
                                    })));
                                }
                            },
                        );
                    },
                );
            } else {
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                    |ui| {
                        *ui.style_mut() = flat_style.clone();
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                            |ui| {
                                let response = ui.add_sized(
                                    Vec2::new(ui.available_width(), row_height - SPACE_S),
                                    egui::DragValue::new(&mut nv)
                                        .speed(if is_angle { 0.5 } else { 0.1 })
                                        .suffix(if is_angle { "°" } else { unit })
                                        .max_decimals(if is_angle { 1 } else { 2 }),
                                );
                                if response.drag_started() {
                                    commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                                }
                                if response.drag_stopped() {
                                    commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                                }
                                if response.changed() {
                                    let out_val = if is_angle { nv.to_radians() } else { nv };
                                    commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Float(out_val),
                                        create_keyframe: keyframe_mode,
                                    })));
                                }
                            },
                        );
                    },
                );
            }
        }
        PropertyKind::U32(v) => {
            let mut nv = *v as i64;
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            let response = ui.add_sized(
                                Vec2::new(ui.available_width(), row_height - SPACE_S),
                                egui::DragValue::new(&mut nv)
                                    .speed(0.1)
                                    .max_decimals(0),
                            );
                            if response.drag_started() {
                                commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                            }
                            if response.drag_stopped() {
                                commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                            }
                            if response.changed() {
                                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::Float(nv as f32),
                                    create_keyframe: keyframe_mode,
                                })));
                            }
                        },
                    );
                },
            );
        }
        PropertyKind::Color(rgba) => {
            let mut color = Color32::from_rgba_premultiplied(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
                (rgba[3] * 255.0) as u8,
            );
            let hex = format!(
                "#{:02x}{:02x}{:02x}",
                color.r(),
                color.g(),
                color.b()
            );
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);
                            let btn = ui.color_edit_button_srgba(&mut color);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&hex)
                                        .monospace()
                                        .size(FONT_SIZE_XS)
                                        .color(TEXT_MUTED),
                                )
                                .selectable(false),
                            );
                            if btn.changed() {
                                let [r, g, b, a] = color.to_array();
                                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::Color([
                                        r as f32 / 255.0,
                                        g as f32 / 255.0,
                                        b as f32 / 255.0,
                                        a as f32 / 255.0,
                                    ]),
                                    create_keyframe: keyframe_mode,
                                })));
                            }
                        },
                    );
                },
            );
        }
        PropertyKind::Text(text) => {
            let mut buf = text.clone();
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(SPACE_S, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            if entry.name == "shape_type" {
                                let variants: Vec<&str> = [
                                    ShapeType::Rect, ShapeType::Ellipse, ShapeType::Line,
                                    ShapeType::Polygon, ShapeType::Path,
                                ]
                                .iter()
                                .map(|st| st.as_str())
                                .collect();
                                egui::ComboBox::from_id_salt(ui.id().with(("enum", entry.name)))
                                    .selected_text(text.as_str())
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for v in variants {
                                            if ui.selectable_label(v == text, v).clicked() {
                                                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                                    actor: actor_label.to_string(),
                                                    property: entry.name.to_string(),
                                                    value: GuiPropertyValue::Text(v.to_string()),
                                                    create_keyframe: keyframe_mode,
                                                })));
                                            }
                                        }
                                    });
                            } else if entry.name == "font_family" {
                                use std::sync::OnceLock;
                                static FONT_CONTEXT: OnceLock<animatix::renderer::text::FontContext> = OnceLock::new();
                                let font_ctx = FONT_CONTEXT.get_or_init(animatix::renderer::text::FontContext::new);
                                let families = animatix::renderer::text::available_font_families(font_ctx);
                                egui::ComboBox::from_id_salt(ui.id().with(("font", entry.name)))
                                    .selected_text(text.as_str())
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for family in families {
                                            let label = format!("Aa   {}", family);
                                            if ui.selectable_label(family == *text, label).clicked() {
                                                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                                    actor: actor_label.to_string(),
                                                    property: entry.name.to_string(),
                                                    value: GuiPropertyValue::Text(family),
                                                    create_keyframe: keyframe_mode,
                                                })));
                                            }
                                        }
                                    });
                            } else if entry.name == "text_content" || entry.name == "text" || entry.name == "source" {
                                let edit = egui::TextEdit::singleline(&mut buf)
                                    .font(egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional))
                                    .desired_width(ui.available_width());
                                let response = ui.add(edit);
                                if response.changed() {
                                    commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Text(buf),
                                        create_keyframe: keyframe_mode,
                                    })));
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
                        },
                    );
                },
            );
        }
    }
}

/// Convert a PropertyEntry's current value to a GuiPropertyValue for keyframe creation.
fn entry_to_gui_value(entry: &PropertyEntry) -> Option<GuiPropertyValue> {
    match &entry.kind {
        PropertyKind::Vec2 { x, y } => Some(GuiPropertyValue::Vec2([*x, *y])),
        PropertyKind::Float(v) => Some(GuiPropertyValue::Float(*v)),
        PropertyKind::U32(v) => Some(GuiPropertyValue::Float(*v as f32)),
        PropertyKind::Color(rgba) => Some(GuiPropertyValue::Color(*rgba)),
        PropertyKind::Text(t) => Some(GuiPropertyValue::Text(t.clone())),
    }
}

/// Return a unit suffix for numeric property names.
fn unit_suffix(name: &str) -> &'static str {
    match name {
        "position" | "motion_offset" | "size" | "layout_size" | "line_from" | "line_to" => "px",
        "stroke_width" => "px",
        "rotation" => "°",
        "scale" => "×",
        "opacity" | "fill_opacity" | "stroke_progress" => "",
        "font_size" => "pt",
        "blur" => "px",
        "brightness" | "contrast" | "saturate" | "sepia" => "",
        "hue_rotate" => "°",
        "volume" => "%",
        _ => "",
    }
}

/// Return axis labels for Vec2 property names.
///
/// Position-like properties use X/Y; size-like properties use W/H.
fn vec2_labels(name: &str) -> (&'static str, &'static str) {
    match name {
        "size" | "layout_size" => ("W: ", "H: "),
        _ => ("X: ", "Y: "),
    }
}
