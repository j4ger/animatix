use super::*;

fn property_index(name: &str) -> Option<usize> {
    resolved().schemas.get(name).map(|(index, _)| *index)
}

/// Look up a property schema by name.
///
/// Goes through the process-wide `Resolved` map, so the name → index join is
/// computed once. Returns `None` if no property with that name exists.
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
    resolved().ids.get(name).copied()
}

/// Descriptors for the built-in properties, in shared-schema order.
///
/// Materialized from the shared spec table plus the runtime `INJECTABLE` flag;
/// this is the tooling view, not a second registry. Extensions append their own
/// descriptors through [`crate::extension_context::PropertyRegistry`].
pub fn builtin_descriptors() -> Vec<animatix_syntax::schema::PropertyDescriptor> {
    animatix_syntax::schema::property_specs()
        .into_iter()
        .map(|spec| {
            let injectable = lookup_property(spec.name)
                .is_some_and(|schema| schema.flags.contains(PropertyFlags::INJECTABLE));
            animatix_syntax::schema::PropertyDescriptor::from_spec(&spec, injectable)
        })
        .collect()
}

/// The one `property name → ActorField` table for the string-keyed introspection
/// APIs (keyframe queries, GUI property edits).
///
/// Canonical names resolve through [`PROPERTY_REGISTRY`]; the match below adds
/// only the internal typed-lane aliases that address a storage field directly
/// and are not canonical source properties (`motion_offset` is written as
/// `shift`, `line_from` as `from`, and the cached `vector_paths`/`text_paths`
/// have no source name at all). Three introspection queries (`has_keyframe_at`,
/// `has_keyframes_for`, `list_keyframes`) share this table so they cannot
/// disagree.
pub fn field_for_property(property: &str) -> Option<ActorField> {
    if let Some(schema) = lookup_property(property) {
        return Some(schema.field);
    }
    use ActorField::*;
    Some(match property {
        "motion_offset" => MotionOffset,
        "layout_size" => LayoutSize,
        "shape_type" => ShapeType,
        "line_from" => LineFrom,
        "line_to" => LineTo,
        "arc_angles" => ArcAngles,
        "text_content" => TextContent,
        "placement_mode" => PlacementMode,
        "morph_options" => MorphOptions,
        "stroke_color" => StrokeColor,
        "vector_paths" => VectorPaths,
        "text_paths" => TextPaths,
        _ => return None,
    })
}

/// Resolve a property name to its schema *and* the runtime plan slot id in one
/// lookup.
///
/// `effective_*` needs both on every property read — at least five per actor
/// per frame — and each used to cost its own lookup (a name map probe, then a
/// second hash into the id map). The one `Resolved` table carries both.
pub fn resolve_property(
    name: &str,
) -> Option<(&'static PropertySchema, Option<animatix_syntax::schema::PropertyId>)> {
    let (index, slot) = *resolved().schemas.get(name)?;
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

/// Pre-resolved registry entries, built once per process.
///
/// This replaces the four separate `OnceLock`s the module used to carry
/// (`resolved_properties`, `schema_property_ids`, `property_schema_by_id_cache`,
/// and `transform_property_reads`): they all walked the same two tables, and
/// each new lookup added another process-wide cache to keep coherent.
struct Resolved {
    /// Every shared-schema row: name → stable id (the descriptor row index).
    ids: std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId>,
    /// Every composed registry row: name → registry index + plan slot id.
    schemas: std::collections::HashMap<
        &'static str,
        (usize, Option<animatix_syntax::schema::PropertyId>),
    >,
    /// Stable id → composed schema, for id-keyed lookups.
    by_id: std::collections::HashMap<animatix_syntax::schema::PropertyId, &'static PropertySchema>,
    /// Transform-path reads resolved once, alongside the maps they come from.
    transform: TransformPropertyReads,
}

fn resolved() -> &'static Resolved {
    static CACHE: std::sync::OnceLock<Resolved> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let ids: std::collections::HashMap<_, _> = animatix_syntax::schema::property_specs()
            .into_iter()
            .map(|spec| (spec.name, spec.id))
            .collect();
        let schemas: std::collections::HashMap<_, _> = PROPERTY_REGISTRY
            .iter()
            .enumerate()
            .map(|(index, schema)| (schema.name, (index, plan_slot_id(&schema.field, &ids))))
            .collect();
        let mut by_id = std::collections::HashMap::new();
        for schema in PROPERTY_REGISTRY.iter() {
            if let Some(id) = ids.get(schema.name) {
                by_id.insert(*id, schema);
            }
        }
        let resolve = |name: &str| {
            let (index, slot) = *schemas.get(name)?;
            Some((&PROPERTY_REGISTRY[index], slot))
        };
        let transform = TransformPropertyReads {
            rotation: resolve("rotation"),
            scale: resolve("scale"),
            opacity: resolve("opacity"),
            size: resolve("size"),
            transform: resolve("transform"),
        };
        Resolved {
            ids,
            schemas,
            by_id,
            transform,
        }
    })
}

pub(crate) fn transform_property_reads() -> &'static TransformPropertyReads {
    &resolved().transform
}

/// The plan-slot id for a property's storage field, or `None` when the field is
/// not a tagged slot the runtime plan can serve.
///
/// Mirrors the guard in `dispatch::read_property_value`: `legend` and
/// `callout_place` are tagged but deliberately excluded from plan reads.
fn plan_slot_id(
    field: &ActorField,
    ids: &std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId>,
) -> Option<animatix_syntax::schema::PropertyId> {
    let ActorField::Tagged(name) = field else {
        return None;
    };
    if *name == "legend" || *name == "callout_place" {
        return None;
    }
    ids.get(name).copied()
}

/// Look up a property schema by stable shared-schema [`animatix_syntax::schema::PropertyId`].
pub fn property_schema_by_id(
    id: animatix_syntax::schema::PropertyId,
) -> Option<&'static PropertySchema> {
    resolved().by_id.get(&id).copied()
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

    /// Verify the registry is name-sorted.
    ///
    /// Lookups go through the name map, but the sorted order keeps the table
    /// reviewable (and `allowed_property_indices` returns registry order), so a
    /// new row still has an obvious place.
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

    /// The canonical `name → field` mapping is derived from the registry; only
    /// typed-lane aliases (names with no source property) are spelled out. This
    /// pins both halves so a new binding cannot silently fall out of the
    /// keyframe-introspection table.
    #[test]
    fn field_for_property_agrees_with_the_registry() {
        for schema in PROPERTY_REGISTRY.iter() {
            assert_eq!(
                field_for_property(schema.name),
                Some(schema.field),
                "`{}` must resolve to its registered field",
                schema.name
            );
        }

        for (alias, field) in [
            ("motion_offset", ActorField::MotionOffset),
            ("layout_size", ActorField::LayoutSize),
            ("shape_type", ActorField::ShapeType),
            ("line_from", ActorField::LineFrom),
            ("line_to", ActorField::LineTo),
            ("arc_angles", ActorField::ArcAngles),
            ("text_content", ActorField::TextContent),
            ("placement_mode", ActorField::PlacementMode),
            ("morph_options", ActorField::MorphOptions),
            ("stroke_color", ActorField::StrokeColor),
            ("vector_paths", ActorField::VectorPaths),
            ("text_paths", ActorField::TextPaths),
        ] {
            assert_eq!(field_for_property(alias), Some(field), "`{alias}` alias");
        }

        assert_eq!(field_for_property("not_a_property"), None);
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
