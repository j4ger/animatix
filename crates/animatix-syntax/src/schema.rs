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

/// Stable property identifier used by runtime plans and schema consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// One known `(actor_type, property) -> Type` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertySpec {
    /// Stable id for this property/actor pair.
    pub id: PropertyId,
    /// Actor source type name, e.g. `Rect`.
    pub actor_type: &'static str,
    /// Canonical property name.
    pub name: &'static str,
    /// Inferred/declared property type.
    pub ty: Type,
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
    /// Is a container primitive.
    pub is_container: bool,
    /// Is a vector shape.
    pub is_shape: bool,
}

/// Metadata for a primitive in the shared schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveSpec {
    /// Source text type name, e.g. `Rect`.
    pub type_name: &'static str,
    /// Display name for GUI palettes.
    pub display_name: &'static str,
    /// UI category.
    pub category: PrimitiveCategory,
    /// Opaque icon id.
    pub icon_id: &'static str,
    /// Whether this primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// Engine capability flags.
    pub capabilities: PrimitiveCapabilities,
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
            type_name,
            display_name,
            category: *category,
            icon_id,
            advanced: *advanced,
            capabilities: schema_capabilities(type_name, *category),
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

/// All known property specs with stable ids assigned in declaration order.
pub fn property_specs() -> Vec<PropertySpec> {
    raw_property_specs()
        .into_iter()
        .enumerate()
        .map(|(index, (actor_type, name, ty))| PropertySpec {
            id: PropertyId(index as u32),
            actor_type,
            name,
            ty,
        })
        .collect()
}

fn raw_property_specs() -> Vec<(&'static str, &'static str, Type)> {
    let mut specs = Vec::new();

    let common: &[(&'static str, Type)] = &[
        ("position", Type::Vec2),
        ("anchor", Type::Vec2),
        ("offset", Type::Vec2),
        ("scale", Type::Num),
        ("rotation", Type::Num),
        ("opacity", Type::Num),
        ("color", Type::Color),
        ("at", Type::Vec2),
        ("transform", transform_type()),
    ];
    for ty in [
        "Text",
        "Code",
        "Typst",
        "Rect",
        "Ellipse",
        "Polygon",
        "Line",
        "Button",
        "Svg",
        "Image",
        "Graph",
        "PlotCurve",
    ] {
        specs.extend(
            common
                .iter()
                .map(|(property, property_type)| (ty, *property, property_type.clone())),
        );
    }

    specs.extend([
        // Text-specific
        ("Text", "text", Type::Str),
        ("Text", "content", Type::Str),
        ("Text", "font_size", Type::Num),
        ("Text", "font_family", Type::Str),
        ("Text", "font_weight", Type::Num),
        ("Text", "font_style", Type::Str),
        ("Text", "line_height", Type::Num),
        ("Text", "letter_spacing", Type::Num),
        ("Text", "word_spacing", Type::Num),
        ("Text", "max_width", Type::Num),
        ("Text", "text_align", Type::Str),
        ("Text", "overflow", Type::Str),
        // Typst-specific
        ("Typst", "content", Type::Str),
        ("Typst", "font_size", Type::Num),
        ("Typst", "font_family", Type::Str),
        ("Typst", "font_weight", Type::Num),
        ("Typst", "font_style", Type::Str),
        ("Typst", "line_height", Type::Num),
        ("Typst", "letter_spacing", Type::Num),
        ("Typst", "word_spacing", Type::Num),
        ("Typst", "max_width", Type::Num),
        ("Typst", "text_align", Type::Str),
        ("Typst", "overflow", Type::Str),
        // Code-specific
        ("Code", "code", Type::Str),
        ("Code", "content", Type::Str),
        ("Code", "language", Type::Str),
        ("Code", "font_weight", Type::Num),
        ("Code", "font_style", Type::Str),
        ("Code", "line_height", Type::Num),
        ("Code", "letter_spacing", Type::Num),
        ("Code", "word_spacing", Type::Num),
        ("Code", "max_width", Type::Num),
        ("Code", "text_align", Type::Str),
        ("Code", "overflow", Type::Str),
        // Shape-specific
        ("Rect", "fill", Type::Color),
        ("Rect", "stroke", Type::Color),
        ("Rect", "stroke_width", Type::Num),
        ("Rect", "size", Type::Vec2),
        ("Rect", "radius", Type::Num),
        ("Ellipse", "fill", Type::Color),
        ("Ellipse", "stroke", Type::Color),
        ("Ellipse", "stroke_width", Type::Num),
        ("Ellipse", "size", Type::Vec2),
        ("Ellipse", "radius", Type::Num),
        ("Polygon", "fill", Type::Color),
        ("Polygon", "stroke", Type::Color),
        ("Polygon", "stroke_width", Type::Num),
        ("Polygon", "size", Type::Vec2),
        ("Polygon", "radius", Type::Num),
        // Line
        ("Line", "start", Type::Vec2),
        ("Line", "end", Type::Vec2),
        ("Line", "stroke", Type::Color),
        ("Line", "stroke_width", Type::Num),
        // Button
        ("Button", "text", Type::Str),
        ("Button", "size", Type::Vec2),
        ("Button", "fill", Type::Color),
        ("Button", "stroke", Type::Color),
        // Svg/Image
        ("Svg", "url", Type::Str),
        ("Svg", "size", Type::Vec2),
        ("Image", "url", Type::Str),
        ("Image", "size", Type::Vec2),
        // Graph
        ("Graph", "x_range", Type::Vec2),
        ("Graph", "y_range", Type::Vec2),
        ("Graph", "function", Type::Str),
        ("PlotCurve", "x_range", Type::Vec2),
        ("PlotCurve", "y_range", Type::Vec2),
        ("PlotCurve", "function", Type::Str),
    ]);

    specs
}

#[cfg(test)]
mod tests {
    use super::{
        PrimitiveCapabilities, PrimitiveCategory, PrimitiveSpec, builtin_primitive_specs,
        property_specs,
    };
    use crate::typing::Type;

    #[test]
    fn shared_schema_contains_common_properties() {
        let specs = property_specs();
        assert!(specs.iter().any(|spec| spec.actor_type == "Rect" && spec.name == "position"));
        assert!(specs.iter().any(|spec| {
            spec.actor_type == "Text" && spec.name == "text" && spec.ty == Type::Str
        }));
    }

    #[test]
    fn transform_property_is_a_union() {
        let specs = property_specs();
        let transform = specs
            .iter()
            .find(|spec| spec.actor_type == "Rect" && spec.name == "transform")
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
            type_name: "Rect",
            display_name: "Rectangle",
            category: PrimitiveCategory::Shape,
            icon_id: "rect",
            advanced: false,
            capabilities: PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                ..PrimitiveCapabilities::default()
            },
        };
        assert_eq!(rect.category.label(), "Shapes");
        assert!(rect.capabilities.vector_paths);
        assert!(!rect.capabilities.layout_container);
    }
}
