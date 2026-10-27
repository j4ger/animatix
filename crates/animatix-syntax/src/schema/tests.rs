use animatix_core::property::{PROPERTY_DESCRIPTORS, PropertyValueKind};

use super::{
    ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveSpec,
    builtin_primitive_specs, common_property_names, property_specs, raw_property_types,
};
use crate::typing::Type;

/// The type-system view must line up with the core descriptor table: same
/// rows, same order. A rename or reorder here would otherwise mis-type a
/// property silently (the tables are joined by index).

#[test]
fn property_types_line_up_with_descriptors() {
    let types = raw_property_types();
    let descriptors = PROPERTY_DESCRIPTORS;
    assert_eq!(
        types.len(),
        descriptors.len(),
        "the property type table has {} rows but the descriptor table has {}",
        types.len(),
        descriptors.len()
    );
    for (index, ((name, _), descriptor)) in types.iter().zip(descriptors).enumerate() {
        assert_eq!(
            *name, descriptor.name,
            "row {index} names `{name}` here but `{}` in the descriptor table",
            descriptor.name
        );
    }
}

/// [`PropertyId`] is the descriptor row index, and serialized plans carry
/// those ids, so the order is an interface. This pins the total count and a
/// set of sentinel ids: appending rows is fine, but reordering or removing
/// one shifts every later id. Today's in-tree persistence stores source text
/// rather than ids, so a shift is not corruption — but it changes what any
/// stored id means for a plugin or a future format, so it stays a deliberate
/// decision made visible here.
#[test]
fn property_id_order_is_pinned() {
    let specs = property_specs();
    assert_eq!(
        specs.len(),
        140,
        "the built-in property count changed; update this pin deliberately (ids are persisted)"
    );
    // Dense, unique ids starting at zero — no gaps for a join to fall into.
    for (index, spec) in specs.iter().enumerate() {
        assert_eq!(spec.id.0 as usize, index);
    }
    // `transform`/`y_scale` moved 85->83 and 96->94 on 2026-09-14, when the
    // redundant `latex` and `math` body aliases were removed (both rows sat
    // before `transform`).
    for (name, expected) in [("align", 0), ("at", 3), ("transform", 83), ("y_scale", 94)] {
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("`{name}` missing from the schema"));
        assert_eq!(
            spec.id.0, expected,
            "`{name}` moved in the property table; ids are persisted, so this is an ABI break"
        );
    }
}

/// Every expression type that has a finite runtime kind must agree with the
/// descriptor's kind. This is the check whose absence let `solo` be declared
/// `Type::Bool` with a `Generic` kind (and the engine's `Bool`) without any
/// test noticing.
#[test]
fn declared_types_agree_with_descriptor_kinds() {
    /// Kinds a declared expression type may legitimately store as.
    ///
    /// `Num` is deliberately two-way: counts and indices (`cols`) store as
    /// `U32` while measurements store as `F32`, and the expression type
    /// cannot tell them apart — the engine's finer `ValueType` can, and its
    /// own totality test pins that side.
    fn kinds_of_type(ty: &Type) -> Option<&'static [PropertyValueKind]> {
        use PropertyValueKind::*;
        Some(match ty {
            Type::Num => &[F32, U32],
            Type::Bool => &[Bool],
            Type::Str => &[String],
            Type::Vec2 => &[Vec2],
            Type::Color | Type::Vec4 => &[Vec4],
            Type::List(inner) if matches!(**inner, Type::Vec2) => &[PointList],
            // Expression-only types carry no finite track kind.
            Type::Any
            | Type::Vec3
            | Type::Actor(_)
            | Type::Component(_)
            | Type::Scene
            | Type::List(_)
            | Type::Tuple(_)
            | Type::Union(_)
            | Type::Enum(_)
            | Type::Function { .. } => return None,
        })
    }

    for spec in property_specs() {
        // `Generic` is the catch-all track kind: it accepts whatever the
        // declared type produces (e.g. `legend` is written as a string and
        // stored as a mode), so there is nothing to contradict.
        if spec.value_kind == PropertyValueKind::Generic {
            continue;
        }
        let Some(kinds) = kinds_of_type(&spec.ty) else {
            continue;
        };
        assert!(
            kinds.contains(&spec.value_kind),
            "`{}` is declared `{:?}` but stores as {:?} (allowed: {kinds:?})",
            spec.name,
            spec.ty,
            spec.value_kind
        );
    }
}

#[test]
fn common_property_list_stays_in_sync_with_schema() {
    // The explicit `common_property_names()` list (inherited by extension
    // actor types) must match the properties in the schema that apply to a
    // dominant majority of built-in actor types. If this fails, the schema
    // gained or lost a near-universal property and the list should be
    // updated deliberately rather than relying on a silently shifting count.
    let specs = property_specs();
    let names = common_property_names();
    // Every listed name must exist in the schema.
    for name in names {
        let spec = specs
            .iter()
            .find(|spec| spec.name == *name)
            .unwrap_or_else(|| panic!("common property '{name}' is missing from the schema"));
        assert!(
            spec.actor_types.len() >= 15,
            "common property '{name}' only applies to {} built-in actor types; \
             it does not look universal and should be removed from common_property_names()",
            spec.actor_types.len()
        );
    }
    // The schema must not silently introduce new near-universal properties
    // that are missing from the list.
    for spec in specs.iter().filter(|spec| spec.actor_types.len() >= 15) {
        assert!(
            names.contains(&spec.name),
            "property '{}' applies to {} built-in actor types but is not in \
             common_property_names(); add it deliberately",
            spec.name,
            spec.actor_types.len()
        );
    }
}

#[test]
fn shared_schema_contains_common_properties() {
    let specs = property_specs();
    assert!(
        specs
            .iter()
            .any(|spec| spec.name == "position" && spec.actor_types.contains(&"Rect"))
    );
    assert!(specs.iter().any(|spec| {
        spec.name == "text" && spec.actor_types.contains(&"Text") && spec.ty == Type::Str
    }));
}

#[test]
fn math_matches_text_like_property_coverage() {
    // `Math` is a first-class Text-like primitive since 2026-08-28; every
    // property a text actor can receive must also list `Math` so the
    // analyzer and the GUI inspector do not flag `Math { text: ... }`.
    let specs = property_specs();
    for name in ["text", "font_size", "font_family", "line_height", "color"] {
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .unwrap_or_else(|| panic!("property '{name}' is missing from the schema"));
        assert!(
            spec.actor_types.contains(&"Math"),
            "property '{name}' must cover Math (first-class Text-like primitive)"
        );
    }
}

#[test]
fn transform_property_is_a_union() {
    let specs = property_specs();
    let transform = specs
        .iter()
        .find(|spec| spec.name == "transform" && spec.actor_types.contains(&"Rect"))
        .map(|spec| &spec.ty)
        .expect("Rect.transform exists");
    assert!(matches!(transform, Type::Union(_)));
}

#[test]
fn legend_accepts_its_bool_and_string_variants() {
    // The engine binds `legend` to `LEGEND_SUM_VARIANTS` (auto/hidden/label),
    // so `legend: false` is a Bool in the sum, not a type error. Typing it as
    // a plain Str made every decorative `legend: false` emit a
    // `type-mismatch` warning even though the value reached the screen.
    let legend = raw_property_types()
        .into_iter()
        .find(|(name, _)| *name == "legend")
        .map(|(_, ty)| ty)
        .expect("legend is a declared property");
    assert_eq!(legend, Type::Union(vec![Type::Bool, Type::Str]));
}

#[test]
fn property_ids_are_stable_and_unique() {
    let specs = property_specs();
    assert!(!specs.is_empty());
    let ids = specs.iter().map(|spec| spec.id).collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), specs.len());
}

#[test]
fn builtin_primitive_specs_cover_core_types() {
    let specs = builtin_primitive_specs();
    assert!(specs.iter().any(|spec| spec.type_name == "Rect"));
    assert!(specs.iter().any(|spec| spec.type_name == "Text"));
    assert!(specs.iter().any(|spec| spec.type_name == "Row"));
    assert!(specs.iter().any(|spec| spec.type_name == "PlotCurve"));
}

#[test]
fn primitive_spec_carries_capabilities_and_category() {
    let rect = PrimitiveSpec {
        type_name: "Rect".to_string(),
        display_name: "Rectangle".to_string(),
        doc: "Rectangle shape with fill and stroke.".to_string(),
        default_stub: Some("size: (100, 100)".to_string()),
        category: PrimitiveCategory::Shape,
        icon_id: "rect".to_string(),
        advanced: false,
        capabilities: PrimitiveCapabilities {
            vector_paths: true,
            ..PrimitiveCapabilities::default()
        },
        child_processing: ChildProcessingKind::Generic,
    };
    assert_eq!(rect.category.label(), "Shapes");
    assert!(rect.capabilities.vector_paths);
    assert!(!rect.capabilities.layout_container);
}

#[test]
fn child_processing_kind_matches_special_containers() {
    // Derived from the animatix-std catalog.
    let child_processing_of = |name: &str| {
        builtin_primitive_specs()
            .iter()
            .find(|spec| spec.type_name == name)
            .map(|spec| spec.child_processing)
    };
    assert_eq!(child_processing_of("Filter"), Some(ChildProcessingKind::Filter));
    assert_eq!(child_processing_of("Mask"), Some(ChildProcessingKind::Mask));
    assert_eq!(child_processing_of("Equation"), Some(ChildProcessingKind::Equation));
    assert_eq!(child_processing_of("Captions"), Some(ChildProcessingKind::Captions));
    assert_eq!(child_processing_of("Rect"), Some(ChildProcessingKind::Generic));
}
