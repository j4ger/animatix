//! Shared schema for actor properties and primitives.
//!
//! This module is the first migration target for metadata that previously
//! lived only in runtime or analyzer-specific static tables. It intentionally
//! contains no runtime logic so `animatix-analyzer` and LSP can consume it
//! without depending on Vello/WGPU.

use crate::typing::{Type, transform_type};

pub use animatix_core::caps::Applicable;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Stable property identifier used by runtime plans and schema consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PropertyId(pub u32);

/// Finite value kind understood by extension property tracks.
///
/// This mirrors the runtime `DynTrack` storage while staying free of
/// renderer/runtime dependencies so analyzer and LSP can consume it too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyValueKind {
    /// 32-bit float.
    F32,
    /// 32-bit unsigned integer.
    U32,
    /// Boolean flag.
    Bool,
    /// 2D vector.
    Vec2,
    /// 4D vector or color.
    Vec4,
    /// String.
    String,
    /// List of 2D points.
    PointList,
    /// Any finite property value.
    Generic,
}

/// One known built-in property with its applicable actor types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertySpec {
    /// Stable id for this property name.
    pub id: PropertyId,
    /// Canonical property name.
    pub name: &'static str,
    /// Actor source type names this property applies to.
    pub actor_types: &'static [&'static str],
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

/// Engine capabilities that determine which subsystems consume a primitive.
///
/// Vocabulary lives in `animatix-core::caps`; re-exported here because the
/// shared schema, manifests, and tooling address it through this crate.
pub use animatix_core::caps::PrimitiveCapabilities;

/// Child-rendering strategy selected by a primitive.
///
/// Vocabulary lives in `animatix-core::caps`; re-exported here because the
/// shared schema, manifests, and tooling address it through this crate.
pub use animatix_core::caps::ChildProcessingKind;

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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectParamSpecDef {
    /// Parameter name as authored.
    pub name: &'static str,
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
#[derive(Clone, Copy, Debug)]
pub struct EffectSpecDef {
    /// Authored type name (`Blur`).
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Declared marshalable parameters. Every effect also implicitly accepts
    /// `enabled: Bool` (default `true`); it is not listed here because it is
    /// not part of the shader's uniform layout.
    pub params: &'static [EffectParamSpecDef],
}

/// Built-in effect kinds, derived from the `animatix-std` catalog.
pub fn effect_specs() -> &'static [EffectSpecDef] {
    use std::sync::OnceLock;
    static SPECS: OnceLock<Vec<EffectSpecDef>> = OnceLock::new();
    SPECS.get_or_init(|| {
        animatix_std::EFFECTS
            .iter()
            .map(|effect| {
                let params: Vec<EffectParamSpecDef> = effect
                    .params()
                    .iter()
                    .map(|spec| EffectParamSpecDef {
                        name: spec.name,
                        kind: shared_kind(spec.kind),
                    })
                    .collect();
                EffectSpecDef {
                    type_name: effect.type_name(),
                    display_name: effect.display_name(),
                    params: Box::leak(params.into_boxed_slice()),
                }
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
                    .map(|info| (info.type_name, animatix_std::caps_from_info(info)))
                    .collect();
            raw_property_specs()
                .into_iter()
                .enumerate()
                .map(|(index, (name, applicable, ty, value_kind))| {
                    let types: Vec<&'static str> = catalog
                        .iter()
                        .filter(|(type_name, caps)| applicable.includes(caps, type_name))
                        .map(|(type_name, _)| *type_name)
                        .collect();
                    PropertySpec {
                        id: PropertyId(index as u32),
                        name,
                        actor_types: Box::leak(types.into_boxed_slice()),
                        ty,
                        value_kind,
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
/// `color`, `size`, ...) as unknown on a custom primitive. Kept as an explicit,
/// reviewable list instead of a count threshold over `actor_types`; the
/// `common_property_list_stays_in_sync` test pins it against the schema so
/// adding a new near-universal property forces a deliberate update here.
pub fn common_property_names() -> &'static [&'static str] {
    &[
        "anchor",
        "at",
        "color",
        "height",
        "legend",
        "max_height",
        "min_height",
        "min_width",
        "offset",
        "opacity",
        "position",
        "rotation",
        "scale",
        "shift",
        "size",
        "transform",
        "width",
    ]
}

fn raw_property_specs() -> Vec<(&'static str, Applicable, Type, PropertyValueKind)> {
    vec![
        (
            "align",
            Applicable::Actors(&["Col", "Grid", "Row", "Stack"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        ("anchor", Applicable::Everything, Type::Any, PropertyValueKind::Generic),
        ("ascent", Applicable::Never, Type::Num, PropertyValueKind::F32),
        ("at", Applicable::Everything, Type::Vec2, PropertyValueKind::Vec2),
        ("background_color", Applicable::Never, Type::Color, PropertyValueKind::Vec4),
        (
            "bar_colors",
            Applicable::Actors(&["BarChart"]),
            Type::Any,
            PropertyValueKind::Generic,
        ),
        (
            "bar_width",
            Applicable::Actors(&["BarChart"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("baseline", Applicable::Never, Type::Num, PropertyValueKind::F32),
        ("bounds", Applicable::Actors(&["Filter"]), Type::Vec4, PropertyValueKind::Vec4),
        ("char_progress", Applicable::TextLike, Type::Num, PropertyValueKind::F32),
        ("code", Applicable::Actors(&["Code"]), Type::Str, PropertyValueKind::String),
        ("color", Applicable::Everything, Type::Color, PropertyValueKind::Vec4),
        ("cols", Applicable::Actors(&["Grid"]), Type::Num, PropertyValueKind::U32),
        ("commands", Applicable::Actors(&["Path"]), Type::Any, PropertyValueKind::Generic),
        ("data", Applicable::Actors(&["BarChart"]), Type::Any, PropertyValueKind::Generic),
        (
            "density",
            Applicable::Actors(&["VectorField"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("descent", Applicable::Never, Type::Num, PropertyValueKind::F32),
        (
            "direction",
            Applicable::Actors(&["BarChart"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        (
            "fill_opacity",
            Applicable::AllShapesExceptLine,
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("font_family", Applicable::TextLike, Type::Str, PropertyValueKind::String),
        (
            "font_size",
            Applicable::Actors(&["Code", "Equation", "Math", "Text", "Typst"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("font_style", Applicable::TextLike, Type::Str, PropertyValueKind::String),
        (
            "font_weight",
            Applicable::TextLike,
            Type::Union(vec![Type::Num, Type::Str]),
            PropertyValueKind::F32,
        ),
        (
            "from",
            Applicable::Actors(&["Arrow", "Callout", "Line"]),
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "func",
            Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve", "VectorField"]),
            Type::Any,
            PropertyValueKind::Generic,
        ),
        (
            "gap",
            Applicable::Actors(&["Col", "Grid", "Group", "Legend", "Mask", "Row", "Stack"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("grid", Applicable::Actors(&["Graph"]), Type::Str, PropertyValueKind::String),
        (
            "head_size",
            Applicable::Actors(&["Arrow", "Callout"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("height", Applicable::SizedActors, Type::Num, PropertyValueKind::F32),
        (
            "highlight_color",
            Applicable::Actors(&["Equation", "Fragment"]),
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        (
            "highlight_opacity",
            Applicable::Actors(&["Equation", "Fragment"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "highlight_padding",
            Applicable::Actors(&["Equation", "Fragment"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "highlight_radius",
            Applicable::Actors(&["Equation", "Fragment"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("kind", Applicable::Actors(&["PlotCurve"]), Type::Str, PropertyValueKind::String),
        ("label", Applicable::Actors(&["Callout"]), Type::Str, PropertyValueKind::String),
        (
            "label_at",
            Applicable::Actors(&["Callout"]),
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "label_color",
            Applicable::Actors(&["Legend"]),
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        ("latex", Applicable::Never, Type::Str, PropertyValueKind::String),
        ("legend", Applicable::Everything, Type::Str, PropertyValueKind::Generic),
        (
            "letter_spacing",
            Applicable::Actors(&[
                "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
            ]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "levels",
            Applicable::Actors(&["ContourSet"]),
            Type::Any,
            PropertyValueKind::Vec2,
        ),
        ("line_cap", Applicable::AllShapes, Type::Num, PropertyValueKind::U32),
        ("line_height", Applicable::TextLike, Type::Num, PropertyValueKind::F32),
        ("line_join", Applicable::AllShapes, Type::Num, PropertyValueKind::U32),
        ("math", Applicable::Actors(&["Typst"]), Type::Str, PropertyValueKind::String),
        (
            "max_depth",
            Applicable::Actors(&["ContourSet", "PlotCurve"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("max_height", Applicable::SizedActors, Type::Num, PropertyValueKind::F32),
        (
            "max_value",
            Applicable::Actors(&["BarChart"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "max_width",
            Applicable::Actors(&[
                "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
            ]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("min_height", Applicable::SizedActors, Type::Num, PropertyValueKind::F32),
        ("min_width", Applicable::SizedActors, Type::Num, PropertyValueKind::F32),
        ("offset", Applicable::Everything, Type::Vec2, PropertyValueKind::Vec2),
        ("opacity", Applicable::Everything, Type::Num, PropertyValueKind::F32),
        ("overflow", Applicable::TextLike, Type::Str, PropertyValueKind::String),
        (
            "padding",
            Applicable::Actors(&["Col", "Graph", "Grid", "Row", "Stack"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("place", Applicable::Actors(&["Callout"]), Type::Str, PropertyValueKind::Generic),
        (
            "points",
            Applicable::Actors(&["Polygon"]),
            Type::List(Box::new(Type::Vec2)),
            PropertyValueKind::PointList,
        ),
        ("position", Applicable::Everything, Type::Vec2, PropertyValueKind::Vec2),
        ("radius_x", Applicable::Actors(&["Ellipse"]), Type::Num, PropertyValueKind::F32),
        ("radius_y", Applicable::Actors(&["Ellipse"]), Type::Num, PropertyValueKind::F32),
        (
            "resolution",
            Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("rotation", Applicable::Everything, Type::Num, PropertyValueKind::F32),
        ("scale", Applicable::Everything, Type::Num, PropertyValueKind::F32),
        ("shift", Applicable::Everything, Type::Vec2, PropertyValueKind::Vec2),
        (
            "show_axis",
            Applicable::Actors(&["BarChart"]),
            Type::Bool,
            PropertyValueKind::Generic,
        ),
        (
            "show_labels",
            Applicable::Actors(&["BarChart"]),
            Type::Bool,
            PropertyValueKind::Generic,
        ),
        ("size", Applicable::ExceptTextLike, Type::Vec2, PropertyValueKind::Vec2),
        ("source", Applicable::Actors(&["Audio"]), Type::Str, PropertyValueKind::String),
        ("standoff", Applicable::Actors(&["Callout"]), Type::Num, PropertyValueKind::F32),
        ("stroke", Applicable::AllStrokePaths, Type::Color, PropertyValueKind::Vec4),
        ("stroke_progress", Applicable::AllStrokePaths, Type::Num, PropertyValueKind::F32),
        (
            "stroke_width",
            Applicable::Actors(&[
                "Arrow",
                "ContourSet",
                "Ellipse",
                "Graph",
                "Heatmap",
                "Line",
                "NumberPlane",
                "Path",
                "PlotCurve",
                "Polygon",
                "Rect",
                "VectorField",
            ]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "swatch_size",
            Applicable::Actors(&["Legend"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "t_domain",
            Applicable::Actors(&["PlotCurve"]),
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "target",
            Applicable::Actors(&["Callout"]),
            Type::Any,
            PropertyValueKind::Generic,
        ),
        (
            "text",
            Applicable::Actors(&["Math", "Text"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        ("text_align", Applicable::TextLike, Type::Str, PropertyValueKind::String),
        (
            "text_max_width",
            Applicable::Actors(&["Legend"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "tick_labels",
            Applicable::Actors(&["Graph"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        ("ticks", Applicable::Actors(&["Graph"]), Type::Str, PropertyValueKind::String),
        ("title", Applicable::Actors(&["Legend"]), Type::Str, PropertyValueKind::String),
        (
            "to",
            Applicable::Actors(&["Arrow", "Callout", "Line"]),
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "to_offset",
            Applicable::Actors(&["Callout"]),
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "tolerance",
            Applicable::Actors(&["PlotCurve"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "transform",
            Applicable::Everything,
            transform_type(),
            PropertyValueKind::Generic,
        ),
        (
            "url",
            Applicable::Actors(&["Image", "Svg"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        (
            "vertical_align",
            Applicable::Actors(&["Col", "Row"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        ("volume", Applicable::Actors(&["Audio"]), Type::Num, PropertyValueKind::F32),
        ("width", Applicable::SizedActors, Type::Num, PropertyValueKind::F32),
        ("word_spacing", Applicable::TextLike, Type::Num, PropertyValueKind::F32),
        ("x_domain", Applicable::PlotGeometry, Type::Vec2, PropertyValueKind::Vec2),
        (
            "x_range",
            Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]),
            Type::Any,
            PropertyValueKind::Vec2,
        ),
        ("x_scale", Applicable::Actors(&["Graph"]), Type::Str, PropertyValueKind::String),
        ("y_domain", Applicable::PlotGeometry, Type::Vec2, PropertyValueKind::Vec2),
        (
            "y_range",
            Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]),
            Type::Any,
            PropertyValueKind::Vec2,
        ),
        ("y_scale", Applicable::Actors(&["Graph"]), Type::Str, PropertyValueKind::String),
        ("content", Applicable::TextLike, Type::Str, PropertyValueKind::String),
        ("language", Applicable::Actors(&["Code"]), Type::Str, PropertyValueKind::String),
        ("fill", Applicable::AllShapesExceptLine, Type::Color, PropertyValueKind::Vec4),
        (
            "radius",
            Applicable::Actors(&["Ellipse", "Polygon", "Rect"]),
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("start", Applicable::Actors(&["Line"]), Type::Vec2, PropertyValueKind::Vec2),
        ("end", Applicable::Actors(&["Line"]), Type::Vec2, PropertyValueKind::Vec2),
        (
            "function",
            Applicable::Actors(&["Graph", "PlotCurve"]),
            Type::Str,
            PropertyValueKind::String,
        ),
        ("stroke_color", Applicable::AllStrokePaths, Type::Color, PropertyValueKind::Vec4),
    ]
}
#[cfg(test)]
mod tests {
    use super::{
        ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveSpec,
        builtin_primitive_specs, common_property_names, property_specs,
    };
    use crate::typing::Type;

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
        for name in [
            "text",
            "content",
            "font_size",
            "font_family",
            "line_height",
            "color",
        ] {
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
                morphable_paths: true,
                vector_reveal_target: true,
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
