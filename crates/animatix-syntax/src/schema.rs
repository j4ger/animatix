//! Shared schema for actor properties and primitives.
//!
//! This module is the first migration target for metadata that previously
//! lived only in runtime or analyzer-specific static tables. It intentionally
//! contains no runtime logic so `animatix-analyzer` and LSP can consume it
//! without depending on Vello/WGPU.

use crate::icon_glyphs::{
    ARROW_RIGHT, ARROWS_OUT_CARDINAL, ARTICLE, CHART_BAR, CHART_DONUT, CHART_LINE_UP, CIRCLE_NOTCH,
    CODE, COLUMNS, FILTERS, FOLDER, GRADIENT, HIGHLIGHTER, IMAGE, MASK_HAPPY, MINUS, PEN, POLYGON,
    ROWS, SIGMA, SPEAKER_HIGH, SQUARE, SQUARES_FOUR, STACK, TEXT_T, VECTOR_THREE,
};
use crate::typing::{Type, transform_type};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrimitiveCapabilities {
    /// Emits text glyph paths.
    pub text_paths: bool,
    /// Emits vector paths.
    pub vector_paths: bool,
    /// Carries raster image payload.
    pub image_payload: bool,
    /// Participates in layout containers.
    pub layout_container: bool,
    /// Supports path morphing.
    pub morphable_paths: bool,
    /// Supports vector reveal actions.
    pub vector_reveal_target: bool,
    /// Emits plot geometry.
    pub plot_geometry: bool,
    /// Hosts plot-curve children in a math coordinate system.
    pub plot_host: bool,
    /// Is a container primitive.
    pub is_container: bool,
    /// Is a vector shape.
    pub is_shape: bool,
}

/// Child-rendering strategy selected by a primitive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChildProcessingKind {
    /// Render children through the normal scene graph recursion.
    #[default]
    Generic,
    /// Render children through the offscreen filter pipeline.
    Filter,
    /// Render children inside a clip mask.
    Mask,
    /// Render children as one aggregated equation document.
    Equation,
}

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
pub fn builtin_primitive_specs() -> Vec<PrimitiveSpec> {
    let entries: &[(&'static str, &'static str, &'static str, PrimitiveCategory, bool)] = &[
        ("Rect", "Rectangle", SQUARE, PrimitiveCategory::Shape, false),
        ("Ellipse", "Ellipse", CIRCLE_NOTCH, PrimitiveCategory::Shape, false),
        ("Line", "Line", MINUS, PrimitiveCategory::Shape, false),
        ("Arrow", "Arrow", ARROW_RIGHT, PrimitiveCategory::Shape, false),
        ("Polygon", "Polygon", POLYGON, PrimitiveCategory::Shape, false),
        ("Path", "Path", PEN, PrimitiveCategory::Shape, false),
        ("Text", "Text", TEXT_T, PrimitiveCategory::Text, false),
        ("Code", "Code", CODE, PrimitiveCategory::Text, true),
        ("Typst", "Typst", ARTICLE, PrimitiveCategory::Text, true),
        ("Image", "Image", IMAGE, PrimitiveCategory::Media, false),
        ("Svg", "SVG", VECTOR_THREE, PrimitiveCategory::Media, true),
        ("Audio", "Audio", SPEAKER_HIGH, PrimitiveCategory::Media, true),
        ("Graph", "Graph", CHART_BAR, PrimitiveCategory::Plot, false),
        ("PlotCurve", "Plot Curve", CHART_LINE_UP, PrimitiveCategory::Plot, true),
        (
            "VectorField",
            "Vector Field",
            ARROWS_OUT_CARDINAL,
            PrimitiveCategory::Plot,
            true,
        ),
        ("Heatmap", "Heatmap", GRADIENT, PrimitiveCategory::Plot, true),
        ("ContourSet", "Contour Set", CHART_DONUT, PrimitiveCategory::Plot, true),
        ("NumberPlane", "Number Plane", SQUARES_FOUR, PrimitiveCategory::Plot, false),
        ("BarChart", "Bar Chart", CHART_BAR, PrimitiveCategory::Plot, false),
        ("Row", "Row", ROWS, PrimitiveCategory::Container, false),
        ("Col", "Column", COLUMNS, PrimitiveCategory::Container, false),
        ("Grid", "Grid", SQUARES_FOUR, PrimitiveCategory::Container, false),
        ("Stack", "Stack", STACK, PrimitiveCategory::Container, false),
        ("Group", "Group", FOLDER, PrimitiveCategory::Container, false),
        ("Mask", "Mask", MASK_HAPPY, PrimitiveCategory::Container, true),
        ("Filter", "Filter", FILTERS, PrimitiveCategory::Container, false),
        ("Equation", "Equation", SIGMA, PrimitiveCategory::Container, false),
        ("Fragment", "Fragment", HIGHLIGHTER, PrimitiveCategory::Text, false),
        ("Callout", "Callout", TEXT_T, PrimitiveCategory::Annotation, false),
        ("Legend", "Legend", CHART_LINE_UP, PrimitiveCategory::Annotation, false),
    ];
    entries
        .iter()
        .map(|(type_name, display_name, icon_id, category, advanced)| PrimitiveSpec {
            type_name: (*type_name).to_string(),
            display_name: (*display_name).to_string(),
            category: *category,
            icon_id: (*icon_id).to_string(),
            advanced: *advanced,
            capabilities: schema_capabilities(type_name, *category),
            child_processing: schema_child_processing(type_name),
        })
        .collect()
}

fn schema_capabilities(type_name: &str, category: PrimitiveCategory) -> PrimitiveCapabilities {
    match category {
        PrimitiveCategory::Shape => PrimitiveCapabilities {
            vector_paths: true,
            morphable_paths: true,
            vector_reveal_target: true,
            is_shape: true,
            ..PrimitiveCapabilities::default()
        },
        PrimitiveCategory::Text => PrimitiveCapabilities {
            text_paths: true,
            morphable_paths: true,
            vector_reveal_target: true,
            ..PrimitiveCapabilities::default()
        },
        PrimitiveCategory::Media => match type_name {
            "Svg" => PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                ..PrimitiveCapabilities::default()
            },
            "Image" => PrimitiveCapabilities {
                image_payload: true,
                ..PrimitiveCapabilities::default()
            },
            _ => PrimitiveCapabilities::default(),
        },
        PrimitiveCategory::Plot => PrimitiveCapabilities {
            vector_paths: true,
            morphable_paths: true,
            vector_reveal_target: true,
            plot_geometry: true,
            plot_host: type_name == "Graph",
            ..PrimitiveCapabilities::default()
        },
        PrimitiveCategory::Container => match type_name {
            "Group" | "Mask" => PrimitiveCapabilities {
                is_container: true,
                ..PrimitiveCapabilities::default()
            },
            _ => PrimitiveCapabilities {
                layout_container: true,
                is_container: true,
                ..PrimitiveCapabilities::default()
            },
        },
        PrimitiveCategory::Annotation => PrimitiveCapabilities {
            vector_paths: true,
            ..PrimitiveCapabilities::default()
        },
    }
}

/// Child-processing strategy for a built-in primitive name.
pub fn schema_child_processing(type_name: &str) -> ChildProcessingKind {
    match type_name {
        "Filter" => ChildProcessingKind::Filter,
        "Mask" => ChildProcessingKind::Mask,
        "Equation" => ChildProcessingKind::Equation,
        _ => ChildProcessingKind::Generic,
    }
}

/// All known built-in property specs with stable ids in declaration order.
///
/// The ids are unique per property name and intentionally follow the runtime
/// registry order so `property_id` can round-trip without a second table.
pub fn property_specs() -> Vec<PropertySpec> {
    raw_property_specs()
        .into_iter()
        .enumerate()
        .map(|(index, (name, actor_types, ty, value_kind))| PropertySpec {
            id: PropertyId(index as u32),
            name,
            actor_types,
            ty,
            value_kind,
        })
        .collect()
}

fn raw_property_specs() -> Vec<(&'static str, &'static [&'static str], Type, PropertyValueKind)> {
    let mut specs: Vec<(&'static str, &'static [&'static str], Type, PropertyValueKind)> = vec![
        ("align", &["Row", "Col", "Grid", "Stack"], Type::Str, PropertyValueKind::String),
        (
            "anchor",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Str,
            PropertyValueKind::Generic,
        ),
        ("ascent", &[], Type::Num, PropertyValueKind::F32),
        (
            "at",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("background_color", &[], Type::Color, PropertyValueKind::Vec4),
        ("bar_colors", &["BarChart"], Type::Any, PropertyValueKind::Generic),
        ("bar_width", &["BarChart"], Type::Num, PropertyValueKind::F32),
        ("baseline", &[], Type::Num, PropertyValueKind::F32),
        ("blur", &["Filter"], Type::Num, PropertyValueKind::F32),
        ("brightness", &["Filter"], Type::Num, PropertyValueKind::F32),
        ("char_progress", &["Text", "Code", "Typst"], Type::Num, PropertyValueKind::F32),
        ("code", &["Code"], Type::Str, PropertyValueKind::String),
        (
            "color",
            &[
                "Rect", "Ellipse", "Line", "Arrow", "Polygon", "Path", "Text", "Typst", "Code",
                "BarChart",
            ],
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        ("cols", &["Grid"], Type::Num, PropertyValueKind::U32),
        ("commands", &["Path"], Type::Str, PropertyValueKind::Generic),
        ("contrast", &["Filter"], Type::Num, PropertyValueKind::F32),
        ("data", &["BarChart"], Type::Str, PropertyValueKind::String),
        ("density", &["VectorField"], Type::Num, PropertyValueKind::F32),
        ("descent", &[], Type::Num, PropertyValueKind::F32),
        ("direction", &["BarChart"], Type::Str, PropertyValueKind::String),
        (
            "fill_opacity",
            &["Rect", "Ellipse", "Arrow", "Polygon", "Path"],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("font_family", &["Text", "Typst", "Code"], Type::Str, PropertyValueKind::String),
        ("font_size", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        ("font_style", &["Text", "Typst", "Code"], Type::Str, PropertyValueKind::String),
        ("font_weight", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        ("from", &["Line", "Arrow", "Callout"], Type::Vec2, PropertyValueKind::Vec2),
        (
            "func",
            &["PlotCurve", "VectorField", "Heatmap", "ContourSet"],
            Type::Any,
            PropertyValueKind::Generic,
        ),
        ("gap", &["Row", "Col", "Grid"], Type::Num, PropertyValueKind::F32),
        ("grid", &["Graph"], Type::Str, PropertyValueKind::String),
        ("head_size", &["Arrow", "Callout"], Type::Num, PropertyValueKind::F32),
        (
            "height",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "highlight_color",
            &["Equation", "Fragment"],
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        (
            "highlight_opacity",
            &["Equation", "Fragment"],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "highlight_padding",
            &["Equation", "Fragment"],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("highlight_radius", &["Equation", "Fragment"], Type::Num, PropertyValueKind::F32),
        ("hue_rotate", &["Filter"], Type::Num, PropertyValueKind::F32),
        ("kind", &["PlotCurve"], Type::Str, PropertyValueKind::String),
        ("label", &["Callout"], Type::Str, PropertyValueKind::String),
        ("label_at", &["Callout"], Type::Vec2, PropertyValueKind::Vec2),
        ("label_color", &["Legend"], Type::Color, PropertyValueKind::Vec4),
        ("latex", &[], Type::Str, PropertyValueKind::String),
        (
            "legend",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Str,
            PropertyValueKind::Generic,
        ),
        ("letter_spacing", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        ("levels", &["ContourSet"], Type::Vec2, PropertyValueKind::Vec2),
        (
            "line_cap",
            &["Rect", "Ellipse", "Line", "Arrow", "Polygon", "Path"],
            Type::Num,
            PropertyValueKind::U32,
        ),
        ("line_height", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        (
            "line_join",
            &["Rect", "Ellipse", "Line", "Arrow", "Polygon", "Path"],
            Type::Num,
            PropertyValueKind::U32,
        ),
        ("math", &["Typst"], Type::Str, PropertyValueKind::String),
        ("max_depth", &["PlotCurve", "ContourSet"], Type::Num, PropertyValueKind::F32),
        (
            "max_height",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("max_value", &["BarChart"], Type::Num, PropertyValueKind::F32),
        ("max_width", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        (
            "min_height",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "min_width",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "offset",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "opacity",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("overflow", &["Text", "Typst", "Code"], Type::Str, PropertyValueKind::String),
        (
            "padding",
            &["Graph", "Row", "Col", "Grid", "Stack"],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("place", &["Callout"], Type::Str, PropertyValueKind::Generic),
        (
            "points",
            &["Polygon"],
            Type::List(Box::new(Type::Vec2)),
            PropertyValueKind::PointList,
        ),
        (
            "position",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("radius_x", &["Ellipse"], Type::Num, PropertyValueKind::F32),
        ("radius_y", &["Ellipse"], Type::Num, PropertyValueKind::F32),
        (
            "resolution",
            &["PlotCurve", "Heatmap", "ContourSet"],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "rotation",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("saturate", &["Filter"], Type::Num, PropertyValueKind::F32),
        (
            "scale",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("sepia", &["Filter"], Type::Num, PropertyValueKind::F32),
        (
            "shift",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("show_axis", &["BarChart"], Type::Str, PropertyValueKind::String),
        ("show_labels", &["BarChart"], Type::Str, PropertyValueKind::String),
        (
            "size",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("source", &["Audio"], Type::Str, PropertyValueKind::String),
        ("standoff", &["Callout"], Type::Num, PropertyValueKind::F32),
        (
            "stroke",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "PlotCurve",
            ],
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        (
            "stroke_progress",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "PlotCurve",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        (
            "stroke_width",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "PlotCurve",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("swatch_size", &["Legend"], Type::Num, PropertyValueKind::F32),
        ("t_domain", &["PlotCurve"], Type::Vec2, PropertyValueKind::Vec2),
        ("target", &["Callout"], Type::Str, PropertyValueKind::String),
        ("text", &["Text"], Type::Str, PropertyValueKind::String),
        ("text_align", &["Text", "Typst", "Code"], Type::Str, PropertyValueKind::String),
        ("text_max_width", &["Legend"], Type::Num, PropertyValueKind::F32),
        ("tick_labels", &["Graph"], Type::Str, PropertyValueKind::String),
        ("ticks", &["Graph"], Type::Str, PropertyValueKind::String),
        ("title", &["Legend"], Type::Str, PropertyValueKind::String),
        ("to", &["Line", "Arrow", "Callout"], Type::Vec2, PropertyValueKind::Vec2),
        ("to_offset", &["Callout"], Type::Vec2, PropertyValueKind::Vec2),
        ("tolerance", &["PlotCurve"], Type::Num, PropertyValueKind::F32),
        (
            "transform",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Text",
                "Code",
                "Typst",
                "Image",
                "Svg",
                "Audio",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Group",
                "Mask",
                "Filter",
                "Equation",
                "Fragment",
                "Callout",
                "Legend",
            ],
            transform_type(),
            PropertyValueKind::Generic,
        ),
        ("url", &["Image", "Svg"], Type::Str, PropertyValueKind::String),
        ("vertical_align", &["Row", "Col"], Type::Str, PropertyValueKind::String),
        ("volume", &["Audio"], Type::Num, PropertyValueKind::F32),
        (
            "width",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "Image",
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
                "Row",
                "Col",
                "Grid",
                "Stack",
                "Filter",
            ],
            Type::Num,
            PropertyValueKind::F32,
        ),
        ("word_spacing", &["Text", "Typst", "Code"], Type::Num, PropertyValueKind::F32),
        (
            "x_domain",
            &[
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "x_range",
            &["Graph", "PlotCurve", "NumberPlane"],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("x_scale", &["Graph"], Type::Str, PropertyValueKind::String),
        (
            "y_domain",
            &[
                "Graph",
                "PlotCurve",
                "VectorField",
                "Heatmap",
                "ContourSet",
                "NumberPlane",
                "BarChart",
            ],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        (
            "y_range",
            &["Graph", "PlotCurve", "NumberPlane"],
            Type::Vec2,
            PropertyValueKind::Vec2,
        ),
        ("y_scale", &["Graph"], Type::Str, PropertyValueKind::String),
    ];

    // Accepted source aliases that are not separate runtime storage entries.
    let aliases: Vec<(&'static str, &'static [&'static str], Type, PropertyValueKind)> = vec![
        ("content", &["Text", "Typst", "Code"], Type::Str, PropertyValueKind::String),
        ("language", &["Code"], Type::Str, PropertyValueKind::String),
        (
            "fill",
            &["Rect", "Ellipse", "Arrow", "Polygon", "Path"],
            Type::Color,
            PropertyValueKind::Vec4,
        ),
        ("radius", &["Rect", "Ellipse", "Polygon"], Type::Num, PropertyValueKind::F32),
        ("start", &["Line"], Type::Vec2, PropertyValueKind::Vec2),
        ("end", &["Line"], Type::Vec2, PropertyValueKind::Vec2),
        ("function", &["Graph", "PlotCurve"], Type::Str, PropertyValueKind::String),
        (
            "stroke_color",
            &[
                "Rect",
                "Ellipse",
                "Line",
                "Arrow",
                "Polygon",
                "Path",
                "PlotCurve",
            ],
            Type::Color,
            PropertyValueKind::Vec4,
        ),
    ];
    specs.extend(aliases);

    specs
}
#[cfg(test)]
mod tests {
    use super::{
        ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveSpec,
        builtin_primitive_specs, property_specs, schema_child_processing,
    };
    use crate::typing::Type;

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
        assert_eq!(schema_child_processing("Filter"), ChildProcessingKind::Filter);
        assert_eq!(schema_child_processing("Mask"), ChildProcessingKind::Mask);
        assert_eq!(schema_child_processing("Equation"), ChildProcessingKind::Equation);
        assert_eq!(schema_child_processing("Rect"), ChildProcessingKind::Generic);
    }
}
