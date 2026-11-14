use crate::typing::{Type, transform_type};

use animatix_core::property::{AuthoredTypeHint, PropertyValueKind};

/// The type-system view of every built-in property, derived from the shared
/// descriptor table. `AuthoredTypeHint::Default` preserves the historical
/// `PropertyValueKind` mapping; explicit hints cover properties whose source
/// type is more specific or broader than that storage kind.
fn property_type_for_descriptor(desc: &animatix_core::property::PropertyDescriptor) -> Type {
    match desc.authored_type {
        AuthoredTypeHint::Default => match desc.value_kind {
            PropertyValueKind::F32 | PropertyValueKind::U32 => Type::Num,
            PropertyValueKind::Bool => Type::Bool,
            PropertyValueKind::Vec2 => Type::Vec2,
            PropertyValueKind::Vec4 => Type::Union(vec![Type::Color, Type::Str]),
            PropertyValueKind::String => Type::Str,
            PropertyValueKind::PointList | PropertyValueKind::Generic => Type::Any,
        },
        AuthoredTypeHint::Str => Type::Str,
        AuthoredTypeHint::Color => Type::Color,
        AuthoredTypeHint::NumOrStr => Type::Union(vec![Type::Num, Type::Str]),
        AuthoredTypeHint::BoolOrStr => Type::Union(vec![Type::Bool, Type::Str]),
        AuthoredTypeHint::Vec2List => Type::List(Box::new(Type::Vec2)),
        AuthoredTypeHint::NumList => Type::List(Box::new(Type::Num)),
        AuthoredTypeHint::Transform => transform_type(),
        AuthoredTypeHint::Vec4 => Type::Vec4,
        AuthoredTypeHint::Any => Type::Any,
    }
}

pub(crate) fn raw_property_types() -> Vec<(&'static str, Type)> {
    animatix_core::property::PROPERTY_DESCRIPTORS
        .iter()
        .map(|desc| (desc.name, property_type_for_descriptor(desc)))
        .collect()
}
