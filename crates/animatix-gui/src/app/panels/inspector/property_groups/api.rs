//! Property build/display helpers shared by the property group modules.

use animatix::timeline::{EntityId, LEGEND_SUM_VARIANTS, PropertyValue, ShapeType, ValueType};

use super::{PropertyEntry, PropertyKind};

pub(super) fn read_inspector_property_value(
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
pub(super) fn effect_value_to_kind(
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

pub(super) fn extension_value_to_kind(
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

pub(super) fn legend_participation_entry(mode: animatix::timeline::LegendMode) -> PropertyEntry {
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
pub(super) fn convert_for_display(value: PropertyValue, name: &str) -> PropertyValue {
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

pub(super) fn value_to_kind(value: PropertyValue, ty: ValueType, name: &str) -> PropertyKind {
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
