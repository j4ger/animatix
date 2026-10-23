use super::*;
fn property_index(name: &str) -> Option<usize> {
    resolved_properties().get(name).map(|(index, _)| *index)
}

/// Look up a property schema by name.
///
/// Uses binary search over the sorted `PROPERTY_REGISTRY`.
/// Returns `None` if no property with that name exists.
pub fn lookup_property(name: &str) -> Option<&'static PropertySchema> {
    property_index(name).map(|i| &PROPERTY_REGISTRY[i])
}

/// Query whether a property name (or synthetic component accessor like `x`/`y`/`radius`)
/// affects scene statistics (geometry or presence).
pub fn property_affects_scene_statistics(property: &str) -> bool {
    if let Some(schema) = lookup_property(property) {
        schema.affects_scene_statistics()
    } else {
        // Fallback for sub-field accessors or syntactic aliases not registered as standalone
        // properties:
        // - `x`, `y`: components of position / motion offset
        // - `radius`: DSL convenience shorthand for ellipse/circle size
        matches!(property, "x" | "y" | "radius")
    }
}

/// Resolve a property name to its stable shared-schema [`animatix_syntax::schema::PropertyId`].
///
/// The id comes from the shared schema declaration order, so analyzer,
/// typechecker, and runtime plans all address the same property namespace.
pub fn property_id(name: &str) -> Option<animatix_syntax::schema::PropertyId> {
    schema_property_ids().get(name).copied()
}

/// Resolve a property name to its schema *and* the runtime plan slot id in one
/// lookup.
///
/// `effective_*` needs both on every property read — at least five per actor
/// per frame — and each used to cost its own lookup (a binary search over the
/// sorted registry, then a hash into the id map). Folding them into one
/// process-wide table removes a repeat lookup from the frame path.
pub fn resolve_property(
    name: &str,
) -> Option<(&'static PropertySchema, Option<animatix_syntax::schema::PropertyId>)> {
    let (index, slot) = *resolved_properties().get(name)?;
    Some((&PROPERTY_REGISTRY[index], slot))
}

/// A registry entry pre-resolved to its schema and plan-slot id.
pub(crate) type ResolvedPropertyRead =
    Option<(&'static PropertySchema, Option<animatix_syntax::schema::PropertyId>)>;

/// Pre-resolved registry entries for the fixed transform-path property set
/// (`rotation`, `scale`, `opacity`, `size`, `transform`).
///
/// `evaluate_node_transform` reads these five per node per frame; resolving
/// them once per process removes a string-hash lookup per property per node
/// while leaving the read itself (`read_property_value_resolved`) and the
/// modifier-override check untouched. `None` (property absent from the
/// registry) is preserved verbatim so behavior matches a failed
/// `resolve_property` exactly.
pub(crate) struct TransformPropertyReads {
    pub rotation: ResolvedPropertyRead,
    pub scale: ResolvedPropertyRead,
    pub opacity: ResolvedPropertyRead,
    pub size: ResolvedPropertyRead,
    pub transform: ResolvedPropertyRead,
}

pub(crate) fn transform_property_reads() -> &'static TransformPropertyReads {
    static CACHE: std::sync::OnceLock<TransformPropertyReads> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| TransformPropertyReads {
        rotation: resolve_property("rotation"),
        scale: resolve_property("scale"),
        opacity: resolve_property("opacity"),
        size: resolve_property("size"),
        transform: resolve_property("transform"),
    })
}
/// The plan-slot id for a property's storage field, or `None` when the field is
/// not a tagged slot the runtime plan can serve.
///
/// Mirrors the guard in `dispatch::read_property_value`: `legend` and
/// `callout_place` are tagged but deliberately excluded from plan reads.
fn plan_slot_id(field: &ActorField) -> Option<animatix_syntax::schema::PropertyId> {
    let ActorField::Tagged(name) = field else {
        return None;
    };
    if *name == "legend" || *name == "callout_place" {
        return None;
    }
    property_id(name)
}

type ResolvedProperty = (usize, Option<animatix_syntax::schema::PropertyId>);

fn resolved_properties() -> &'static std::collections::HashMap<&'static str, ResolvedProperty> {
    static CACHE: std::sync::OnceLock<std::collections::HashMap<&'static str, ResolvedProperty>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        PROPERTY_REGISTRY
            .iter()
            .enumerate()
            .map(|(index, schema)| (schema.name, (index, plan_slot_id(&schema.field))))
            .collect()
    })
}

fn schema_property_ids()
-> &'static std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId> {
    static CACHE: std::sync::OnceLock<
        std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        animatix_syntax::schema::property_specs()
            .into_iter()
            .map(|spec| (spec.name, spec.id))
            .collect()
    })
}

/// Look up a property schema by stable shared-schema [`animatix_syntax::schema::PropertyId`].
pub fn property_schema_by_id(
    id: animatix_syntax::schema::PropertyId,
) -> Option<&'static PropertySchema> {
    property_schema_by_id_cache().get(&id).copied()
}

fn property_schema_by_id_cache()
-> &'static std::collections::HashMap<animatix_syntax::schema::PropertyId, &'static PropertySchema>
{
    static CACHE: std::sync::OnceLock<
        std::collections::HashMap<animatix_syntax::schema::PropertyId, &'static PropertySchema>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for schema in PROPERTY_REGISTRY.iter() {
            if let Some(id) = property_id(schema.name) {
                map.insert(id, schema);
            }
        }
        map
    })
}

/// Look up the canonical name for a stable shared-schema [`animatix_syntax::schema::PropertyId`].
pub fn property_name(id: animatix_syntax::schema::PropertyId) -> Option<&'static str> {
    property_schema_by_id(id).map(|schema| schema.name)
}

// ─────────────────────────────────────────────────────────────
// Per-actor-kind allowed property indices
// ─────────────────────────────────────────────────────────────

/// Convenience: build a sorted set of allowed property indices for an actor.
/// Returns indices into PROPERTY_REGISTRY.
pub fn allowed_property_indices(caps: &crate::timeline::ActorCaps, actor_type: &str) -> Vec<usize> {
    PROPERTY_REGISTRY
        .iter()
        .enumerate()
        .filter(|(_, schema)| schema.applicable.includes(caps, actor_type))
        .map(|(i, _)| i)
        .collect()
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the registry is sorted by name (required for binary search).
    #[test]
    fn registry_is_sorted() {
        for window in PROPERTY_REGISTRY.windows(2) {
            assert!(
                window[0].name <= window[1].name,
                "PROPERTY_REGISTRY is not sorted: '{}' should come before '{}'",
                window[0].name,
                window[1].name
            );
        }
    }

    /// Verify every property can be looked up by name.
    #[test]
    fn every_property_is_lookupable() {
        for schema in PROPERTY_REGISTRY.iter() {
            let found = lookup_property(schema.name);
            assert!(found.is_some(), "Property '{}' cannot be looked up by name", schema.name);
            assert_eq!(found.unwrap().name, schema.name);
        }
    }

    /// Verify that no property name is duplicated.
    #[test]
    fn no_duplicate_names() {
        let mut names: Vec<&str> = PROPERTY_REGISTRY.iter().map(|s| s.name).collect();
        let original_len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), original_len, "Duplicate property names detected");
    }

    #[test]
    fn property_ids_roundtrip_through_registry() {
        for schema in PROPERTY_REGISTRY.iter() {
            let id = property_id(schema.name).expect("runtime property must have a schema id");
            assert_eq!(property_schema_by_id(id).map(|s| s.name), Some(schema.name));
            assert_eq!(property_name(id), Some(schema.name));
        }
    }

    /// Every engine binding must have a shared descriptor: the composed
    /// registry panics otherwise, and a test failure names the offender instead
    /// of surfacing at first render.
    #[test]
    fn every_binding_has_a_descriptor() {
        for binding in BINDINGS {
            assert!(
                animatix_core::property::descriptor(binding.name).is_some(),
                "binding `{}` has no descriptor in animatix_core::property::PROPERTY_DESCRIPTORS",
                binding.name
            );
        }
    }

    /// Descriptors the runtime deliberately does not bind.
    ///
    /// Empty since 2026-09-14: every built-in property now has a binding. The
    /// list stays as the gate — a new descriptor without a binding fails
    /// `unbound_descriptors_are_pinned`, so accepting a property the runtime
    /// cannot store has to be a deliberate, reviewed act rather than an
    /// oversight. The eight names that used to be listed here were either pure
    /// second names for an existing field (`content`, `stroke_color`) or had no
    /// consumer at all (`fill`, `radius`, `start`, `end`, `function`,
    /// `language`); they were removed from the table instead, so the analyzer
    /// rejects them with a clear "unknown property" rather than accepting a
    /// drop that never reaches the screen.
    /// Properties the per-frame override path actually consumes.
    ///
    /// `primitives::apply_text_style_overrides` (and the shape equivalent) read
    /// exactly these keys out of the frame override map an `always` block
    /// writes. Each must therefore be flagged `ANIMATED`, or the engine tells
    /// the author "the per-frame write is ignored" about a write that works —
    /// which is what every entry here used to do, while `text_max_width` was
    /// flagged ANIMATED but unread and so silently did nothing.
    ///
    /// Adding a key here without flagging it ANIMATED, or flagging a property
    /// ANIMATED that no frame path reads, both fail this test.
    #[test]
    fn frame_consumed_properties_are_flagged_animated() {
        const FRAME_READ: &[&str] = &[
            "text",
            "code",
            "font_family",
            "font_size",
            "font_weight",
            "font_style",
            "line_height",
            "letter_spacing",
            "word_spacing",
            "text_max_width",
            "max_width",
            "text_align",
            "text_anchor",
            "overflow",
            "color",
            "stroke",
            "stroke_width",
            "width",
            "fill_opacity",
            "language",
            "at",
            "position",
            "shift",
            "rotation",
            "scale",
            "opacity",
            "size",
            "radius_x",
            "radius_y",
            "transform",
            "corner_radius",
            "from",
            "to",
            "head_size",
        ];
        for name in FRAME_READ {
            let schema = lookup_property(name).unwrap_or_else(|| {
                panic!("`{name}` is read by a frame override path but has no binding")
            });
            assert!(
                schema.flags.contains(PropertyFlags::ANIMATED),
                "`{name}` is consumed by the per-frame override path but is not flagged \
                 ANIMATED, so `always` writes to it are wrongly reported as ignored"
            );
        }
    }

    const UNBOUND_DESCRIPTORS: &[&str] = &[];

    #[test]
    fn unbound_descriptors_are_pinned() {
        let bound: std::collections::HashSet<&str> = BINDINGS.iter().map(|b| b.name).collect();
        let unbound: Vec<&str> = animatix_core::property::PROPERTY_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.name)
            .filter(|name| !bound.contains(name))
            .collect();
        let mut unbound = unbound;
        unbound.sort_unstable();
        let mut expected = UNBOUND_DESCRIPTORS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            unbound, expected,
            "the set of descriptors without an engine binding changed; add the binding \
             (or extend UNBOUND_DESCRIPTORS with a reason)"
        );
    }

    /// `ValueType` is finer than the shared `PropertyValueKind`, so the mapping
    /// is total here (no `_ => Generic` catch-all, which is exactly what let
    /// `solo`'s `Bool`/`Generic` disagreement hide) and must match the
    /// descriptor for every bound property.
    #[test]
    fn value_kinds_agree_with_descriptors() {
        use animatix_core::property::PropertyValueKind as Kind;

        fn kind_of(value_type: ValueType) -> Kind {
            match value_type {
                ValueType::F32 => Kind::F32,
                ValueType::U32 => Kind::U32,
                ValueType::Vec2 => Kind::Vec2,
                ValueType::Vec4 | ValueType::Color => Kind::Vec4,
                ValueType::String => Kind::String,
                ValueType::Bool => Kind::Bool,
                ValueType::PointList => Kind::PointList,
                ValueType::F32List => Kind::Generic,
                ValueType::ShapeType
                | ValueType::PlacementMode
                | ValueType::SceneAnchor
                | ValueType::PositionBinding
                | ValueType::MorphOptions
                | ValueType::CalloutPlace
                | ValueType::CommandList
                | ValueType::BuildTimeOnly
                | ValueType::Gradient
                | ValueType::Enum(_)
                | ValueType::Union(_)
                | ValueType::Sum(_)
                | ValueType::Transform => Kind::Generic,
            }
        }

        for schema in PROPERTY_REGISTRY.iter() {
            let descriptor = animatix_core::property::descriptor(schema.name)
                .expect("binding without descriptor");
            assert_eq!(
                kind_of(schema.value_type),
                descriptor.value_kind,
                "`{}` stores as {:?} in the engine but {:?} in the shared descriptor",
                schema.name,
                schema.value_type,
                descriptor.value_kind
            );
        }
    }

    /// The composed registry must expose the applicability from the descriptor
    /// table (the engine no longer declares it), for every bound property.
    #[test]
    fn applicability_comes_from_the_descriptor_table() {
        for schema in PROPERTY_REGISTRY.iter() {
            let descriptor = animatix_core::property::descriptor(schema.name)
                .expect("binding without descriptor");
            assert_eq!(
                schema.applicable, descriptor.applicable,
                "`{}` applicability disagrees with the descriptor table",
                schema.name
            );
        }
    }

    #[test]
    fn unknown_property_has_no_id() {
        assert_eq!(property_id("definitely_not_a_property"), None);
        assert!(property_schema_by_id(animatix_syntax::schema::PropertyId(u32::MAX)).is_none());
    }

    #[test]
    fn scene_statistics_property_classification() {
        // Spatial and presence properties MUST affect scene statistics
        let spatial_and_presence = [
            "at",
            "position",
            "size",
            "width",
            "height",
            "radius_x",
            "radius_y",
            "from",
            "to",
            "shift",
            "offset",
            "rotation",
            "scale",
            "transform",
            "opacity",
        ];
        for prop in spatial_and_presence {
            assert!(
                property_affects_scene_statistics(prop),
                "Property '{prop}' should affect scene statistics"
            );
            let schema = lookup_property(prop).expect("known property must exist in registry");
            assert!(
                schema.affects_scene_statistics(),
                "Schema for '{prop}' must report affects_scene_statistics() = true"
            );
        }

        // Sub-field accessors and DSL aliases
        assert!(property_affects_scene_statistics("x"));
        assert!(property_affects_scene_statistics("y"));
        assert!(property_affects_scene_statistics("radius"));

        // Styling and non-spatial properties MUST NOT affect scene statistics
        let non_spatial = [
            "color",
            "stroke",
            "stroke_width",
            "stroke_progress",
            "fill_opacity",
            "font_size",
            "char_progress",
            "dash_offset",
            "line_cap",
            "corner_radius",
            "head_size",
        ];
        for prop in non_spatial {
            assert!(
                !property_affects_scene_statistics(prop),
                "Property '{prop}' should NOT affect scene statistics"
            );
            let schema = lookup_property(prop).expect("known property must exist in registry");
            assert!(
                !schema.affects_scene_statistics(),
                "Schema for '{prop}' must report affects_scene_statistics() = false"
            );
        }
    }

    #[test]
    fn affects_vector_paths_classification() {
        let path_affecting = [
            "size",
            "radius_x",
            "radius_y",
            "from",
            "to",
            "points",
            "commands",
            "corner_radius",
            "head_size",
        ];
        for prop in path_affecting {
            let schema = lookup_property(prop).expect("known property must exist in registry");
            assert!(schema.affects_vector_paths(), "Property '{prop}' should affect vector paths");
        }

        let non_path_affecting = ["color", "opacity", "at", "position", "font_size", "shift"];
        for prop in non_path_affecting {
            let schema = lookup_property(prop).expect("known property must exist in registry");
            assert!(
                !schema.affects_vector_paths(),
                "Property '{prop}' should NOT affect vector paths"
            );
        }
    }
}
