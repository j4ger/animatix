//! Shared schema for actor properties and primitives: metadata that would
//! otherwise be duplicated in runtime or analyzer-specific tables. No runtime
//! logic, so analyzer/LSP can consume it without Vello/WGPU.

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
    /// Actor source type names this property applies to; boxed because the
    /// `OnceLock` cache owns the table.
    pub actor_types: Box<[&'static str]>,
    /// Inferred/declared property type.
    pub ty: Type,
    /// Finite value kind used by dynamic property tracks.
    pub value_kind: PropertyValueKind,
}

/// Neutral property descriptor shared by built-ins, extensions, and tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDescriptor {
    /// Stable runtime property id, when registered at runtime; `None` for
    /// manifests and analyzer-only descriptors.
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

/// Child-rendering strategy selected by a primitive; re-exported for tooling.
pub use animatix_core::caps::ChildProcessingKind;
/// Engine capabilities that determine which subsystems consume a primitive.
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

/// One author-visible effect parameter: its name and value kind.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectParamSpecDef {
    /// Parameter name as authored.
    pub name: Box<str>,
    /// Declared value kind.
    pub kind: PropertyValueKind,
}

/// One author-visible effect kind inside a `Filter` scope. Effects are not
/// primitives; this table lets the analyzer validate/complete declarations.
#[derive(Clone, Debug)]
pub struct EffectSpecDef {
    /// Authored type name (`Blur`).
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Declared marshalable parameters; `enabled: Bool` is implicit and not in
    /// the shader uniform layout.
    pub params: Vec<EffectParamSpecDef>,
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

#[cfg(test)]
mod tests;
