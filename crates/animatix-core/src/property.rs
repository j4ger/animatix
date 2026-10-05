//! Built-in property descriptors: the single declaration of the property list.
//!
//! Both sides of the language derive from this table. The parser builds its
//! `PropertySpec`s from it, adding only the type-system view of each property
//! (which lives in `animatix-syntax` alongside the rest of the type model), and
//! the engine joins it with per-property runtime bindings (storage field,
//! flags, default, read source). Adding a property is therefore one row here,
//! one type row in the parser, and one binding in the engine — and a missing or
//! mismatched step fails a test instead of drifting silently, which is how two
//! hand-sorted tables behaved before.

use crate::caps::Applicable;

/// The reserved assignment target that addresses the scene camera.
///
/// `camera` is not an actor: `camera.at` / `camera.zoom` / `camera.rotation`
/// write a scene-wide transform. It lives here because both the parser's
/// label check and the engine's assignment routing need the same name, and
/// `animatix-core` is the only crate below both of them.
pub const CAMERA_TARGET: &str = "camera";

/// Stable property identifier used by runtime plans and schema consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertyId(pub u32);

/// Finite value kind understood by extension property tracks.
///
/// This mirrors the runtime dynamic-track storage while staying free of
/// renderer/runtime dependencies so the parser, analyzer, and LSP can consume
/// it too. It is deliberately coarser than the engine's `ValueType` (which adds
/// the shape/placement/anchor kinds) and than the parser's `Type` (which adds
/// the expression-system types); both sides map onto it, and both mappings are
/// pinned as *total* functions by tests.
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

/// One built-in property: canonical name, where it applies, and the value kind
/// its runtime track stores.
pub struct PropertyDescriptor {
    /// Canonical source-text name.
    pub name: &'static str,
    /// Actor types (or shape kinds) the property applies to.
    pub applicable: Applicable,
    /// Finite value kind stored by dynamic property tracks.
    pub value_kind: PropertyValueKind,
}

impl PropertyDescriptor {
    /// Const constructor for [`PROPERTY_DESCRIPTORS`].
    pub const fn new(
        name: &'static str,
        applicable: Applicable,
        value_kind: PropertyValueKind,
    ) -> Self {
        Self {
            name,
            applicable,
            value_kind,
        }
    }
}

/// Every built-in property, in declaration order.
///
/// **The order is the ABI.** [`PropertyId`] is the row index and serialized
/// plans (`PropertyPlan` slots, `persistence.rs`) store those ids, so appending
/// a row is safe but reordering or removing one invalidates saved files. The
/// `property_id_order_is_pinned` test guards it.
pub static PROPERTY_DESCRIPTORS: &[PropertyDescriptor] = &[
    PropertyDescriptor::new(
        "align",
        Applicable::Actors(&["Col", "Grid", "Row", "Stack"]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new("anchor", Applicable::Everything, PropertyValueKind::Generic),
    PropertyDescriptor::new("ascent", Applicable::Never, PropertyValueKind::F32),
    PropertyDescriptor::new("at", Applicable::Everything, PropertyValueKind::Vec2),
    PropertyDescriptor::new("background_color", Applicable::Never, PropertyValueKind::Vec4),
    PropertyDescriptor::new(
        "bar_colors",
        Applicable::Actors(&["BarChart"]),
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new("bar_width", Applicable::Actors(&["BarChart"]), PropertyValueKind::F32),
    PropertyDescriptor::new("baseline", Applicable::Never, PropertyValueKind::F32),
    PropertyDescriptor::new("bounds", Applicable::Actors(&["Filter"]), PropertyValueKind::Vec4),
    PropertyDescriptor::new("char_progress", Applicable::TextLike, PropertyValueKind::F32),
    PropertyDescriptor::new("code", Applicable::Actors(&["Code"]), PropertyValueKind::String),
    PropertyDescriptor::new("color", Applicable::Everything, PropertyValueKind::Vec4),
    PropertyDescriptor::new("cols", Applicable::Actors(&["Grid"]), PropertyValueKind::U32),
    PropertyDescriptor::new("commands", Applicable::Actors(&["Path"]), PropertyValueKind::Generic),
    PropertyDescriptor::new("data", Applicable::Actors(&["BarChart"]), PropertyValueKind::Generic),
    PropertyDescriptor::new(
        "density",
        Applicable::Actors(&["VectorField"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("descent", Applicable::Never, PropertyValueKind::F32),
    PropertyDescriptor::new(
        "direction",
        Applicable::Actors(&["BarChart"]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new(
        "fill_opacity",
        Applicable::AllShapesExceptLine,
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("font_family", Applicable::TextLike, PropertyValueKind::String),
    PropertyDescriptor::new(
        "font_size",
        // `Legend` draws its own labels through the text engine and reads
        // `font_size` off its props (`primitives/legend.rs`), so the row has to
        // include it or the applicability lint reports a working property as
        // dropped.
        Applicable::Actors(&["Code", "Equation", "Legend", "Math", "Text", "Typst"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("font_style", Applicable::TextLike, PropertyValueKind::String),
    PropertyDescriptor::new("font_weight", Applicable::TextLike, PropertyValueKind::F32),
    PropertyDescriptor::new(
        "from",
        Applicable::Actors(&["Arrow", "Callout", "Line"]),
        PropertyValueKind::Vec2,
    ),
    PropertyDescriptor::new(
        "func",
        Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve", "VectorField"]),
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new(
        "gap",
        // `BarChart` reads the same name as the bar spacing; for containers it is
        // the child gap. One row, two consumers — see `build/plot.rs`.
        Applicable::Actors(&[
            "BarChart", "Col", "Grid", "Group", "Legend", "Mask", "Row", "Stack",
        ]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("grid", Applicable::Actors(&["Graph"]), PropertyValueKind::String),
    PropertyDescriptor::new(
        "head_size",
        Applicable::Actors(&["Arrow", "Callout"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("height", Applicable::SizedActors, PropertyValueKind::F32),
    PropertyDescriptor::new(
        "highlight_color",
        Applicable::Actors(&["Equation", "Fragment"]),
        PropertyValueKind::Vec4,
    ),
    PropertyDescriptor::new(
        "highlight_opacity",
        Applicable::Actors(&["Equation", "Fragment"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new(
        "highlight_padding",
        Applicable::Actors(&["Equation", "Fragment"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new(
        "highlight_radius",
        Applicable::Actors(&["Equation", "Fragment"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("kind", Applicable::Actors(&["PlotCurve"]), PropertyValueKind::String),
    PropertyDescriptor::new("label", Applicable::Actors(&["Callout"]), PropertyValueKind::String),
    PropertyDescriptor::new("label_at", Applicable::Actors(&["Callout"]), PropertyValueKind::Vec2),
    PropertyDescriptor::new(
        "label_color",
        Applicable::Actors(&["Legend"]),
        PropertyValueKind::Vec4,
    ),
    PropertyDescriptor::new("legend", Applicable::Everything, PropertyValueKind::Generic),
    PropertyDescriptor::new(
        "letter_spacing",
        Applicable::Actors(&[
            "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
        ]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("levels", Applicable::Actors(&["ContourSet"]), PropertyValueKind::Vec2),
    PropertyDescriptor::new("line_cap", Applicable::AllShapes, PropertyValueKind::U32),
    PropertyDescriptor::new("line_height", Applicable::TextLike, PropertyValueKind::F32),
    PropertyDescriptor::new("line_join", Applicable::AllShapes, PropertyValueKind::U32),
    PropertyDescriptor::new(
        "max_depth",
        Applicable::Actors(&["ContourSet", "PlotCurve"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("max_height", Applicable::SizedActors, PropertyValueKind::F32),
    PropertyDescriptor::new("max_value", Applicable::Actors(&["BarChart"]), PropertyValueKind::F32),
    PropertyDescriptor::new(
        "max_width",
        Applicable::Actors(&[
            "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
        ]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("min_height", Applicable::SizedActors, PropertyValueKind::F32),
    PropertyDescriptor::new("min_width", Applicable::SizedActors, PropertyValueKind::F32),
    PropertyDescriptor::new("offset", Applicable::Everything, PropertyValueKind::Vec2),
    PropertyDescriptor::new("opacity", Applicable::Everything, PropertyValueKind::F32),
    PropertyDescriptor::new("overflow", Applicable::TextLike, PropertyValueKind::String),
    PropertyDescriptor::new(
        "padding",
        Applicable::Actors(&["Col", "Graph", "Grid", "Row", "Stack"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("place", Applicable::Actors(&["Callout"]), PropertyValueKind::Generic),
    PropertyDescriptor::new(
        "points",
        Applicable::Actors(&["Polygon"]),
        PropertyValueKind::PointList,
    ),
    PropertyDescriptor::new("position", Applicable::Everything, PropertyValueKind::Vec2),
    PropertyDescriptor::new("radius_x", Applicable::Actors(&["Ellipse"]), PropertyValueKind::F32),
    PropertyDescriptor::new("radius_y", Applicable::Actors(&["Ellipse"]), PropertyValueKind::F32),
    PropertyDescriptor::new(
        "resolution",
        Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("rotation", Applicable::Everything, PropertyValueKind::F32),
    PropertyDescriptor::new("scale", Applicable::Everything, PropertyValueKind::F32),
    PropertyDescriptor::new("shift", Applicable::Everything, PropertyValueKind::Vec2),
    PropertyDescriptor::new(
        "show_axis",
        Applicable::Actors(&["BarChart"]),
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new(
        "show_labels",
        Applicable::Actors(&["BarChart"]),
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new("size", Applicable::ExceptTextLike, PropertyValueKind::Vec2),
    // Authored solo flag: while any actor declares `solo: true`, every
    // non-solo subtree is hidden (recursively) in preview and export alike.
    PropertyDescriptor::new("solo", Applicable::Everything, PropertyValueKind::Bool),
    PropertyDescriptor::new("source", Applicable::Actors(&["Audio"]), PropertyValueKind::String),
    PropertyDescriptor::new("standoff", Applicable::Actors(&["Callout"]), PropertyValueKind::F32),
    PropertyDescriptor::new("stroke", Applicable::AllStrokePaths, PropertyValueKind::Vec4),
    PropertyDescriptor::new("stroke_progress", Applicable::AllStrokePaths, PropertyValueKind::F32),
    PropertyDescriptor::new(
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
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("swatch_size", Applicable::Actors(&["Legend"]), PropertyValueKind::F32),
    PropertyDescriptor::new(
        "t_domain",
        Applicable::Actors(&["PlotCurve"]),
        PropertyValueKind::Vec2,
    ),
    PropertyDescriptor::new("target", Applicable::Actors(&["Callout"]), PropertyValueKind::Generic),
    // The uniform body property for every text-like actor (`Text`, `Typst`,
    // `Code`, `Math`); `code` remains the `Code` spelling. It used to list only
    // Text and Math, which left `Typst` with no body property the analyzer
    // recognised once the redundant `content` alias was removed.
    PropertyDescriptor::new("text", Applicable::TextLike, PropertyValueKind::String),
    PropertyDescriptor::new("text_align", Applicable::TextLike, PropertyValueKind::String),
    PropertyDescriptor::new(
        "text_max_width",
        // `Legend` has its own legend label wrapping, and the text engine reads
        // this as the canonical wrap width (`declarations_text.rs`: "Canonical
        // name per spec; `max_width` kept as a legacy alias"). The row used to
        // list only `Legend`, which is why 94 site scenes could set a wrap width
        // on a `Text` that the table claimed did not take one.
        Applicable::Any(&[Applicable::Actors(&["Legend"]), Applicable::TextLike]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new(
        "tick_labels",
        Applicable::Actors(&["Graph"]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new("ticks", Applicable::Actors(&["Graph"]), PropertyValueKind::String),
    PropertyDescriptor::new("title", Applicable::Actors(&["Legend"]), PropertyValueKind::String),
    PropertyDescriptor::new(
        "to",
        Applicable::Actors(&["Arrow", "Callout", "Line"]),
        PropertyValueKind::Vec2,
    ),
    PropertyDescriptor::new("to_offset", Applicable::Actors(&["Callout"]), PropertyValueKind::Vec2),
    PropertyDescriptor::new(
        "tolerance",
        Applicable::Actors(&["PlotCurve"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("transform", Applicable::Everything, PropertyValueKind::Generic),
    PropertyDescriptor::new(
        "url",
        Applicable::Actors(&["Image", "Svg"]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new(
        "vertical_align",
        Applicable::Actors(&["Col", "Row"]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new("volume", Applicable::Actors(&["Audio"]), PropertyValueKind::F32),
    PropertyDescriptor::new("width", Applicable::SizedActors, PropertyValueKind::F32),
    PropertyDescriptor::new("word_spacing", Applicable::TextLike, PropertyValueKind::F32),
    PropertyDescriptor::new("x_domain", Applicable::PlotGeometry, PropertyValueKind::Vec2),
    PropertyDescriptor::new(
        "x_range",
        Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]),
        PropertyValueKind::Vec2,
    ),
    PropertyDescriptor::new("x_scale", Applicable::Actors(&["Graph"]), PropertyValueKind::String),
    PropertyDescriptor::new("y_domain", Applicable::PlotGeometry, PropertyValueKind::Vec2),
    PropertyDescriptor::new(
        "y_range",
        Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]),
        PropertyValueKind::Vec2,
    ),
    PropertyDescriptor::new("y_scale", Applicable::Actors(&["Graph"]), PropertyValueKind::String),
    // Appended last on purpose: `PropertyId` is the row index, so new rows go
    // at the end rather than into name order.
    // `Glass` paints no surface, but its radius is not decorative: it is the
    // rounded clip the backdrop is blitted through.
    PropertyDescriptor::new(
        "corner_radius",
        Applicable::Actors(&["Rect", "Glass"]),
        PropertyValueKind::F32,
    ),
    PropertyDescriptor::new("language", Applicable::Actors(&["Code"]), PropertyValueKind::String),
    PropertyDescriptor::new("dash_offset", Applicable::AllStrokePaths, PropertyValueKind::F32),
    PropertyDescriptor::new("dash_pattern", Applicable::AllStrokePaths, PropertyValueKind::Generic),
    PropertyDescriptor::new("blend", Applicable::Everything, PropertyValueKind::String),
    PropertyDescriptor::new(
        "fill_gradient",
        Applicable::AllShapesExceptLine,
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new(
        "stroke_gradient",
        Applicable::AllStrokePaths,
        PropertyValueKind::Generic,
    ),
    PropertyDescriptor::new(
        "gradient_extend",
        Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]),
        PropertyValueKind::String,
    ),
    PropertyDescriptor::new(
        "gradient_space",
        Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]),
        PropertyValueKind::String,
    ),
    // Bundled stroke-icon name (`icon: "check"`). Build-time only: the engine
    // expands the name into `Path` command geometry and stores nothing on a
    // track, so it is declared `Generic` to match its `BuildTimeOnly` runtime
    // binding (mirrors how `data`/`bar_width` are handled).
    PropertyDescriptor::new("icon", Applicable::Actors(&["Path"]), PropertyValueKind::Generic),
    // The scene camera's per-actor opt-out. `Applicable::Everything` because the
    // camera wraps every root subtree, so any actor may need to sit outside it
    // (a HUD, a watermark, a caption pinned to the viewport). Declared `Generic`
    // to match its `BuildTimeOnly` runtime binding, exactly as `icon` does: the
    // value is read once at build into a plain track field and never keyframed.
    PropertyDescriptor::new("camera_follow", Applicable::Everything, PropertyValueKind::Generic),
];

/// The descriptor for `name`, when it is a built-in property.
pub fn descriptor(name: &str) -> Option<&'static PropertyDescriptor> {
    PROPERTY_DESCRIPTORS.iter().find(|d| d.name == name)
}

/// The [`PropertyId`] for `name` — its index in [`PROPERTY_DESCRIPTORS`].
pub fn descriptor_id(name: &str) -> Option<PropertyId> {
    PROPERTY_DESCRIPTORS
        .iter()
        .position(|d| d.name == name)
        .map(|index| PropertyId(index as u32))
}
