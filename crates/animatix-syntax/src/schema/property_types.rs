use crate::typing::{Type, transform_type};

use super::PropertyValueKind;

/// The type-system view of every built-in property, index-aligned with
/// [`animatix_core::property::PROPERTY_DESCRIPTORS`]. Supplies only the expression
/// `Type`; a mismatched row fails `property_types_line_up_with_descriptors`.
fn property_type_for_descriptor(desc: &animatix_core::property::PropertyDescriptor) -> Type {
    match desc.name {
        // `place`/`icon` descriptor kind is `Generic`, so pin their string type.
        "place" | "icon" => Type::Str,
        // `Vec4` falls back to `Union(Color, Str)`, but these rows are colours only.
        "active_color" | "inactive_color" | "backdrop_color" | "shadow_color" => Type::Color,
        // Numeric rows that also accept a string escape hatch (`gap: "auto"`).
        "bar_width" | "font_weight" | "gap" | "max_value" => {
            Type::Union(vec![Type::Num, Type::Str])
        },
        // A sum of `true`/`false` and an explicit label.
        "show_axis" | "show_labels" | "legend" => Type::Union(vec![Type::Bool, Type::Str]),
        "points" => Type::List(Box::new(Type::Vec2)),
        "dash_pattern" => Type::List(Box::new(Type::Num)),
        "transform" => transform_type(),
        "bounds" | "shadow" => Type::Vec4,
        // Runtime `Vec2`, authored as tuples or `"auto"`-style expressions.
        "levels" | "x_range" | "y_range" => Type::Any,

        _ => match desc.value_kind {
            PropertyValueKind::F32 | PropertyValueKind::U32 => Type::Num,
            PropertyValueKind::Bool => Type::Bool,
            PropertyValueKind::Vec2 => Type::Vec2,
            PropertyValueKind::Vec4 => Type::Union(vec![Type::Color, Type::Str]),
            PropertyValueKind::String => Type::Str,
            PropertyValueKind::PointList | PropertyValueKind::Generic => Type::Any,
        },
    }
}

pub(crate) fn raw_property_types() -> Vec<(&'static str, Type)> {
    animatix_core::property::PROPERTY_DESCRIPTORS
        .iter()
        .map(|desc| (desc.name, property_type_for_descriptor(desc)))
        .collect()
}
