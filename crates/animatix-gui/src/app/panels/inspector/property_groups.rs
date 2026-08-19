use animatix::renderer::text as animatix_text;
use animatix::timeline::{
    ActorField, AnimationTrack, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyValue, ShapeType,
    SumVariant, ValueType, allowed_property_indices, property_has_keyframes,
    read_property_value_or_default,
};
use egui::{Color32, Stroke, Vec2};
use eparts::widget::{Select, UiExt, color_to_hex_rgba};
use eparts::{NumberField, TextField};

use crate::app::commands::{
    ActionQueue, DocumentCommand, DragEvent, KeyframeCommand, PropertyEdit,
    PropertyValue as GuiPropertyValue, ShellAction,
};
use crate::app::components::button::Button;
use crate::app::components::row;
use crate::app::components::{Badge, ColorPicker};
use crate::app::design_tokens::spatial::inspector::{
    COL_GAP as INSPECTOR_COL_GAP, KF_BTN_WIDTH as INSPECTOR_KF_BTN_WIDTH,
    KF_COL_WIDTH as INSPECTOR_KF_COL_WIDTH, LABEL_MAX_WIDTH as INSPECTOR_LABEL_MAX_WIDTH,
    LABEL_MIN_WIDTH as INSPECTOR_LABEL_MIN_WIDTH, LABEL_WIDTH_FRAC as INSPECTOR_LABEL_WIDTH_FRAC,
};
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;

// ─── Data Structures ──────────────────────────────────────────────────────

pub(crate) struct PropertyGroup {
    pub name: &'static str,
    pub icon: &'static str,
    pub properties: Vec<PropertyEntry>,
}

pub(crate) struct PropertyEntry {
    pub name: String,
    pub kind: PropertyKind,
    pub has_keyframes: bool,
    pub has_keyframe_at_current_time: bool,
    pub keyframe_count: usize,
}

pub(crate) enum PropertyKind {
    Vec2 {
        x: f32,
        y: f32,
    },
    Float(f32),
    U32(u32),
    Bool(bool),
    Color([f32; 4]),
    Text(String),
    Union {
        variants: &'static [ValueType],
        value: PropertyValue,
    },
    Sum {
        variants: &'static [SumVariant],
        value: PropertyValue,
    },
    Enum {
        variants: &'static [&'static str],
        value: String,
    },
    EnumOwned {
        variants: Vec<String>,
        value: String,
    },
}

// ─── Group Builder (generic via registry) ─────────────────────────────────

pub(crate) fn build_property_groups(
    timeline: &animatix::timeline::Timeline,
    track: &AnimationTrack,
    time_ms: u64,
) -> Vec<PropertyGroup> {
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

        let value = read_property_value_or_default(track, schema, time_ms);
        let has_kf = property_has_keyframes(track, schema.field);
        let has_kf_now = animatix::timeline::property_has_keyframe_at(track, schema.field, time_ms);
        let kf_count = animatix::timeline::property_keyframe_count(track, schema.field);

        let value = convert_for_display(value, schema.name, track.kind);
        let kind = match schema.value_type {
            ValueType::Union(variants) => PropertyKind::Union { variants, value },
            ValueType::Sum(variants) => PropertyKind::Sum { variants, value },
            ValueType::Enum(variants) => PropertyKind::Enum {
                variants,
                value: match value {
                    PropertyValue::Enum(s) | PropertyValue::String(s) => s,
                    other => format!("{other:?}"),
                },
            },
            _ => value_to_kind(value, schema.value_type, schema.name),
        };
        let entry = PropertyEntry {
            name: schema.name.to_string(),
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
            _ => {},
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

    let mut legend_props = Vec::new();
    if track.kind == animatix::timeline::ActorKindId::Legend {
        legend_props.extend(legend_style_entries(track));
    }
    let mode = animatix::timeline::legend::legend_mode_for_track(track);
    if track.legend.color.is_some() || mode != animatix::timeline::LegendMode::Auto {
        legend_props.push(legend_participation_entry(track, mode));
    }
    if !legend_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Legend",
            icon: egui_phosphor::regular::CHART_LINE,
            properties: legend_props,
        });
    }

    let mut extension_props = Vec::new();
    let actor_type = track.actor_type.as_deref();
    for descriptor in timeline.extension_property_descriptors() {
        if !descriptor.actor_types.iter().any(|ty| Some(ty.as_str()) == actor_type) {
            continue;
        }
        let id = animatix::property_descriptor::runtime_id(&descriptor);
        let Some(value) = animatix::timeline::read_property_plan_slot(track, id, time_ms) else {
            continue;
        };
        extension_props.push(PropertyEntry {
            name: descriptor.name.clone(),
            kind: extension_value_to_kind(value, &descriptor.ty),
            has_keyframes: track.property_plan.keyframe_count(id) > 0,
            has_keyframe_at_current_time: track.property_plan.has_keyframe_at(id, time_ms),
            keyframe_count: track.property_plan.keyframe_count(id),
        });
    }
    if !extension_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Extensions",
            icon: egui_phosphor::regular::PLUG,
            properties: extension_props,
        });
    }

    groups
}

fn extension_value_to_kind(
    value: PropertyValue,
    ty: &animatix_syntax::typing::Type,
) -> PropertyKind {
    if let animatix_syntax::typing::Type::Enum(variants) = ty {
        let text = match &value {
            PropertyValue::Enum(s) | PropertyValue::String(s) => s.clone(),
            other => format!("{other:?}"),
        };
        return PropertyKind::EnumOwned {
            variants: variants.clone(),
            value: text,
        };
    }
    match value {
        PropertyValue::F32(v) => PropertyKind::Float(v),
        PropertyValue::U32(v) => PropertyKind::U32(v),
        PropertyValue::Bool(b) => PropertyKind::Bool(b),
        PropertyValue::Vec2(v) => PropertyKind::Vec2 { x: v[0], y: v[1] },
        PropertyValue::Vec4(v) | PropertyValue::Color(v) => PropertyKind::Color(v),
        PropertyValue::String(s) => PropertyKind::Text(s),
        other => PropertyKind::Text(format!("{other:?}")),
    }
}

fn legend_participation_entry(
    _track: &AnimationTrack,
    mode: animatix::timeline::LegendMode,
) -> PropertyEntry {
    let value = match &mode {
        animatix::timeline::LegendMode::Auto => PropertyValue::Variant {
            name: "auto".to_string(),
            value: Box::new(PropertyValue::Bool(true)),
        },
        animatix::timeline::LegendMode::Hidden => PropertyValue::Variant {
            name: "hidden".to_string(),
            value: Box::new(PropertyValue::Bool(false)),
        },
        animatix::timeline::LegendMode::Label(label) => PropertyValue::Variant {
            name: "label".to_string(),
            value: Box::new(PropertyValue::String(label.clone())),
        },
    };
    PropertyEntry {
        name: "legend".to_string(),
        kind: PropertyKind::Sum {
            variants: LEGEND_SUM_VARIANTS,
            value,
        },
        has_keyframes: false,
        has_keyframe_at_current_time: false,
        keyframe_count: 0,
    }
}

fn legend_style_entries(track: &AnimationTrack) -> Vec<PropertyEntry> {
    fn entry(name: &str, kind: PropertyKind) -> PropertyEntry {
        PropertyEntry {
            name: name.to_string(),
            kind,
            has_keyframes: false,
            has_keyframe_at_current_time: false,
            keyframe_count: 0,
        }
    }
    let label_color = track
        .legend
        .label_color
        .map(color_display)
        .unwrap_or_else(|| "auto".to_string());
    vec![
        entry("title", PropertyKind::Text(track.legend.title.clone())),
        entry("font_size", PropertyKind::Float(track.legend.font_size)),
        entry("label_color", PropertyKind::Text(label_color)),
        entry("swatch_size", PropertyKind::Float(track.legend.swatch_size)),
        entry("gap", PropertyKind::Float(track.legend.gap)),
        entry("text_max_width", PropertyKind::Float(track.legend.text_max_width)),
    ]
}

fn color_display(color: [f32; 4]) -> String {
    let r = (color[0] * 255.0).round() as u8;
    let g = (color[1] * 255.0).round() as u8;
    let b = (color[2] * 255.0).round() as u8;
    let a = color[3];
    if a >= 0.999 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{a:.2})")
    }
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
        },
        "radius_y" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[1])
            } else {
                value
            }
        },
        // `size` is stored as half-extents; the inspector shows full dimensions.
        "size" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::Vec2([v[0] * 2.0, v[1] * 2.0])
            } else {
                value
            }
        },
        // Angle properties are stored as components of `arc_angles`.
        "start_angle" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[0])
            } else {
                value
            }
        },
        "sweep_angle" => {
            if let PropertyValue::Vec2(v) = value {
                PropertyValue::F32(v[1])
            } else {
                value
            }
        },
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
        },
        (PropertyValue::Color(v), _) => PropertyKind::Color(v),
        (PropertyValue::String(v), _) => PropertyKind::Text(v),
        (PropertyValue::Enum(v), _) => PropertyKind::Text(v),
        (PropertyValue::Bool(v), _) => PropertyKind::Text(v.to_string()),
        (PropertyValue::Variant { value, .. }, _) => match value.as_ref() {
            PropertyValue::Bool(v) => PropertyKind::Text(v.to_string()),
            PropertyValue::String(v) => PropertyKind::Text(v.clone()),
            other => PropertyKind::Text(format!("{other:?}")),
        },
        (PropertyValue::Vec4(v), ValueType::Color) => PropertyKind::Color(v),
        (PropertyValue::Vec4(v), _) => PropertyKind::Vec2 { x: v[0], y: v[1] },
        (PropertyValue::U32(v), ValueType::ShapeType) => {
            PropertyKind::Text(ShapeType::from(v).to_string())
        },
        (PropertyValue::U32(v), _) => PropertyKind::U32(v),
        (PropertyValue::StringList(v), _) => PropertyKind::Text(format!("[{} items]", v.len())),
        (PropertyValue::PointList(v), _) => PropertyKind::Text(format!("[{} pts]", v.len())),
        (PropertyValue::CommandList(v), _) => PropertyKind::Text(v),
        (PropertyValue::Transform(v), _) => PropertyKind::Text(format!(
            "[{:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}]",
            v[0], v[1], v[2], v[3], v[4], v[5]
        )),
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
    active_scene: Option<&str>,
) {
    let theme = eparts::theme(ui);
    let group_id = ui.id().with(("prop_group", group.name));
    let mut expanded = ui.data(|d| d.get_temp::<bool>(group_id)).unwrap_or(true);
    let sp = spatial(ui);

    let header = row::Row::new(group.name)
        .height(sp.base.row_m)
        .icon(Some(group.icon))
        .has_children(true)
        .expanded(expanded)
        .right(|ui| {
            ui.add(Badge::new(group.properties.len().to_string()));
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
            s.visuals.widgets.inactive.bg_fill = theme.surface.widget;
            s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            s.visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            s.visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.active.bg_stroke = Stroke::NONE;
            s.visuals.widgets.open.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.open.bg_stroke = Stroke::NONE;
            s
        };
        ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_1);
        for entry in &group.properties {
            render_property_row(
                ui,
                actor_label,
                entry,
                commands,
                keyframe_mode,
                current_time_s,
                &flat_style,
                active_scene,
            );
        }
        ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_2);
    }
    ui.add_space(sp.base.space_4);
}

pub(crate) fn render_property_row(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    flat_style: &egui::Style,
    active_scene: Option<&str>,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let row_height = sp.inspector.row_height;
    let available = ui.available_width();
    let (row_rect, row_response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if row_response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, theme.surface.hover);
    }

    let baseline_y = row_rect.center().y;

    // ── Column layout ──
    // [KF dot:14px] [Label: ~42%] [Gap:6px] [Input area: flex] [Gap:4px] [KF btn:14px]
    let kf_col_right = row_rect.min.x + INSPECTOR_KF_COL_WIDTH;
    let label_width = (available * INSPECTOR_LABEL_WIDTH_FRAC)
        .clamp(INSPECTOR_LABEL_MIN_WIDTH, INSPECTOR_LABEL_MAX_WIDTH);
    let label_col_right = kf_col_right + label_width;
    let input_col_left = label_col_right + INSPECTOR_COL_GAP;
    let kf_btn_right = row_rect.max.x - sp.base.space_2;
    let kf_btn_left = kf_btn_right - INSPECTOR_KF_BTN_WIDTH;
    let input_col_right = kf_btn_left - sp.base.space_2;

    // ── Keyframe dot (centered in KF column) ──
    let dot_center = egui::pos2(row_rect.min.x + INSPECTOR_KF_COL_WIDTH / 2.0, baseline_y);
    if entry.has_keyframe_at_current_time {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(6.0, 6.0));
        ui.painter().rect_filled(dot, 2.0, theme.status.warning);
    } else if entry.has_keyframes {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(5.0, 5.0));
        ui.painter().rect_filled(dot, 2.5, theme.text.muted);
    }

    // ── Property label (truncated, vertically centered) ──
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(kf_col_right + sp.base.space_2, row_rect.min.y),
        egui::pos2(label_col_right, row_rect.max.y),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(entry.name.as_str())
                            .size(TextRole::BodyS.size())
                            .color(theme.text.secondary),
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
        ui.painter().rect_filled(
            input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0)),
            RADIUS_S,
            theme.surface.widget,
        );
    }

    // Flat widget styling passed from the group renderer (cached once per group).

    // ── Keyframe toggle button (far right) ──
    let kf_btn_rect = egui::Rect::from_min_size(
        egui::pos2(kf_btn_left, row_rect.min.y + (row_height - INSPECTOR_KF_BTN_WIDTH) * 0.5),
        Vec2::new(INSPECTOR_KF_BTN_WIDTH, INSPECTOR_KF_BTN_WIDTH),
    );
    let kf_btn_resp = ui.interact(
        kf_btn_rect,
        ui.id().with(("kf_btn", entry.name.as_str())),
        egui::Sense::click(),
    );

    // Draw diamond icon — dimmed when keyframe_mode is off and no keyframe exists
    let kf_color = if entry.has_keyframe_at_current_time {
        theme.status.warning
    } else if entry.has_keyframes {
        if kf_btn_resp.hovered() {
            theme.status.warning
        } else {
            theme.text.muted
        }
    } else if !keyframe_mode {
        // Show faint outline when keyframe mode is off
        if kf_btn_resp.hovered() {
            theme.text.disabled
        } else {
            Color32::TRANSPARENT
        }
    } else if kf_btn_resp.hovered() {
        theme.text.secondary
    } else {
        Color32::TRANSPARENT
    };
    if kf_color != Color32::TRANSPARENT {
        let center = kf_btn_rect.center();
        let size = if entry.has_keyframe_at_current_time {
            5.5
        } else {
            4.5
        };
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
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                Color32::TRANSPARENT,
                Stroke::new(STROKE_WIDTH, kf_color),
            ));
        }
    }
    // Click keyframe button to create a keyframe (when not already present)
    if kf_btn_resp.clicked() && keyframe_mode && !entry.has_keyframe_at_current_time {
        if let Some(value) = entry_to_gui_value(entry) {
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    value,
                    create_keyframe: true,
                })
                .into(),
            );
        }
    }

    // Context menu on filled diamond: delete keyframe or change easing
    if entry.has_keyframe_at_current_time {
        kf_btn_resp.context_menu(|ui| {
            ui.set_min_width(140.0);
            ui.strong(format!("Keyframe @ {:.2}s", current_time_s));
            ui.separator();
            if ui
                .add(Button::danger(format!("{} Delete", egui_phosphor::regular::TRASH)))
                .clicked()
            {
                commands.push_back(
                    KeyframeCommand::DeleteKeyframe {
                        scene: active_scene.map(ToOwned::to_owned),
                        actor: actor_label.to_string(),
                        property: entry.name.to_string(),
                        time_s: current_time_s,
                    }
                    .into(),
                );
                ui.close();
            }
            ui.menu_button(format!("{} Easing", egui_phosphor::regular::WAVEFORM), |ui| {
                for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                    let variant = animatix_syntax::easing::parse_easing_name(id_str)
                        .unwrap_or(animatix_syntax::easing::Easing::Linear);
                    if ui.stable_selectable_label(false, display_name).clicked() {
                        commands.push_back(
                            KeyframeCommand::SetKeyframeEasing {
                                scene: active_scene.map(ToOwned::to_owned),
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                time_s: current_time_s,
                                easing: variant,
                            }
                            .into(),
                        );
                        ui.close();
                    }
                }
            });
        });
    }

    // ── Input widget (inside input_rect, no extra frame) ──
    match &entry.kind {
        PropertyKind::Vec2 { x, y } => {
            let nx = *x;
            let ny = *y;
            let (a_label, b_label) = vec2_labels(&entry.name);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                            let half_w = (ui.available_width() - 3.0 * sp.base.space_2) / 2.0;
                            // Axis label for first component
                            ui.add_sized(
                                Vec2::new(16.0, row_height - sp.base.space_2),
                                egui::Label::new(
                                    egui::RichText::new(a_label)
                                        .size(TextRole::Micro.size())
                                        .color(theme.text.muted),
                                )
                                .selectable(false),
                            );
                            let mut nfx = nx as f64;
                            let rx = NumberField::new(&mut nfx)
                                .speed(0.5)
                                .desired_width((half_w - 16.0).max(20.0))
                                .show(ui);
                            // Axis label for second component
                            ui.add_sized(
                                Vec2::new(16.0, row_height - sp.base.space_2),
                                egui::Label::new(
                                    egui::RichText::new(b_label)
                                        .size(TextRole::Micro.size())
                                        .color(theme.text.muted),
                                )
                                .selectable(false),
                            );
                            let mut nfy = ny as f64;
                            let ry = NumberField::new(&mut nfy)
                                .speed(0.5)
                                .desired_width((half_w - 16.0).max(20.0))
                                .show(ui);
                            let nx_out = nfx as f32;
                            let ny_out = nfy as f32;
                            if rx.drag_started() || ry.drag_started() {
                                commands.push_back(ShellAction::Drag(
                                    DragEvent::InspectorInputDragStarted,
                                ));
                            }
                            if rx.drag_stopped() || ry.drag_stopped() {
                                commands.push_back(ShellAction::Drag(
                                    DragEvent::InspectorInputDragEnded,
                                ));
                            }
                            if rx.changed() || ry.changed() {
                                commands.push_back(
                                    DocumentCommand::PropertyEdit(PropertyEdit {
                                        time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Vec2([nx_out, ny_out]),
                                        create_keyframe: keyframe_mode,
                                    })
                                    .into(),
                                );
                            }
                        },
                    );
                },
            );
        },
        PropertyKind::Float(v) => {
            let mut nv = *v;
            let is_01 = matches!(
                entry.name.as_str(),
                "opacity" | "fill_opacity" | "stroke_progress" | "sepia" | "volume"
            );
            let is_angle = entry.name == "rotation" || entry.name == "hue_rotate";
            let unit = unit_suffix(&entry.name);
            if is_01 {
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                    |ui| {
                        *ui.style_mut() = flat_style.clone();
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                            |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                                let slider_w = ui.available_width() * 0.55;
                                let slider = ui.add_sized(
                                    Vec2::new(slider_w.max(40.0), row_height - sp.base.space_2),
                                    egui::Slider::new(&mut nv, 0.0..=1.0)
                                        .show_value(false)
                                        .trailing_fill(true),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{:.2}", nv))
                                            .monospace()
                                            .size(TextRole::Micro.size())
                                            .color(theme.text.primary),
                                    )
                                    .selectable(false),
                                );
                                if slider.drag_started() {
                                    commands.push_back(ShellAction::Drag(
                                        DragEvent::InspectorInputDragStarted,
                                    ));
                                }
                                if slider.drag_stopped() {
                                    commands.push_back(ShellAction::Drag(
                                        DragEvent::InspectorInputDragEnded,
                                    ));
                                }
                                if slider.changed() {
                                    commands.push_back(
                                        DocumentCommand::PropertyEdit(PropertyEdit {
                                            time_s: None,
                                            actor: actor_label.to_string(),
                                            property: entry.name.to_string(),
                                            value: GuiPropertyValue::F32(nv),
                                            create_keyframe: keyframe_mode,
                                        })
                                        .into(),
                                    );
                                }
                            },
                        );
                    },
                );
            } else {
                // Non-0..1 float: use eparts NumberField
                let mut nv = *v as f64;
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                    |ui| {
                        *ui.style_mut() = flat_style.clone();
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                            |ui| {
                                let response = NumberField::new(&mut nv)
                                    .speed(if is_angle { 0.5 } else { 0.1_f32 })
                                    .suffix(if is_angle { "°" } else { unit })
                                    .desired_width(ui.available_width())
                                    .show(ui);
                                if response.drag_started() {
                                    commands.push_back(ShellAction::Drag(
                                        DragEvent::InspectorInputDragStarted,
                                    ));
                                }
                                if response.drag_stopped() {
                                    commands.push_back(ShellAction::Drag(
                                        DragEvent::InspectorInputDragEnded,
                                    ));
                                }
                                if response.changed() {
                                    let out_val = if is_angle { nv.to_radians() } else { nv };
                                    commands.push_back(
                                        DocumentCommand::PropertyEdit(PropertyEdit {
                                            time_s: None,
                                            actor: actor_label.to_string(),
                                            property: entry.name.to_string(),
                                            value: GuiPropertyValue::F32(out_val as f32),
                                            create_keyframe: keyframe_mode,
                                        })
                                        .into(),
                                    );
                                }
                            },
                        );
                    },
                );
            }
        },
        PropertyKind::U32(v) => {
            let mut nv = *v as f64;
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            let response = NumberField::new(&mut nv)
                                .speed(0.1_f32)
                                .desired_width(ui.available_width())
                                .show(ui);
                            if response.drag_started() {
                                commands.push_back(ShellAction::Drag(
                                    DragEvent::InspectorInputDragStarted,
                                ));
                            }
                            if response.drag_stopped() {
                                commands.push_back(ShellAction::Drag(
                                    DragEvent::InspectorInputDragEnded,
                                ));
                            }
                            if response.changed() {
                                commands.push_back(
                                    DocumentCommand::PropertyEdit(PropertyEdit {
                                        time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::F32(nv as u32 as f32),
                                        create_keyframe: keyframe_mode,
                                    })
                                    .into(),
                                );
                            }
                        },
                    );
                },
            );
        },
        PropertyKind::Bool(value) => {
            let mut checked = *value;
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            if ui.checkbox(&mut checked, "").changed() {
                                commands.push_back(
                                    DocumentCommand::PropertyEdit(PropertyEdit {
                                        time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Bool(checked),
                                        create_keyframe: keyframe_mode,
                                    })
                                    .into(),
                                );
                            }
                        },
                    );
                },
            );
        },
        PropertyKind::Color(rgba) => {
            let mut color = Color32::from_rgba_premultiplied(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
                (rgba[3] * 255.0) as u8,
            );
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                            let response = ColorPicker::new(
                                ui.id().with(("color", entry.name.as_str())),
                                &mut color,
                            )
                            .swatches(&[
                                theme.accent.primary,
                                theme.status.success,
                                theme.status.warning,
                                theme.status.error,
                                theme.text.primary,
                                theme.text.muted,
                                theme.surface.surface,
                                theme.surface.widget,
                            ])
                            .show(ui);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(color_to_hex_rgba(color))
                                        .monospace()
                                        .size(TextRole::Micro.size())
                                        .color(theme.text.muted),
                                )
                                .selectable(false),
                            );
                            if response.changed {
                                let [r, g, b, a] = color.to_array();
                                commands.push_back(
                                    DocumentCommand::PropertyEdit(PropertyEdit {
                                        time_s: None,
                                        actor: actor_label.to_string(),
                                        property: entry.name.to_string(),
                                        value: GuiPropertyValue::Color([
                                            r as f32 / 255.0,
                                            g as f32 / 255.0,
                                            b as f32 / 255.0,
                                            a as f32 / 255.0,
                                        ]),
                                        create_keyframe: keyframe_mode,
                                    })
                                    .into(),
                                );
                            }
                        },
                    );
                },
            );
        },
        PropertyKind::Enum { variants, value } => {
            let mut selected = variants.iter().position(|variant| *variant == value);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.add_sized(
                        Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                        Select::new(
                            ui.id().with(("enum", entry.name.as_str())),
                            &mut selected,
                            variants,
                        ),
                    );
                },
            );
            if let Some(selected) = selected {
                let chosen = variants[selected];
                if chosen != value {
                    commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            value: GuiPropertyValue::String(chosen.to_string()),
                            create_keyframe: keyframe_mode,
                        })
                        .into(),
                    );
                }
            }
        },
        PropertyKind::EnumOwned { variants, value } => {
            let mut selected = variants.iter().position(|variant| variant == value);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    let selected_ref = &mut selected;
                    ui.add_sized(
                        Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                        Select::new(
                            ui.id().with(("enum_owned", entry.name.as_str())),
                            selected_ref,
                            variants.as_slice(),
                        ),
                    );
                },
            );
            if let Some(selected) = selected {
                let chosen = variants[selected].clone();
                if chosen != *value {
                    commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            value: GuiPropertyValue::String(chosen),
                            create_keyframe: keyframe_mode,
                        })
                        .into(),
                    );
                }
            }
        },
        PropertyKind::Sum { variants, value } => {
            let current_variant = match value {
                PropertyValue::Variant { name, .. } => name.as_str(),
                _ => "",
            };
            let mut selected =
                variants.iter().position(|variant| variant.name == current_variant).unwrap_or(0);
            let previous = selected;
            let mut buf = match value {
                PropertyValue::Variant { value: inner, .. } => match inner.as_ref() {
                    PropertyValue::String(s) => s.clone(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            let mut label_changed = false;
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
                            for (index, variant) in variants.iter().enumerate() {
                                ui.selectable_value(&mut selected, index, variant.name);
                            }
                            if variants
                                .get(selected)
                                .is_some_and(|v| v.value_type == ValueType::String)
                            {
                                let tf_resp = TextField::new(&mut buf)
                                    .desired_width(ui.available_width().max(60.0))
                                    .show(ui);
                                label_changed = tf_resp.changed;
                            }
                        },
                    );
                },
            );
            if selected != previous {
                let edit_value = gui_value_for_sum_variant(&variants[selected], buf.clone());
                commands.push_back(
                    DocumentCommand::PropertyEdit(PropertyEdit {
                        time_s: None,
                        actor: actor_label.to_string(),
                        property: entry.name.to_string(),
                        value: edit_value,
                        create_keyframe: keyframe_mode,
                    })
                    .into(),
                );
            } else if label_changed {
                commands.push_back(
                    DocumentCommand::PropertyEdit(PropertyEdit {
                        time_s: None,
                        actor: actor_label.to_string(),
                        property: entry.name.to_string(),
                        value: GuiPropertyValue::String(buf),
                        create_keyframe: keyframe_mode,
                    })
                    .into(),
                );
            }
        },
        PropertyKind::Union { variants, value } => {
            if variants.contains(&ValueType::Bool) && variants.contains(&ValueType::String) {
                let (mut selected, mut buf) = match value {
                    PropertyValue::Bool(true) => (0, String::new()),
                    PropertyValue::Bool(false) => (1, String::new()),
                    PropertyValue::String(label) => (2, label.clone()),
                    _ => (0, String::new()),
                };
                let previous = selected;
                let mut label_changed = false;
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                    |ui| {
                        *ui.style_mut() = flat_style.clone();
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                            |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
                                ui.selectable_value(&mut selected, 0, "Auto");
                                ui.selectable_value(&mut selected, 1, "Hidden");
                                ui.selectable_value(&mut selected, 2, "Label");
                                if selected == 2 {
                                    let tf_resp = TextField::new(&mut buf)
                                        .desired_width(ui.available_width().max(60.0))
                                        .show(ui);
                                    label_changed = tf_resp.changed;
                                }
                            },
                        );
                    },
                );
                if selected != previous {
                    let edit_value = match selected {
                        0 => GuiPropertyValue::Bool(true),
                        1 => GuiPropertyValue::Bool(false),
                        _ => GuiPropertyValue::String(buf.clone()),
                    };
                    commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            value: edit_value,
                            create_keyframe: keyframe_mode,
                        })
                        .into(),
                    );
                } else if label_changed {
                    commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            value: GuiPropertyValue::String(buf),
                            create_keyframe: keyframe_mode,
                        })
                        .into(),
                    );
                }
            } else {
                let text = match value {
                    PropertyValue::Bool(v) => v.to_string(),
                    PropertyValue::String(v) => v.clone(),
                    PropertyValue::F32(v) => format!("{v:.2}"),
                    PropertyValue::U32(v) => v.to_string(),
                    other => format!("{other:?}"),
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text.as_str())
                            .size(TextRole::Body.size())
                            .color(theme.text.muted),
                    )
                    .selectable(false),
                );
            }
        },
        PropertyKind::Text(text) => {
            let mut buf = text.clone();
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
                |ui| {
                    *ui.style_mut() = flat_style.clone();
                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                        |ui| {
                            if entry.name == "shape_type" {
                                use std::sync::OnceLock;
                                static SHAPE_VARIANTS: OnceLock<Vec<&'static str>> =
                                    OnceLock::new();
                                let variants = SHAPE_VARIANTS.get_or_init(|| {
                                    vec![
                                        ShapeType::Rect.as_str(),
                                        ShapeType::Ellipse.as_str(),
                                        ShapeType::Line.as_str(),
                                        ShapeType::Polygon.as_str(),
                                        ShapeType::Path.as_str(),
                                    ]
                                });
                                // Map current text to Option<usize> index for Select
                                let mut sel_idx = variants.iter().position(|v| *v == text);
                                ui.add_sized(
                                    Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                                    Select::new(
                                        ui.id().with(("enum", entry.name.as_str())),
                                        &mut sel_idx,
                                        &variants[..],
                                    ),
                                );
                                if let Some(idx) = sel_idx {
                                    let chosen = variants[idx];
                                    if chosen != text {
                                        commands.push_back(
                                            DocumentCommand::PropertyEdit(PropertyEdit {
                                                time_s: None,
                                                actor: actor_label.to_string(),
                                                property: entry.name.to_string(),
                                                value: GuiPropertyValue::String(chosen.to_string()),
                                                create_keyframe: keyframe_mode,
                                            })
                                            .into(),
                                        );
                                    }
                                }
                            } else if entry.name == "font_family" {
                                let font_ctx = crate::fonts::system_font_context();
                                let families = animatix_text::available_font_families(font_ctx);
                                egui::ComboBox::from_id_salt(
                                    ui.id().with(("font", entry.name.as_str())),
                                )
                                .selected_text(text.as_str())
                                .width(ui.available_width())
                                .show_ui(ui, |ui| {
                                    for family in families {
                                        let label = format!("Aa   {}", family);
                                        if ui
                                            .stable_selectable_label(family == *text, label)
                                            .clicked()
                                        {
                                            commands.push_back(
                                                DocumentCommand::PropertyEdit(PropertyEdit {
                                                    time_s: None,
                                                    actor: actor_label.to_string(),
                                                    property: entry.name.to_string(),
                                                    value: GuiPropertyValue::String(family),
                                                    create_keyframe: keyframe_mode,
                                                })
                                                .into(),
                                            );
                                        }
                                    }
                                });
                            } else if entry.name == "text_content"
                                || entry.name == "text"
                                || entry.name == "source"
                            {
                                let tf_resp = TextField::new(&mut buf)
                                    .desired_width(ui.available_width())
                                    .show(ui);
                                if tf_resp.changed {
                                    commands.push_back(
                                        DocumentCommand::PropertyEdit(PropertyEdit {
                                            time_s: None,
                                            actor: actor_label.to_string(),
                                            property: entry.name.to_string(),
                                            value: GuiPropertyValue::String(buf),
                                            create_keyframe: keyframe_mode,
                                        })
                                        .into(),
                                    );
                                }
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text.as_str())
                                            .size(TextRole::Body.size())
                                            .color(theme.text.muted),
                                    )
                                    .selectable(false),
                                );
                            }
                        },
                    );
                },
            );
        },
    }
}

fn gui_value_for_sum_variant(variant: &SumVariant, buf: String) -> GuiPropertyValue {
    match variant.literal {
        Some(animatix::timeline::property_registry::SumLiteral::Bool(value)) => {
            GuiPropertyValue::Bool(value)
        },
        Some(animatix::timeline::property_registry::SumLiteral::Str(value)) => {
            GuiPropertyValue::String(value.to_string())
        },
        None => match variant.value_type {
            ValueType::String => GuiPropertyValue::String(buf),
            ValueType::Bool => GuiPropertyValue::Bool(true),
            _ => GuiPropertyValue::String(buf),
        },
    }
}

/// Convert a PropertyEntry's current value to a GuiPropertyValue for keyframe creation.
fn entry_to_gui_value(entry: &PropertyEntry) -> Option<GuiPropertyValue> {
    match &entry.kind {
        PropertyKind::Vec2 { x, y } => Some(GuiPropertyValue::Vec2([*x, *y])),
        PropertyKind::Float(v) => Some(GuiPropertyValue::F32(*v)),
        PropertyKind::U32(v) => Some(GuiPropertyValue::F32(*v as f32)),
        PropertyKind::Bool(v) => Some(GuiPropertyValue::Bool(*v)),
        PropertyKind::Color(rgba) => Some(GuiPropertyValue::Color(*rgba)),
        PropertyKind::Text(t) => Some(GuiPropertyValue::String(t.clone())),
        PropertyKind::Enum { value, .. } => Some(GuiPropertyValue::String(value.clone())),
        PropertyKind::EnumOwned { value, .. } => Some(GuiPropertyValue::String(value.clone())),
        PropertyKind::Sum { variants, value } => match value {
            PropertyValue::Variant { name, value: inner } => {
                let index = variants.iter().position(|variant| variant.name == name).unwrap_or(0);
                let buf = match inner.as_ref() {
                    PropertyValue::String(s) => s.clone(),
                    _ => String::new(),
                };
                Some(gui_value_for_sum_variant(&variants[index], buf))
            },
            _ => None,
        },
        PropertyKind::Union { value, .. } => match value {
            PropertyValue::Bool(b) => Some(GuiPropertyValue::Bool(*b)),
            PropertyValue::String(s) => Some(GuiPropertyValue::String(s.clone())),
            PropertyValue::F32(v) => Some(GuiPropertyValue::F32(*v)),
            PropertyValue::U32(v) => Some(GuiPropertyValue::F32(*v as f32)),
            PropertyValue::Vec2(v) => Some(GuiPropertyValue::Vec2(*v)),
            PropertyValue::Color(v) | PropertyValue::Vec4(v) => Some(GuiPropertyValue::Color(*v)),
            _ => None,
        },
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
