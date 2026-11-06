use super::{
    EffectParamSpecDef, EffectSpecDef, PrimitiveCategory, PrimitiveSpec, PropertyId, PropertySpec,
    PropertyValueKind, raw_property_types,
};

/// Built-in primitive metadata shared by runtime, GUI, and LSP tooling, derived
/// from the `animatix-std` catalog.
pub fn builtin_primitive_specs() -> Vec<PrimitiveSpec> {
    use animatix_core::caps::ActorCategory;
    animatix_std::CATALOG
        .iter()
        .map(|info| PrimitiveSpec {
            type_name: info.type_name.to_string(),
            display_name: info.display_name.to_string(),
            doc: info.doc.to_string(),
            default_stub: info.default_stub.map(|s| s.to_string()),
            category: match info.category {
                ActorCategory::Shape => PrimitiveCategory::Shape,
                ActorCategory::Text => PrimitiveCategory::Text,
                ActorCategory::Media => PrimitiveCategory::Media,
                ActorCategory::Plot => PrimitiveCategory::Plot,
                ActorCategory::Container => PrimitiveCategory::Container,
                ActorCategory::Annotation => PrimitiveCategory::Annotation,
            },
            icon_id: info.icon_id.to_string(),
            advanced: info.advanced,
            capabilities: info.capabilities,
            child_processing: info.child_processing,
        })
        .collect()
}
/// Built-in effect kinds, derived from the `animatix-std` catalog. Only built-ins
/// are described here, so type/display names stay `&'static str`.
pub fn effect_specs() -> &'static [EffectSpecDef] {
    use std::sync::OnceLock;
    static SPECS: OnceLock<Vec<EffectSpecDef>> = OnceLock::new();
    SPECS.get_or_init(|| {
        animatix_std::EFFECTS
            .iter()
            .map(|effect| EffectSpecDef {
                type_name: effect.type_name(),
                display_name: effect.display_name(),
                params: effect
                    .params()
                    .iter()
                    .map(|spec| EffectParamSpecDef {
                        name: spec.name.as_ref().into(),
                        kind: shared_kind(spec.kind),
                    })
                    .collect(),
            })
            .collect()
    })
}

/// Map the core effect parameter kind onto the shared property value kind.
fn shared_kind(kind: animatix_core::effect::EffectParamKind) -> PropertyValueKind {
    use animatix_core::effect::EffectParamKind as K;
    match kind {
        K::F32 => PropertyValueKind::F32,
        K::U32 => PropertyValueKind::U32,
        K::Bool => PropertyValueKind::Bool,
        K::Vec2 => PropertyValueKind::Vec2,
        K::Vec4 => PropertyValueKind::Vec4,
    }
}

/// Look up an effect spec by authored type name.
pub fn effect_spec(type_name: &str) -> Option<&'static EffectSpecDef> {
    effect_specs().iter().find(|spec| spec.type_name == type_name)
}

/// All known built-in property specs with stable ids following registry order,
/// so `property_id` round-trips without a second table.
pub fn property_specs() -> Vec<PropertySpec> {
    use std::sync::OnceLock;
    static SPECS: OnceLock<Vec<PropertySpec>> = OnceLock::new();
    SPECS
        .get_or_init(|| {
            let catalog: Vec<(&'static str, animatix_core::caps::ActorCaps)> =
                animatix_std::CATALOG
                    .iter()
                    .filter_map(|info| {
                        Some((info.static_type_name()?, animatix_std::caps_from_info(info)))
                    })
                    .collect();
            let types = raw_property_types();
            let descriptors = animatix_core::property::PROPERTY_DESCRIPTORS;
            debug_assert_eq!(
                types.len(),
                descriptors.len(),
                "the property type table is out of step with the descriptor table"
            );
            descriptors
                .iter()
                .enumerate()
                .map(|(index, descriptor)| {
                    let (type_name, ty) = &types[index];
                    debug_assert_eq!(
                        *type_name, descriptor.name,
                        "property type table order must match the descriptor table"
                    );
                    let types: Vec<&'static str> = catalog
                        .iter()
                        .filter(|(type_name, caps)| descriptor.applicable.includes(caps, type_name))
                        .map(|(type_name, _)| *type_name)
                        .collect();
                    PropertySpec {
                        id: PropertyId(index as u32),
                        name: descriptor.name,
                        actor_types: types.into_boxed_slice(),
                        ty: ty.clone(),
                        value_kind: descriptor.value_kind,
                    }
                })
                .collect()
        })
        .clone()
}

/// Property names shared by (nearly) every built-in actor type; extensions inherit
/// them so the analyzer does not flag `at`, `opacity`, `color`, etc. as unknown.
pub fn common_property_names() -> &'static [&'static str] {
    &[
        "anchor",
        "at",
        "blend",
        "camera_follow",
        "color",
        "height",
        "idle",
        "legend",
        "max_height",
        "max_item_width",
        "min_height",
        "min_width",
        "offset",
        "opacity",
        "parallax",
        "perspective",
        "position",
        "rotate_x",
        "rotate_y",
        "rotation",
        "scale",
        "shared_id",
        "shift",
        "size",
        "solo",
        "transform",
        "width",
    ]
}
