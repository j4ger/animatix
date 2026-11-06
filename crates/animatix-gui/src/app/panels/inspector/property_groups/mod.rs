use animatix::renderer::text as animatix_text;
use animatix::timeline::{
    ActorField, EntityId, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyValue, ShapeType,
    SumVariant, ValueType, allowed_property_indices,
};
use egui::{Color32, Stroke, Vec2};
use eparts::widget::{Select, UiExt, color_to_hex_rgba};
use eparts::{Checkbox, NumberField, Slider, TextField};

use super::property_groups_legend::legend_style_entries;
use crate::app::commands::{
    ActionQueue, Command, DragEvent, PropertyEdit, PropertyValue as GuiPropertyValue, ShellAction,
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

fn read_inspector_property_value(
    timeline: &animatix::timeline::Timeline,
    id: EntityId,
    schema: &animatix::timeline::PropertySchema,
    time_ms: u64,
) -> PropertyValue {
    // Plot domains live on the plot payload's generator, not on a property
    // track, so they are read before the World/registry fallback below.
    if let Some(plot) = timeline.world().procedural_plot(id) {
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
    // Read through the World field bundle: it is the shape-aware view, so
    // shape-tier properties (corner_radius, arc_angles, line_from/line_to,
    // head_size, shape_type, points, commands, vector_paths) do not show
    // their schema default when the actor actually carries a value.
    timeline.world().read_property_value_or_default(id, schema, time_ms)
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

fn legend_participation_entry(mode: animatix::timeline::LegendMode) -> PropertyEntry {
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

mod build;
mod inputs;
mod render;
#[cfg(test)]
mod tests;

pub(crate) use build::build_property_groups;
pub(crate) use render::render_property_group;
