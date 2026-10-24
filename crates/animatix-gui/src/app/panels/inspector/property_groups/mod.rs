use animatix::renderer::text as animatix_text;
use animatix::timeline::{
    ActorField, AnimationTrack, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyValue, ShapeType,
    SumVariant, ValueType, allowed_property_indices, property_has_keyframes,
    read_property_value_or_default,
};
use egui::{Color32, Stroke, Vec2};
use eparts::widget::{Select, UiExt, color_to_hex_rgba};
use eparts::{Checkbox, NumberField, Slider, TextField};

use super::property_groups_legend::legend_style_entries;
use crate::app::commands::{
    ActionQueue, DocumentCommand, DragEvent, KeyframeCommand, PropertyEdit,
    PropertyValue as GuiPropertyValue, ShellAction,
};
use crate::app::components::button::Button;
use crate::app::components::{Badge, ColorPicker, row, text_tooltip};
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

#[derive(Debug, PartialEq)]
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
    let indices = allowed_property_indices(&track.caps, &track.actor_type);

    let mut geometry = Vec::new();
    let mut layout = Vec::new();
    let mut style = Vec::new();
    let mut shape = Vec::new();
    let mut text = Vec::new();
    let mut plot = Vec::new();
    let mut annotation = Vec::new();
    let mut media = Vec::new();
    let mut audio = Vec::new();
    let mut captions = Vec::new();
    let mut visibility = Vec::new();
    let mut legend_props = Vec::new();
    let legend = timeline.entity(&track.label).and_then(|entity| entity.legend());
    let effects = timeline
        .entity(&track.label)
        .and_then(|entity| entity.effects())
        .cloned()
        .unwrap_or_default();
    for &idx in &indices {
        let schema = &PROPERTY_REGISTRY[idx];
        // Skip group-resolution fields and build-time-only properties
        if schema.value_type == ValueType::BuildTimeOnly
            || schema.value_type == ValueType::CommandList
            || schema.value_type == ValueType::PointList
            || matches!(
                schema.field,
                ActorField::PositionBindingGroup | ActorField::VectorShapeGroup
            )
        {
            continue;
        }

        let value = read_inspector_property_value(timeline, track, schema, time_ms);
        // One store: the registry-driven plan is authoritative for tagged
        // properties, and these helpers already consult it before the legacy
        // `tagged_tracks` map. Reading the map first here would show stale or
        // absent keyframe state for anything the engine wrote into the plan.
        let has_kf = property_has_keyframes(track, schema.field);
        let has_kf_now = animatix::timeline::property_has_keyframe_at(track, schema.field, time_ms);
        let kf_count = animatix::timeline::property_keyframe_count(track, schema.field);

        let value = convert_for_display(value, schema.name);
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
            // Geometry tier
            ActorField::Position
            | ActorField::MotionOffset
            | ActorField::Size
            | ActorField::LayoutSize
            | ActorField::Rotation
            | ActorField::Scale
            | ActorField::PlacementMode
            | ActorField::PositionBinding
            | ActorField::Transform
            | ActorField::MinWidth
            | ActorField::MinHeight
            | ActorField::MaxHeight => geometry.push(entry),

            // Container & Layout
            ActorField::ContainerLayoutGroup => layout.push(entry),

            // Plot & Domain
            ActorField::PlotDomainGroup => plot.push(entry),

            // Style tier
            ActorField::Color
            | ActorField::Opacity
            | ActorField::StrokeWidth
            | ActorField::StrokeColor
            | ActorField::StrokeProgress
            | ActorField::FillOpacity
            | ActorField::MorphOptions
            | ActorField::LineCap
            | ActorField::LineJoin
            | ActorField::DashPattern
            | ActorField::DashOffset
            | ActorField::Blend
            | ActorField::FillGradient
            | ActorField::StrokeGradient
            | ActorField::GradientExtend
            | ActorField::GradientSpace => style.push(entry),

            // Shape tier
            ActorField::ShapeType
            | ActorField::LineFrom
            | ActorField::LineTo
            | ActorField::ArcAngles
            | ActorField::Points
            | ActorField::VectorPaths
            | ActorField::HeadSize
            | ActorField::CornerRadius => shape.push(entry),

            // Text tier
            ActorField::TextContent
            | ActorField::FontFamily
            | ActorField::FontSize
            | ActorField::FontWeight
            | ActorField::FontStyle
            | ActorField::LineHeight
            | ActorField::LetterSpacing
            | ActorField::WordSpacing
            | ActorField::TextMaxWidth
            | ActorField::TextAlign
            | ActorField::Overflow
            | ActorField::Language
            | ActorField::CharProgress
            | ActorField::TextPaths
            | ActorField::Ascent
            | ActorField::Descent
            | ActorField::Baseline
            | ActorField::HighlightColor
            | ActorField::HighlightOpacity
            | ActorField::HighlightPadding
            | ActorField::HighlightRadius => text.push(entry),

            // Annotation tier
            ActorField::LabelAt
            | ActorField::CalloutTarget
            | ActorField::CalloutPlace
            | ActorField::CalloutStandoff
            | ActorField::CalloutToOffset => annotation.push(entry),

            // Media & Audio tier
            ActorField::ImageData | ActorField::SvgPaths => media.push(entry),
            ActorField::AudioSource | ActorField::AudioVolume => audio.push(entry),

            // Tagged union properties
            ActorField::Tagged(tag) => match tag {
                "arrow" | "routing" => shape.push(entry),
                "callout_place" => annotation.push(entry),
                "shadow" | "shadow_color" => style.push(entry),
                "rotate_x" | "rotate_y" | "perspective" | "idle" => geometry.push(entry),
                "solo" => visibility.push(entry),
                "comma" | "decimals" | "prefix" | "suffix" | "value" | "split_by"
                | "split_mask" | "split_offset_y" | "split_stagger" => text.push(entry),
                "transcript" if track.actor_type == "Audio" => audio.push(entry),
                "caption_mode" | "active_color" | "inactive_color" | "backdrop"
                | "backdrop_color" | "backdrop_radius" | "backdrop_padding" | "pop_scale"
                | "max_lines" | "transition_time" | "transcript" => captions.push(entry),
                s if s.starts_with("legend_") => {
                    legend_props.push(entry);
                },
                _ => style.push(entry),
            },
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
    if !layout.is_empty() {
        groups.push(PropertyGroup {
            name: "Layout",
            icon: egui_phosphor::regular::ROWS,
            properties: layout,
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
    if !plot.is_empty() {
        groups.push(PropertyGroup {
            name: "Plot & Coordinates",
            icon: egui_phosphor::regular::CHART_LINE,
            properties: plot,
        });
    }
    if !annotation.is_empty() {
        groups.push(PropertyGroup {
            name: "Annotation",
            icon: egui_phosphor::regular::TEXT_T,
            properties: annotation,
        });
    }
    if !media.is_empty() {
        groups.push(PropertyGroup {
            name: "Media",
            icon: egui_phosphor::regular::FILM_STRIP,
            properties: media,
        });
    }
    if !audio.is_empty() {
        groups.push(PropertyGroup {
            name: "Audio",
            icon: egui_phosphor::regular::SPEAKER_HIGH,
            properties: audio,
        });
    }
    if !captions.is_empty() {
        groups.push(PropertyGroup {
            name: "Captions",
            icon: egui_phosphor::regular::TEXT_ALIGN_CENTER,
            properties: captions,
        });
    }
    // `solo` is a scene-visibility switch, not a text/counter knob: every actor
    // type carries it (`Applicable::Everything`), so it gets its own group
    // instead of growing a bogus "Text" group on shapes.
    if !visibility.is_empty() {
        groups.push(PropertyGroup {
            name: "Visibility",
            icon: egui_phosphor::regular::EYE,
            properties: visibility,
        });
    }

    if track.caps.legend_host {
        legend_props.extend(legend_style_entries(legend));
    }
    let mode = animatix::timeline::legend::legend_mode_for_track(
        track,
        timeline.world().meta(track.id).first_seen_ms,
        Some(timeline.world()),
    );
    if legend.and_then(|legend| legend.color).is_some()
        || mode != animatix::timeline::LegendMode::Auto
    {
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
    let actor_type = track.actor_type.as_str();
    for descriptor in timeline.extension_property_descriptors() {
        if !descriptor.actor_types.iter().any(|ty| ty.as_str() == actor_type) {
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

    // Effect stages on a compositing scope. Entry names are `stage.param`;
    // the property-edit handler routes the dotted form to the effect chain.
    let mut effect_props = Vec::new();
    for stage in &effects.stages {
        let Some(effect) = animatix::timeline::effects::effect(&stage.kind) else {
            continue;
        };
        for spec in effect.params() {
            let track_opt = stage.params.get(spec.name.as_ref());
            let value = track_opt.and_then(|param| param.sample(time_ms)).unwrap_or_else(|| {
                animatix::timeline::effects::identity_to_property(spec.identity)
            });
            effect_props.push(PropertyEntry {
                name: format!("{}.{}", stage.label, spec.name),
                kind: effect_value_to_kind(spec.kind, value),
                has_keyframes: track_opt.is_some_and(|param| param.keyframe_count() > 0),
                has_keyframe_at_current_time: track_opt
                    .is_some_and(|param| param.has_keyframe_at(time_ms)),
                keyframe_count: track_opt.map_or(0, |param| param.keyframe_count()),
            });
        }
        effect_props.push(PropertyEntry {
            name: format!("{}.enabled", stage.label),
            kind: PropertyKind::Bool(matches!(
                stage.enabled.sample(time_ms),
                Some(PropertyValue::Bool(true))
            )),
            has_keyframes: stage.enabled.keyframe_count() > 0,
            has_keyframe_at_current_time: stage.enabled.has_keyframe_at(time_ms),
            keyframe_count: stage.enabled.keyframe_count(),
        });
    }
    if !effect_props.is_empty() {
        groups.push(PropertyGroup {
            name: "Effects",
            icon: egui_phosphor::regular::MAGIC_WAND,
            properties: effect_props,
        });
    }

    groups
}

fn read_inspector_property_value(
    timeline: &animatix::timeline::Timeline,
    track: &AnimationTrack,
    schema: &animatix::timeline::PropertySchema,
    time_ms: u64,
) -> PropertyValue {
    // Plot domains live on the plot payload's generator, not on a property
    // track, so they are read before the plan/legacy fallback below.
    if let Some(plot) = timeline.world().procedural_plot(track.id) {
        match schema.name {
            "x_domain" => {
                return PropertyValue::Vec2([plot.p_x_domain[0] as f32, plot.p_x_domain[1] as f32]);
            },
            "y_domain" => {
                return PropertyValue::Vec2([plot.p_y_domain[0] as f32, plot.p_y_domain[1] as f32]);
            },
            "t_domain" => {
                return PropertyValue::Vec2([plot.t_domain[0] as f32, plot.t_domain[1] as f32]);
            },
            _ => {},
        }
    }
    read_property_value_or_default(track, schema, time_ms)
}

/// Map an effect parameter value to the inspector's editable kind.
fn effect_value_to_kind(
    kind: animatix::timeline::effects::EffectParamKind,
    value: PropertyValue,
) -> PropertyKind {
    use animatix::timeline::effects::EffectParamKind as K;
    match kind {
        K::F32 => PropertyKind::Float(match value {
            PropertyValue::F32(v) => v,
            PropertyValue::U32(v) => v as f32,
            _ => 0.0,
        }),
        K::U32 => PropertyKind::U32(match value {
            PropertyValue::U32(v) => v,
            _ => 0,
        }),
        K::Bool => PropertyKind::Bool(matches!(value, PropertyValue::Bool(true))),
        K::Vec2 => match value {
            PropertyValue::Vec2([x, y]) => PropertyKind::Vec2 { x, y },
            _ => PropertyKind::Vec2 { x: 0.0, y: 0.0 },
        },
        K::Vec4 => match value {
            PropertyValue::Vec4(v) | PropertyValue::Color(v) => PropertyKind::Color(v),
            _ => PropertyKind::Color([0.0, 0.0, 0.0, 1.0]),
        },
    }
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

/// Convert a stored property value to a display value.
///
/// Several properties are stored on shared fields (e.g. `radius` is stored
/// in the `size` Vec2 track) or use half-extent internally while the UI
/// shows full dimensions.  This function normalises those cases.
fn convert_for_display(value: PropertyValue, name: &str) -> PropertyValue {
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
    if name == "caption_mode" {
        static CAPTION_MODES: &[&str] = &["word", "karaoke", "pop", "line", "block"];
        let current_val = match &value {
            PropertyValue::String(s) | PropertyValue::Enum(s) => s.clone(),
            other => format!("{other:?}"),
        };
        return PropertyKind::Enum {
            variants: CAPTION_MODES,
            value: current_val,
        };
    }
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
        (PropertyValue::Bool(v), _) => PropertyKind::Bool(v),
        (PropertyValue::Variant { value, .. }, _) => match value.as_ref() {
            PropertyValue::Bool(v) => PropertyKind::Bool(*v),
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
        (PropertyValue::F32List(v), _) => PropertyKind::Text(format!("[{} vals]", v.len())),
        (PropertyValue::Gradient(g), _) => PropertyKind::Text(format!("[{} stops]", g.stops.len())),
        (PropertyValue::CommandList(v), _) => PropertyKind::Text(v),
        (PropertyValue::Transform(v), _) => PropertyKind::Text(format!(
            "[{:.2}, {:.2}, {:.2}, {:.2}, {:.2}, {:.2}]",
            v[0], v[1], v[2], v[3], v[4], v[5]
        )),
    }
}

// ─── Rendering ────────────────────────────────────────────────────────────

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

mod inputs;
mod render;
#[cfg(test)]
mod tests;

pub(crate) use render::render_property_group;
