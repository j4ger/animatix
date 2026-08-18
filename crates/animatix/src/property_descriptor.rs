//! Unified property descriptor shared by built-in and extension properties.

use animatix_syntax::schema::{PropertyId, PropertySpec, PropertyValueKind};
use animatix_syntax::typing::Type;

use crate::extension_context::ExtensionPropertySpec;

/// Neutral descriptor for one property, regardless of its origin.
///
/// Built-in properties can be converted from [`PropertySpec`] with runtime
/// metadata (such as injection flags); extension properties can be converted
/// from [`ExtensionPropertySpec`]. This is the first step toward one property
/// table for tooling and runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDescriptor {
    /// Stable property id used by plans and runtime lookup.
    pub id: PropertyId,
    /// Canonical source-text property name.
    pub name: String,
    /// Actor source types this property applies to.
    pub actor_types: Vec<String>,
    /// Type annotation consumed by analyzer/typechecker-compatible APIs.
    pub ty: Type,
    /// Finite value kind used by dynamic property tracks.
    pub value_kind: PropertyValueKind,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
}

impl PropertyDescriptor {
    /// Build a descriptor from a shared schema spec and runtime flags.
    pub fn from_schema(spec: &PropertySpec, injectable: bool) -> Self {
        Self {
            id: spec.id,
            name: spec.name.to_string(),
            actor_types: spec.actor_types.iter().map(|actor| actor.to_string()).collect(),
            ty: spec.ty.clone(),
            value_kind: spec.value_kind,
            injectable,
        }
    }

    /// Build a descriptor from an extension property spec.
    pub fn from_extension(spec: &ExtensionPropertySpec) -> Self {
        Self {
            id: spec.id,
            name: spec.name.clone(),
            actor_types: vec![spec.actor_type.clone()],
            ty: type_from_kind(spec.kind),
            value_kind: spec.kind,
            injectable: spec.injectable,
        }
    }
}

fn type_from_kind(kind: PropertyValueKind) -> Type {
    match kind {
        PropertyValueKind::F32 | PropertyValueKind::U32 => Type::Num,
        PropertyValueKind::Vec2 => Type::Vec2,
        PropertyValueKind::Vec4 => Type::Vec4,
        PropertyValueKind::String => Type::Str,
        PropertyValueKind::PointList => Type::List(Box::new(Type::Vec2)),
        PropertyValueKind::Generic => Type::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::schema::{PropertyId, PropertyValueKind};
    use animatix_syntax::typing::Type;

    #[test]
    fn schema_descriptor_uses_shared_fields() {
        let spec = PropertySpec {
            id: PropertyId(7),
            name: "size",
            actor_types: &["Rect"],
            ty: Type::Vec2,
            value_kind: PropertyValueKind::Vec2,
        };
        let descriptor = PropertyDescriptor::from_schema(&spec, true);
        assert_eq!(descriptor.id, PropertyId(7));
        assert_eq!(descriptor.name, "size");
        assert_eq!(descriptor.actor_types, vec!["Rect".to_string()]);
        assert_eq!(descriptor.ty, Type::Vec2);
        assert!(descriptor.injectable);
    }

    #[test]
    fn extension_descriptor_derives_type_from_value_kind() {
        let spec = ExtensionPropertySpec {
            id: PropertyId(1_000_000),
            actor_type: "Gauge".to_string(),
            name: "level".to_string(),
            kind: PropertyValueKind::F32,
            injectable: true,
        };
        let descriptor = PropertyDescriptor::from_extension(&spec);
        assert_eq!(descriptor.name, "level");
        assert_eq!(descriptor.ty, Type::Num);
        assert_eq!(descriptor.value_kind, PropertyValueKind::F32);
    }
}
