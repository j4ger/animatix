//! Shared schema for actor properties and primitives.
//!
//! This module is the first migration target for metadata that previously
//! lived only in runtime or analyzer-specific static tables. It intentionally
//! contains no runtime logic so `animatix-analyzer` and LSP can consume it
//! without depending on Vello/WGPU.

pub use animatix_core::caps::Applicable;
pub use animatix_core::property::{PropertyId, PropertyValueKind};

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
    /// Author-facing documentation for completions and hover info.
    pub doc: String,
    /// Default syntax stub when inserting or creating this primitive.
    pub default_stub: Option<String>,
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
/// reviewable list rather than a count threshold; `common_property_list_stays_in_sync`
/// pins it against the schema so adding a near-universal property is deliberate.
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
fn property_type_for_descriptor(desc: &animatix_core::property::PropertyDescriptor) -> Type {
    match desc.name {
        "align" | "code" | "direction" | "font_family" | "font_style" | "grid" | "kind"
        | "label" | "overflow" | "place" | "source" | "text" | "text_align" | "tick_labels"
        | "ticks" | "title" | "url" | "vertical_align" | "x_scale" | "y_scale" | "language"
        | "blend" | "gradient_extend" | "gradient_space" | "icon" | "routing" | "split_by"
        | "prefix" | "suffix" | "idle" | "shared_id" | "caption_mode" | "transcript"
        | "text_anchor" => Type::Str,

        "background_color" | "color" | "highlight_color" | "label_color" | "stroke" => {
            Type::Union(vec![Type::Color, Type::Str])
        },
        "active_color" | "inactive_color" | "backdrop_color" | "shadow_color" => Type::Color,

        "bar_width" | "font_weight" | "gap" | "max_value" => {
            Type::Union(vec![Type::Num, Type::Str])
        },
        // `legend` is a sum property, not a plain string: the engine's
        // `LEGEND_SUM_VARIANTS` reads `true`/`false` as the auto/hidden
        // variants and a string as the explicit label.
        "show_axis" | "show_labels" | "legend" => Type::Union(vec![Type::Bool, Type::Str]),

        "points" => Type::List(Box::new(Type::Vec2)),
        "dash_pattern" => Type::List(Box::new(Type::Num)),
        "transform" => transform_type(),
        "bounds" | "shadow" => Type::Vec4,
        "anchor" | "bar_colors" | "commands" | "data" | "func" | "levels" | "target"
        | "x_range" | "y_range" | "fill_gradient" | "stroke_gradient" => Type::Any,

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

#[cfg(test)]
mod tests;
