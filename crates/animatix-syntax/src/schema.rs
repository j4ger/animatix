//! Shared schema for actor properties and primitives.
//!
//! This module is the first migration target for metadata that previously
//! lived only in runtime or analyzer-specific static tables. It intentionally
//! contains no runtime logic so `animatix-analyzer` and LSP can consume it
//! without depending on Vello/WGPU.

pub use animatix_core::caps::Applicable;
pub use animatix_core::property::{PropertyId, PropertyValueKind};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::typing::{Type, transform_type};

/// One known built-in property with its applicable actor types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertySpec {
    /// Stable id for this property name.
    pub id: PropertyId,
    /// Canonical property name.
    pub name: &'static str,
    /// Actor source type names this property applies to.
    ///
    /// Boxed rather than `&'static [&'static str]`: the `OnceLock` cache owns
    /// the table, so materializing it allocates but never leaks.
    pub actor_types: Box<[&'static str]>,
    /// Inferred/declared property type.
    pub ty: Type,
    /// Finite value kind used by dynamic property tracks.
    pub value_kind: PropertyValueKind,
}

/// Neutral property descriptor shared by built-ins, extensions, and tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDescriptor {
    /// Stable runtime property id, when this descriptor is registered at runtime.
    ///
    /// Manifests and analyzer-only descriptors keep this `None`; runtime
    /// `PropertyRegistry` descriptors always carry `Some`.
    pub id: Option<PropertyId>,
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
    /// Human-readable name for GUI labels, when it differs from `name`.
    pub display_name: Option<String>,
    /// Inspector grouping key.
    pub group: Option<String>,
    /// Help text for tooltips and documentation.
    pub help: Option<String>,
}

impl PropertyDescriptor {
    /// Build a descriptor from a shared schema spec and runtime flags.
    pub fn from_spec(spec: &PropertySpec, injectable: bool) -> Self {
        Self {
            id: Some(spec.id),
            name: spec.name.to_string(),
            actor_types: spec.actor_types.iter().map(|actor| actor.to_string()).collect(),
            ty: spec.ty.clone(),
            value_kind: spec.value_kind,
            injectable,
            display_name: None,
            group: None,
            help: None,
        }
    }
}

/// Description of one action parameter or named modifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionParam {
    /// Parameter name as written in source, e.g. `"to"`.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Expected type information for docs, completion, and validation.
    pub type_info: String,
}

/// Description of a built-in or extension action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionSignature {
    /// Action verb as written in source, e.g. `"fade-in"`.
    pub name: String,
    /// High-level grouping for UI organization, e.g. `"Motion"`.
    pub category: String,
    /// One-line explanation of what the action does.
    pub description: String,
    /// Positional arguments accepted by the action.
    pub params: Vec<ActionParam>,
    /// Named modifiers accepted by the action.
    pub modifiers: Vec<ActionParam>,
}

/// Description of an extension expression function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDescriptor {
    /// Function name as written in source.
    pub name: String,
    /// Positional parameter descriptions.
    pub params: Vec<ActionParam>,
    /// Optional return type for language intelligence.
    pub return_type: Option<Type>,
    /// Optional help text.
    pub help: Option<String>,
}

/// Description of an opaque extension service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDescriptor {
    /// Canonical service name.
    pub name: String,
    /// Optional type information for tooling.
    pub type_info: Option<String>,
    /// Optional help text.
    pub help: Option<String>,
}

/// UI/domain category for a primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimitiveCategory {
    /// Geometric shapes.
    Shape,
    /// Text and typography.
    Text,
    /// Images, SVG, and audio.
    Media,
    /// Plots and charts.
    Plot,
    /// Layout containers.
    Container,
    /// Annotations and callouts.
    Annotation,
}

impl PrimitiveCategory {
    /// Human-readable category label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Shape => "Shapes",
            Self::Text => "Text",
            Self::Media => "Media",
            Self::Plot => "Plots",
            Self::Container => "Containers",
            Self::Annotation => "Annotations",
        }
    }
}

/// Child-rendering strategy selected by a primitive.
///
/// Vocabulary lives in `animatix-core::caps`; re-exported here because the
/// shared schema, manifests, and tooling address it through this crate.
pub use animatix_core::caps::ChildProcessingKind;
/// Engine capabilities that determine which subsystems consume a primitive.
///
/// Vocabulary lives in `animatix-core::caps`; re-exported here because the
/// shared schema, manifests, and tooling address it through this crate.
pub use animatix_core::caps::PrimitiveCapabilities;

/// Metadata for a primitive in the shared schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveSpec {
    /// Source text type name, e.g. `Rect`.
    pub type_name: String,
    /// Display name for GUI palettes.
    pub display_name: String,
    /// UI category.
    pub category: PrimitiveCategory,
    /// Opaque icon id.
    pub icon_id: String,
    /// Whether this primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// Engine capability flags.
    pub capabilities: PrimitiveCapabilities,
    /// Child-rendering strategy used by the scene subtree renderer.
    pub child_processing: ChildProcessingKind,
}

/// Neutral primitive descriptor shared by built-ins, extensions, and tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    /// Source text type name, e.g. `Gauge`.
    pub type_name: String,
    /// Display name for GUI palettes.
    pub display_name: String,
    /// UI category.
    pub category: PrimitiveCategory,
    /// Opaque icon id.
    pub icon_id: String,
    /// Whether this primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// Engine capability flags.
    pub capabilities: PrimitiveCapabilities,
    /// Child-rendering strategy used by the scene subtree renderer.
    pub child_processing: ChildProcessingKind,
    /// Names of properties declared by this primitive.
    pub properties: Vec<String>,
}

/// Built-in primitive metadata shared by runtime, GUI, and LSP tooling.
///
/// Derived from the `animatix-std` catalog — the built-in implementations are
/// the single source; this crate only re-shapes them for tooling.
pub fn builtin_primitive_specs() -> Vec<PrimitiveSpec> {
    use animatix_core::caps::ActorCategory;
    animatix_std::CATALOG
        .iter()
        .map(|info| PrimitiveSpec {
            type_name: info.type_name.to_string(),
            display_name: info.display_name.to_string(),
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

/// One author-visible effect parameter: its name and value kind.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectParamSpecDef {
    /// Parameter name as authored.
    pub name: Box<str>,
    /// Declared value kind.
    pub kind: PropertyValueKind,
}

/// One author-visible effect kind recognized inside a `Filter` scope.
///
/// Effects are not primitives and do not appear in
/// [`builtin_primitive_specs`]; this table exists so the analyzer can validate
/// and complete effect declarations and their parameters. The data is
/// **derived** from the built-in catalog in `animatix-std` — the effect
/// implementations are the single source; this crate only re-shapes them for
/// tooling.
#[derive(Clone, Debug)]
pub struct EffectSpecDef {
    /// Authored type name (`Blur`).
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Declared marshalable parameters. Every effect also implicitly accepts
    /// `enabled: Bool` (default `true`); it is not listed here because it is
    /// not part of the shader's uniform layout.
    pub params: Vec<EffectParamSpecDef>,
}

/// Built-in effect kinds, derived from the `animatix-std` catalog.
///
/// Only built-ins are described here, so the type/display names stay
/// `&'static str` (the `EFFECTS` array is a `static`). The per-parameter rows
/// are owned because they are derived rather than literal.
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

/// All known built-in property specs with stable ids in declaration order.
///
/// The ids are unique per property name and intentionally follow the runtime
/// registry order so `property_id` can round-trip without a second table.
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

/// Property names shared by (nearly) every built-in actor type.
///
/// Extension (plugin) actor types inherit these so the analyzer does not flag
/// the standard transform/geometry/layout properties (`at`, `opacity`,
/// `color`, `size`, ...) as unknown on a custom primitive. `solo` is in the
/// list for the same reason as the transform set: the render gate is
/// actor-type-agnostic, so any primitive can be soloed. Kept as an explicit,
/// reviewable list instead of a count threshold over `actor_types`; the
/// `common_property_list_stays_in_sync` test pins it against the schema so
/// adding a new near-universal property forces a deliberate update here.
pub fn common_property_names() -> &'static [&'static str] {
    &[
        "anchor",
        "at",
        "blend",
        "camera_follow",
        "color",
        "height",
        "legend",
        "max_height",
        "min_height",
        "min_width",
        "offset",
        "opacity",
        "parallax",
        "position",
        "rotation",
        "scale",
        "shift",
        "size",
        "solo",
        "transform",
        "width",
    ]
}

/// The type-system view of every built-in property, index-aligned with
/// [`animatix_core::property::PROPERTY_DESCRIPTORS`].
///
/// The names and the order come from the core descriptor table; this function
/// supplies only the expression type (`Type`), which the runtime- and
/// analyzer-neutral core crate must not carry (unions, function types, and
/// actor/component references belong to the type model). A row added, renamed,
/// or moved here without the matching core change fails
/// `property_types_line_up_with_descriptors` instead of silently mis-typing a
/// property.
fn raw_property_types() -> Vec<(&'static str, Type)> {
    vec![
        // A color may be written as text (`color: "#ff2d55"`, `stroke: "red"`)
        // since batch 2 taught `utils::color_from_text` to every color site, so
        // the five color rows accept `Str` as well. Without this the type
        // checker warns on source the engine renders correctly.
        ("align", Type::Str),
        ("anchor", Type::Any),
        ("ascent", Type::Num),
        ("at", Type::Vec2),
        ("background_color", Type::Union(vec![Type::Color, Type::Str])),
        ("bar_colors", Type::Any),
        // Both spellings of "auto" are read by the BarChart builder
        // (`Expr::Ident` or `Expr::Str`), and the docs say so; a bare `auto`
        // already type-checked because an unresolved identifier is not a Str.
        ("bar_width", Type::Union(vec![Type::Num, Type::Str])),
        ("baseline", Type::Num),
        ("bounds", Type::Vec4),
        ("char_progress", Type::Num),
        ("code", Type::Str),
        ("color", Type::Union(vec![Type::Color, Type::Str])),
        ("cols", Type::Num),
        ("commands", Type::Any),
        ("data", Type::Any),
        ("density", Type::Num),
        ("descent", Type::Num),
        ("direction", Type::Str),
        ("fill_opacity", Type::Num),
        ("font_family", Type::Str),
        ("font_size", Type::Num),
        ("font_style", Type::Str),
        ("font_weight", Type::Union(vec![Type::Num, Type::Str])),
        ("from", Type::Vec2),
        ("func", Type::Any),
        // Two consumers, one row: BarChart reads it as bar spacing and accepts
        // "auto" (`build/plot.rs`), the layout containers read it as a number.
        // Widening this to allow a string is safe only because every container
        // now reports the value it cannot read (`Primitive::...`'s
        // `unreadable_layout_value`) instead of dropping it silently.
        ("gap", Type::Union(vec![Type::Num, Type::Str])),
        ("grid", Type::Str),
        ("head_size", Type::Num),
        ("height", Type::Num),
        ("highlight_color", Type::Union(vec![Type::Color, Type::Str])),
        ("highlight_opacity", Type::Num),
        ("highlight_padding", Type::Num),
        ("highlight_radius", Type::Num),
        ("kind", Type::Str),
        ("label", Type::Str),
        ("label_at", Type::Vec2),
        ("label_color", Type::Union(vec![Type::Color, Type::Str])),
        ("legend", Type::Str),
        ("letter_spacing", Type::Num),
        ("levels", Type::Any),
        ("line_cap", Type::Num),
        ("line_height", Type::Num),
        ("line_join", Type::Num),
        ("max_depth", Type::Num),
        ("max_height", Type::Num),
        ("max_value", Type::Union(vec![Type::Num, Type::Str])),
        ("max_width", Type::Num),
        ("min_height", Type::Num),
        ("min_width", Type::Num),
        ("offset", Type::Vec2),
        ("opacity", Type::Num),
        ("overflow", Type::Str),
        ("padding", Type::Num),
        ("place", Type::Str),
        ("points", Type::List(Box::new(Type::Vec2))),
        ("position", Type::Vec2),
        ("radius_x", Type::Num),
        ("radius_y", Type::Num),
        ("resolution", Type::Num),
        ("rotation", Type::Num),
        ("scale", Type::Num),
        ("shift", Type::Vec2),
        // The BarChart builder reads `Value::Bool` or `Value::Str` ("true"/"1").
        ("show_axis", Type::Union(vec![Type::Bool, Type::Str])),
        ("show_labels", Type::Union(vec![Type::Bool, Type::Str])),
        ("size", Type::Vec2),
        // Authored solo flag: hides every non-solo subtree (recursively).
        ("solo", Type::Bool),
        ("source", Type::Str),
        ("standoff", Type::Num),
        ("stroke", Type::Union(vec![Type::Color, Type::Str])),
        ("stroke_progress", Type::Num),
        ("stroke_width", Type::Num),
        ("swatch_size", Type::Num),
        ("t_domain", Type::Vec2),
        ("target", Type::Any),
        ("text", Type::Str),
        ("text_align", Type::Str),
        ("text_max_width", Type::Num),
        ("tick_labels", Type::Str),
        ("ticks", Type::Str),
        ("title", Type::Str),
        ("to", Type::Vec2),
        ("to_offset", Type::Vec2),
        ("tolerance", Type::Num),
        ("transform", transform_type()),
        ("url", Type::Str),
        ("vertical_align", Type::Str),
        ("volume", Type::Num),
        ("width", Type::Num),
        ("word_spacing", Type::Num),
        ("x_domain", Type::Vec2),
        ("x_range", Type::Any),
        ("x_scale", Type::Str),
        ("y_domain", Type::Vec2),
        ("y_range", Type::Any),
        ("y_scale", Type::Str),
        // Appended with the descriptor row: the two tables are index aligned,
        // and new descriptors go at the end (the row index is `PropertyId`).
        ("corner_radius", Type::Num),
        ("language", Type::Str),
        ("dash_offset", Type::Num),
        ("dash_pattern", Type::List(Box::new(Type::Num))),
        ("blend", Type::Str),
        ("fill_gradient", Type::Any),
        ("stroke_gradient", Type::Any),
        ("gradient_extend", Type::Str),
        ("gradient_space", Type::Str),
        // Build-time icon name for `Path` (expands to `commands`); stores as
        // `Generic` via the `BuildTimeOnly` engine binding, so the declared
        // expression type only guides the analyzer (a string literal).
        ("icon", Type::Str),
        // The scene camera's per-actor opt-out (`hud: Text, camera_follow:
        // false`). Build-time only, stored on a plain track field; the declared
        // type guides the analyzer and the assignment path, which rejects it.
        ("camera_follow", Type::Bool),
        ("parallax", Type::Num),
        ("routing", Type::Str),
        ("arrow", Type::Bool),
        ("split_by", Type::Str),
        ("split_stagger", Type::Num),
        ("split_offset_y", Type::Num),
        ("split_mask", Type::Bool),
        ("value", Type::Num),
        ("prefix", Type::Str),
        ("suffix", Type::Str),
        ("decimals", Type::Num),
        ("comma", Type::Bool),
    ]
}

#[cfg(test)]
mod tests {
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
            118,
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
        assert_eq!(child_processing_of("Rect"), Some(ChildProcessingKind::Generic));
    }
}
