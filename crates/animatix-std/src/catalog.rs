//! The built-in primitive catalog: one [`PrimitiveInfo`] identity card per
//! built-in actor type.
//!
//! This is the single author-visible source for primitive metadata — display
//! name, category, icon, capabilities, child processing, and the shape/text
//! projections. Each built-in is one `pub static` card below, assembled into
//! [`CATALOG`]; the engine's `BUILT_INS` rows reference these cards by symbol,
//! so metadata and behaviour cannot drift positionally. The parser crate
//! derives its contract tables from the catalog. Adding a built-in primitive
//! is one behaviour file in the engine plus one card here plus one `BUILT_INS`
//! registration row — no parallel declarations anywhere else.

use std::borrow::Cow;

use animatix_core::caps::{
    ActorCaps, ActorCategory, ChildProcessingKind, PlotKind, PrimitiveCapabilities, ShapeKind,
    TextKind,
};
use animatix_core::icon_glyphs;

/// The engine-visible identity card of one built-in primitive.
///
/// Built-ins borrow string literals (`Cow::Borrowed`, no allocation); an
/// extension primitive owns the names it received over FFI (`Cow::Owned`), so
/// registering one allocates nothing permanently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveInfo {
    /// Authored type name (`.amx`) — the registry key.
    pub type_name: Cow<'static, str>,
    /// Human-readable label for UI palettes and tooltips.
    pub display_name: Cow<'static, str>,
    /// UI category.
    pub category: ActorCategory,
    /// Opaque icon identifier (an [`animatix_core::icon_glyphs`] constant).
    pub icon_id: Cow<'static, str>,
    /// Whether shown in an "advanced" submenu instead of top-level.
    pub advanced: bool,
    /// Engine capabilities.
    pub capabilities: PrimitiveCapabilities,
    /// Child-processing strategy.
    pub child_processing: ChildProcessingKind,
    /// The concrete shape geometry, for shape primitives.
    pub shape: Option<ShapeKind>,
    /// The text engine backing this actor, for text-like primitives.
    pub text: Option<TextKind>,
    /// Which plot family this actor belongs to, for plot primitives. Carried to
    /// the engine through `PrimitiveFamilyDescriptor`, replacing the plot
    /// builder's `ty == "VectorField"`-style string compares.
    pub plot: Option<PlotKind>,
    /// Renders as a stroke-based path (shapes and `PlotCurve`).
    pub stroke_path: bool,
    /// The generic actor-declaration body writes this primitive's declaration
    /// properties into the connector geometry lanes (`from` / `to` / `routing` /
    /// `arrow`), so the primitive is deliberately *not* dispatched to
    /// [`crate::PrimitiveInfo`]'s `Primitive::build` path. Catalog data rather
    /// than a `ty == "Connector"` compare at each dispatch site.
    pub connector: bool,
    /// Author-facing documentation for completions, tooltips, and hover info.
    pub doc: &'static str,
    /// Default snippet / property stub when inserting or stubbing this primitive.
    pub default_stub: Option<&'static str>,
}

const fn caps(
    text_paths: bool,
    vector_paths: bool,
    image_payload: bool,
    layout_container: bool,
    plot_geometry: bool,
    plot_host: bool,
    is_container: bool,
    is_shape: bool,
    has_visual_content: bool,
    stroke_primary: bool,
    callout: bool,
    legend_host: bool,
    time_dependent: bool,
) -> PrimitiveCapabilities {
    PrimitiveCapabilities {
        text_paths,
        vector_paths,
        image_payload,
        layout_container,
        plot_geometry,
        plot_host,
        is_container,
        is_shape,
        has_visual_content,
        stroke_primary,
        callout,
        legend_host,
        time_dependent,
    }
}

const SHAPE_CAPS: PrimitiveCapabilities = caps(
    false, true, false, false, false, false, false, true, true, false, false, false, false,
);
const TEXT_CAPS: PrimitiveCapabilities = caps(
    true, false, false, false, false, false, false, false, true, false, false, false, false,
);
const PLOT_CAPS: PrimitiveCapabilities = caps(
    false, true, false, false, true, false, false, false, true, false, false, false, false,
);
const CONTAINER_CAPS: PrimitiveCapabilities = caps(
    false, false, false, true, false, false, true, false, false, false, false, false, false,
);
const EQUATION_CAPS: PrimitiveCapabilities = caps(
    false, false, false, true, false, false, true, false, true, false, false, false, false,
);
const CAPTIONS_CAPS: PrimitiveCapabilities = caps(
    true, false, false, true, false, false, true, false, true, false, false, false, true,
);
const CUE_CAPS: PrimitiveCapabilities = caps(
    true, false, false, false, false, false, false, false, true, false, false, false, true,
);
/// `Glass` is a container with a rectangular *region*, not a surface. It carries
/// no `ShapeKind` — the same choice `Filter` and `Mask` make — so the shape
/// predicates (`fill_opacity`, the gradient paints) key off `caps.shape.is_some()`
/// and do not invite a scope that paints nothing; `corner_radius` stays applicable
/// through its own actor list because it is the frost's clip, and `is_shape` stays
/// set because the region and its anchors need a bounded actor.
const GLASS_CAPS: PrimitiveCapabilities = caps(
    false, false, false, true, false, false, true, true, false, false, false, false, false,
);
const GROUP_CAPS: PrimitiveCapabilities = caps(
    false, false, false, false, false, false, true, false, false, false, false, false, false,
);
const NO_CAPS: PrimitiveCapabilities = caps(
    false, false, false, false, false, false, false, false, false, false, false, false, false,
);
const IMAGE_CAPS: PrimitiveCapabilities = caps(
    false, false, true, false, false, false, false, false, true, false, false, false, false,
);
const SVG_CAPS: PrimitiveCapabilities = caps(
    false, true, false, false, false, false, false, false, true, false, false, false, false,
);
const GRAPH_CAPS: PrimitiveCapabilities = caps(
    false, true, false, false, true, true, false, false, true, false, false, false, false,
);
const ANNOTATION_CAPS: PrimitiveCapabilities = caps(
    false, true, false, false, false, false, false, false, true, false, false, false, false,
);

impl PrimitiveInfo {
    /// A built-in card in `category` with `capabilities`: `Generic` child
    /// processing, no shape/text projection, no stroke, top-level. Refine
    /// with the builders below, or use a kind constructor ([[`Self::shape`]],
    /// [[`Self::text`]]) for the common families.
    pub const fn new(
        type_name: &'static str,
        display_name: &'static str,
        icon: &'static str,
        category: ActorCategory,
        capabilities: PrimitiveCapabilities,
    ) -> Self {
        Self {
            type_name: Cow::Borrowed(type_name),
            display_name: Cow::Borrowed(display_name),
            category,
            icon_id: Cow::Borrowed(icon),
            advanced: false,
            capabilities,
            child_processing: ChildProcessingKind::Generic,
            shape: None,
            text: None,
            plot: None,
            stroke_path: false,
            connector: false,
            doc: "",
            default_stub: None,
        }
    }

    /// A vector-shape card: shape capabilities, one [`ShapeKind`] geometry,
    /// stroke rendering.
    pub const fn shape(
        type_name: &'static str,
        display_name: &'static str,
        icon: &'static str,
        kind: ShapeKind,
    ) -> Self {
        let stroke_primary = matches!(kind, ShapeKind::Line | ShapeKind::Arrow);
        let mut capabilities = SHAPE_CAPS;
        capabilities.stroke_primary = stroke_primary;
        Self {
            type_name: Cow::Borrowed(type_name),
            display_name: Cow::Borrowed(display_name),
            category: ActorCategory::Shape,
            icon_id: Cow::Borrowed(icon),
            advanced: false,
            capabilities,
            child_processing: ChildProcessingKind::Generic,
            shape: Some(kind),
            text: None,
            plot: None,
            stroke_path: true,
            connector: false,
            doc: "",
            default_stub: None,
        }
    }

    /// A text-like card backed by one [`TextKind`] engine.
    pub const fn text(
        type_name: &'static str,
        display_name: &'static str,
        icon: &'static str,
        kind: TextKind,
    ) -> Self {
        Self {
            type_name: Cow::Borrowed(type_name),
            display_name: Cow::Borrowed(display_name),
            category: ActorCategory::Text,
            icon_id: Cow::Borrowed(icon),
            advanced: false,
            capabilities: TEXT_CAPS,
            child_processing: ChildProcessingKind::Generic,
            shape: None,
            text: Some(kind),
            plot: None,
            stroke_path: false,
            connector: false,
            doc: "",
            default_stub: None,
        }
    }

    /// Attach author-facing documentation to this primitive card.
    pub const fn with_doc(mut self, doc: &'static str) -> Self {
        self.doc = doc;
        self
    }

    /// Attach a default snippet/stub to this primitive card.
    pub const fn with_stub(mut self, stub: &'static str) -> Self {
        self.default_stub = Some(stub);
        self
    }

    /// Move the card into the palette's "advanced" submenu.
    pub const fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    /// Mark the card stroke-based.
    pub const fn stroked(mut self) -> Self {
        self.stroke_path = true;
        self
    }

    /// Mark the card as one whose declaration properties the generic actor body
    /// writes into the connector geometry lanes (see [`Self::connector`]).
    pub const fn connector_geometry(mut self) -> Self {
        self.connector = true;
        self
    }

    /// Declare which plot family this card is (see [`Self::plot`]).
    pub const fn plot_kind(mut self, kind: PlotKind) -> Self {
        self.plot = Some(kind);
        self
    }

    /// Mark the card as primarily stroke-drawn (e.g. Line, Arrow, Callout, PlotCurve, VectorField,
    /// ContourSet).
    pub const fn stroke_primary(mut self) -> Self {
        self.capabilities.stroke_primary = true;
        self
    }

    /// Mark the card as having callout geometry.
    pub const fn callout(mut self) -> Self {
        self.capabilities.callout = true;
        self
    }

    /// Mark the card as hosting a series legend.
    pub const fn legend_host(mut self) -> Self {
        self.capabilities.legend_host = true;
        self
    }

    /// Mark the card as intrinsically time-dependent (evaluation varies over time without keyframes).
    pub const fn time_dependent(mut self) -> Self {
        self.capabilities.time_dependent = true;
        self
    }

    /// Override the child-processing strategy (`Mask`/`Filter`/`Equation`).
    pub const fn with_child_processing(mut self, kind: ChildProcessingKind) -> Self {
        self.child_processing = kind;
        self
    }

    /// Attach a shape geometry variant to a card that is not declared through
    /// [`PrimitiveInfo::shape`], so a container can draw a surface of its own.
    pub const fn with_shape(mut self, kind: ShapeKind) -> Self {
        self.shape = Some(kind);
        self
    }
}

// ── The identity cards ──────────────────────────────────────────────────

// Shapes
pub static RECT: PrimitiveInfo =
    PrimitiveInfo::shape("Rect", "Rectangle", icon_glyphs::SQUARE, ShapeKind::Rect)
        .with_doc("Rectangle shape with fill and stroke.")
        .with_stub("size: (100, 100)");
pub static ELLIPSE: PrimitiveInfo =
    PrimitiveInfo::shape("Ellipse", "Ellipse", icon_glyphs::CIRCLE_NOTCH, ShapeKind::Ellipse)
        .with_doc("Ellipse, circle, arc, or dot shape.")
        .with_stub("size: (100, 100)");
pub static LINE: PrimitiveInfo =
    PrimitiveInfo::shape("Line", "Line", icon_glyphs::MINUS, ShapeKind::Line)
        .with_doc("Line segment or arrow with optional head.")
        .with_stub("from: (0, 0), to: (100, 0)");
pub static ARROW: PrimitiveInfo =
    PrimitiveInfo::shape("Arrow", "Arrow", icon_glyphs::ARROW_RIGHT, ShapeKind::Arrow)
        .with_doc("Directional arrow with customizable head and shaft.")
        .with_stub("from: (0, 0), to: (100, 0)");
pub static POLYGON: PrimitiveInfo =
    PrimitiveInfo::shape("Polygon", "Polygon", icon_glyphs::POLYGON, ShapeKind::Polygon)
        .with_doc("Polygon or regular polygon shape.")
        .with_stub("points: {(0, 0), (100, 0), (50, 80)}");
pub static PATH: PrimitiveInfo =
    PrimitiveInfo::shape("Path", "Path", icon_glyphs::PEN, ShapeKind::Path)
        .with_doc("SVG path element.")
        .with_stub("commands: \"M 0 0 L 100 0\"");

// Text
pub static TEXT: PrimitiveInfo =
    PrimitiveInfo::text("Text", "Text", icon_glyphs::TEXT_T, TextKind::Text)
        .with_doc("Text element with content and styling properties.")
        .with_stub("text: \"Hello\"");
pub static CODE: PrimitiveInfo =
    PrimitiveInfo::text("Code", "Code", icon_glyphs::CODE, TextKind::Code)
        .advanced()
        .with_doc("Code block with syntax highlighting.")
        .with_stub("code: \"let x = 1;\", language: \"rust\"");
pub static MATH: PrimitiveInfo =
    PrimitiveInfo::text("Math", "Math", icon_glyphs::FUNCTION, TextKind::Math)
        .advanced()
        .with_doc("Mathematical formula notation using Typst math.")
        .with_stub("text: \"E = m c^2\"");
pub static TYPST: PrimitiveInfo =
    PrimitiveInfo::text("Typst", "Typst", icon_glyphs::ARTICLE, TextKind::Typst)
        .advanced()
        .with_doc("Typst markup and mathematical document fragment.")
        .with_stub("text: \"x^2 + y^2 = r^2\"");
pub static COUNTER: PrimitiveInfo =
    PrimitiveInfo::text("Counter", "Counter", icon_glyphs::HASH, TextKind::Text)
        .with_doc("Animated numerical counter with formatting and decimals.")
        .with_stub("value: 100");

// Media
pub static IMAGE: PrimitiveInfo =
    PrimitiveInfo::new("Image", "Image", icon_glyphs::IMAGE, ActorCategory::Media, IMAGE_CAPS)
        .with_doc("Raster image element.")
        .with_stub("url: \"image.png\"");
pub static SVG: PrimitiveInfo =
    PrimitiveInfo::new("Svg", "SVG", icon_glyphs::VECTOR_THREE, ActorCategory::Media, SVG_CAPS)
        .advanced()
        .with_doc("SVG image element.")
        .with_stub("url: \"graphic.svg\"");
pub static AUDIO: PrimitiveInfo =
    PrimitiveInfo::new("Audio", "Audio", icon_glyphs::SPEAKER_HIGH, ActorCategory::Media, NO_CAPS)
        .advanced()
        .with_doc("Audio track playback and synchronization element.")
        .with_stub("source: \"sound.mp3\"");

// Plots
pub static GRAPH: PrimitiveInfo =
    PrimitiveInfo::new("Graph", "Graph", icon_glyphs::CHART_BAR, ActorCategory::Plot, GRAPH_CAPS)
        .stroked()
        .with_doc("2D coordinate host with customizable axis domains and ticks.")
        .with_stub("x_domain: (0, 10), y_domain: (0, 10)");
pub static PLOT_CURVE: PrimitiveInfo = PrimitiveInfo::new(
    "PlotCurve",
    "Plot Curve",
    icon_glyphs::CHART_LINE_UP,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.plot_kind(PlotKind::Curve)
.advanced()
.stroked()
.stroke_primary()
.with_doc("Plot curve with configurable sampling kind.")
.with_stub("func: (x) => sin(x)");
pub static VECTOR_FIELD: PrimitiveInfo = PrimitiveInfo::new(
    "VectorField",
    "Vector Field",
    icon_glyphs::ARROWS_OUT_CARDINAL,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.plot_kind(PlotKind::VectorField)
.advanced()
.stroked()
.stroke_primary()
.with_doc("2D vector direction field visualization.")
.with_stub("func: (x, y) => (y, -x)");
pub static HEATMAP: PrimitiveInfo =
    PrimitiveInfo::new("Heatmap", "Heatmap", icon_glyphs::GRADIENT, ActorCategory::Plot, PLOT_CAPS)
        .plot_kind(PlotKind::Heatmap)
        .advanced()
        .stroked()
        .with_doc("2D scalar density heatmap plot.")
        .with_stub("func: (x, y) => sin(x) * cos(y)");
pub static CONTOUR_SET: PrimitiveInfo = PrimitiveInfo::new(
    "ContourSet",
    "Contour Set",
    icon_glyphs::CHART_DONUT,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.plot_kind(PlotKind::ContourSet)
.advanced()
.stroked()
.stroke_primary()
.with_doc("2D isocontour line plot.")
.with_stub("func: (x, y) => x^2 + y^2");
pub static NUMBER_PLANE: PrimitiveInfo = PrimitiveInfo::new(
    "NumberPlane",
    "Number Plane",
    icon_glyphs::SQUARES_FOUR,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.plot_kind(PlotKind::NumberPlane)
.stroked()
.with_doc("Cartesian coordinate plane with grid lines and axes.")
.with_stub("x_range: (-5, 5), y_range: (-5, 5)");
pub static BAR_CHART: PrimitiveInfo = PrimitiveInfo::new(
    "BarChart",
    "Bar Chart",
    icon_glyphs::CHART_BAR,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.plot_kind(PlotKind::BarChart)
.with_doc("Categorical bar chart visualization.")
.with_stub("data: {(\"A\", 10), (\"B\", 20)}");

// Containers
pub static ROW: PrimitiveInfo =
    PrimitiveInfo::new("Row", "Row", icon_glyphs::ROWS, ActorCategory::Container, CONTAINER_CAPS)
        .with_doc("Horizontal flex-like container for child actors.")
        .with_stub("gap: 10");
pub static COL: PrimitiveInfo = PrimitiveInfo::new(
    "Col",
    "Column",
    icon_glyphs::COLUMNS,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_doc("Vertical flex-like container for child actors.")
.with_stub("gap: 10");
pub static GRID: PrimitiveInfo = PrimitiveInfo::new(
    "Grid",
    "Grid",
    icon_glyphs::SQUARES_FOUR,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_doc("2D grid container with configurable columns and spacing.")
.with_stub("cols: 3, gap: 10");
pub static STACK: PrimitiveInfo = PrimitiveInfo::new(
    "Stack",
    "Stack",
    icon_glyphs::STACK,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_doc("Z-ordered overlapping layout container.");
pub static GROUP: PrimitiveInfo =
    PrimitiveInfo::new("Group", "Group", icon_glyphs::FOLDER, ActorCategory::Container, GROUP_CAPS)
        .with_doc("Hierarchical grouping container without layout constraints.");
pub static MASK: PrimitiveInfo = PrimitiveInfo::new(
    "Mask",
    "Mask",
    icon_glyphs::MASK_HAPPY,
    ActorCategory::Container,
    GROUP_CAPS,
)
.advanced()
.with_child_processing(ChildProcessingKind::Mask)
.with_doc("Masking container that clips children to a shape.");
pub static FILTER: PrimitiveInfo = PrimitiveInfo::new(
    "Filter",
    "Filter",
    icon_glyphs::FILTERS,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_child_processing(ChildProcessingKind::Filter)
.with_doc("Compositing scope that applies GPU shader effect chains.");
/// A backdrop-blur scope: whatever the frame holds behind its rect is blurred
/// and the scope's children composite above that blur. It draws no surface of
/// its own — the frost samples the finished frame, so a fill or border on the
/// scope itself would be inside the pixels that get blurred. Its `size` is the
/// frosted region and its `corner_radius` is what that region is clipped to.
pub static GLASS: PrimitiveInfo = PrimitiveInfo::new(
    "Glass",
    "Glass",
    icon_glyphs::SQUARES_FOUR,
    ActorCategory::Container,
    GLASS_CAPS,
)
.with_child_processing(ChildProcessingKind::Glass)
.with_doc("Frost/blur glass container that blurs content behind it.")
.with_stub("corner_radius: 8");
pub static EQUATION: PrimitiveInfo = PrimitiveInfo::new(
    "Equation",
    "Equation",
    icon_glyphs::SIGMA,
    ActorCategory::Container,
    EQUATION_CAPS,
)
.with_child_processing(ChildProcessingKind::Equation)
.with_doc("Aggregates Typst equation fragments with animation support.");

// Equation fragment sub-item (text-like caps, but no single TextKind engine)
pub static FRAGMENT: PrimitiveInfo = PrimitiveInfo::new(
    "Fragment",
    "Fragment",
    icon_glyphs::HIGHLIGHTER,
    ActorCategory::Text,
    TEXT_CAPS,
)
.with_doc("Sub-equation fragment within an Equation container.")
.with_stub("text: \"x\"");

// Captions container & Cue sub-item
pub static CAPTIONS: PrimitiveInfo = PrimitiveInfo::new(
    "Captions",
    "Captions",
    icon_glyphs::ARTICLE,
    ActorCategory::Container,
    CAPTIONS_CAPS,
)
.with_child_processing(ChildProcessingKind::Captions)
.with_doc("Aggregates speech-synced subtitle cues with karaoke and kinetic styling.");

pub static CUE: PrimitiveInfo =
    PrimitiveInfo::new("Cue", "Cue", icon_glyphs::TEXT_T, ActorCategory::Text, CUE_CAPS)
        .with_doc("Individual timed subtitle cue inside a Captions container.")
        .with_stub("text: \"\"");

// Annotations
pub static CALLOUT: PrimitiveInfo = PrimitiveInfo::new(
    "Callout",
    "Callout",
    icon_glyphs::TEXT_T,
    ActorCategory::Annotation,
    ANNOTATION_CAPS,
)
.stroked()
.stroke_primary()
.callout()
.with_doc("Targeted leader line and annotation callout.")
.with_stub("to: (100, 100)");
pub static LEGEND: PrimitiveInfo = PrimitiveInfo::new(
    "Legend",
    "Legend",
    icon_glyphs::CHART_LINE_UP,
    ActorCategory::Annotation,
    ANNOTATION_CAPS,
)
.legend_host()
.with_doc("Color and series legend for plot and chart elements.");
pub static CONNECTOR: PrimitiveInfo = PrimitiveInfo::new(
    "Connector",
    "Connector",
    icon_glyphs::ARROW_RIGHT,
    ActorCategory::Annotation,
    ANNOTATION_CAPS,
)
.stroked()
.stroke_primary()
.connector_geometry()
.with_doc("Connecting line or curve between two actor anchors.");

/// The built-in primitive catalog.
///
/// The order here is presentation-only (palette grouping and docs tables).
/// The engine pairs behaviours with these cards by symbol in
/// `animatix::primitives::BUILT_INS`, so this list and that one cannot
/// drift positionally — only name-set drift is possible, which the
/// engine's `registry_matches_primitives` test pins.
pub static CATALOG: &[&PrimitiveInfo] = &[
    // Shapes
    &RECT,
    &ELLIPSE,
    &LINE,
    &ARROW,
    &POLYGON,
    &PATH,
    // Text
    &TEXT,
    &CODE,
    &MATH,
    &TYPST,
    &COUNTER,
    // Media
    &IMAGE,
    &SVG,
    &AUDIO,
    // Plots
    &GRAPH,
    &PLOT_CURVE,
    &VECTOR_FIELD,
    &HEATMAP,
    &CONTOUR_SET,
    &NUMBER_PLANE,
    &BAR_CHART,
    // Containers
    &ROW,
    &COL,
    &GRID,
    &STACK,
    &GROUP,
    &MASK,
    &FILTER,
    &GLASS,
    &EQUATION,
    // Equation fragment sub-item
    &FRAGMENT,
    // Captions container & Cue sub-item
    &CAPTIONS,
    &CUE,
    // Annotations
    &CALLOUT,
    &LEGEND,
    &CONNECTOR,
];

impl PrimitiveInfo {
    /// A baseline identity card for an extension primitive: generic child
    /// processing, no capabilities, display name = type name. Callers mutate
    /// the fields for text-like/shape-like/layout extensions — the same data
    /// the native ABI descriptor carries.
    pub fn extension(type_name: impl Into<Cow<'static, str>>, category: ActorCategory) -> Self {
        let type_name = type_name.into();
        Self {
            type_name: type_name.clone(),
            display_name: type_name,
            category,
            icon_id: Cow::Borrowed("extension"),
            advanced: false,
            capabilities: PrimitiveCapabilities::default(),
            child_processing: ChildProcessingKind::Generic,
            shape: None,
            text: None,
            plot: None,
            stroke_path: false,
            connector: false,
            doc: "",
            default_stub: None,
        }
    }
}

impl PrimitiveInfo {
    /// The authored type name as a `'static` string.
    ///
    /// Always `Some` for catalog rows (their names are string literals) and
    /// `None` for extension rows, whose names are heap-allocated. Callers that
    /// need a `'static` table (the parser's contract tables) use this to skip
    /// extension rows explicitly instead of leaking.
    pub fn static_type_name(&self) -> Option<&'static str> {
        match &self.type_name {
            Cow::Borrowed(name) => Some(name),
            Cow::Owned(_) => None,
        }
    }

    /// The icon glyph as a `'static` string, for built-in rows only (see
    /// [`Self::static_type_name`]).
    pub fn static_icon_id(&self) -> Option<&'static str> {
        match &self.icon_id {
            Cow::Borrowed(icon) => Some(icon),
            Cow::Owned(_) => None,
        }
    }
}

/// Look up a catalog row by authored type name.
pub fn catalog_lookup(name: &str) -> Option<&'static PrimitiveInfo> {
    CATALOG.iter().copied().find(|info| info.type_name.as_ref() == name)
}

/// Derive the engine's capability projection from a catalog row.
pub fn caps_from_info(info: &PrimitiveInfo) -> ActorCaps {
    let stroke_primary = info.capabilities.stroke_primary
        || (info.shape.is_some_and(|k| matches!(k, ShapeKind::Line | ShapeKind::Arrow)));
    ActorCaps {
        category: info.category,
        child_processing: info.child_processing,
        shape: info.shape,
        text: info.text,
        is_shape: info.capabilities.is_shape,
        stroke_path: info.stroke_path || stroke_primary || info.capabilities.callout,
        text_paths: info.capabilities.text_paths,
        vector_paths: info.capabilities.vector_paths,
        image_payload: info.capabilities.image_payload,
        layout_container: info.capabilities.layout_container,
        is_container: info.capabilities.is_container,
        plot_geometry: info.capabilities.plot_geometry,
        plot_host: info.capabilities.plot_host,
        time_dependent: info.capabilities.time_dependent,
        has_visual_content: info.capabilities.has_visual_content
            || (!info.capabilities.is_container
                && (info.capabilities.is_shape
                    || info.capabilities.text_paths
                    || info.capabilities.image_payload
                    || info.capabilities.vector_paths
                    || info.capabilities.plot_geometry
                    || info.capabilities.plot_host
                    || matches!(
                        info.category,
                        ActorCategory::Shape
                            | ActorCategory::Text
                            | ActorCategory::Plot
                            | ActorCategory::Annotation
                    ))),
        stroke_primary,
        callout: info.capabilities.callout,
        legend_host: info.capabilities.legend_host,
    }
}

/// Derive caps for an authored type name from the catalog. Returns `None` for
/// unregistered names — callers decide the fallback (the all-false
/// [`ActorCaps::default()`] preserves the historical extension behaviour).
pub fn caps_for_type(actor_type: &str) -> Option<ActorCaps> {
    catalog_lookup(actor_type).map(caps_from_info)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// The "LLM Generation Checklist" in `docs/spec.md` lists the supported
    /// primitives by hand; this test pins that list to the catalog so the
    /// author-facing name list cannot drift when cards are added or removed.
    #[test]
    fn spec_llm_checklist_primitives_match_catalog() {
        let spec = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/spec.md"));
        let line = spec
            .lines()
            .find(|l| l.starts_with("- Use supported primitives only:"))
            .expect("docs/spec.md must carry the LLM checklist primitive line");
        let documented: HashSet<&str> = line.split('`').skip(1).step_by(2).collect();
        let catalog: HashSet<&str> = CATALOG.iter().map(|info| &*info.type_name).collect();
        let missing: Vec<&&str> = catalog.difference(&documented).collect();
        let extra: Vec<&&str> = documented.difference(&catalog).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "docs/spec.md LLM checklist out of sync with CATALOG: missing {missing:?}, extra {extra:?}"
        );
    }
}

/// `connector` is the catalog datum that keeps Connector out of the
/// `Primitive::build` dispatch: its declaration properties are written into
/// the connector geometry lanes by the generic actor body. Exactly one card
/// may set it, so a new annotation primitive cannot silently inherit the
/// inline path (or lose Connector's) without this failing first.
#[test]
fn exactly_one_card_declares_connector_geometry() {
    let flagged: Vec<&str> = CATALOG
        .iter()
        .filter(|info| info.connector)
        .map(|info| &*info.type_name)
        .collect();
    assert_eq!(
        flagged,
        vec!["Connector"],
        "the connector-geometry card set changed; update `find_actor_kind` and the \
             generic body's connector block together"
    );
}

/// Every plot card declares its `PlotKind`, and no non-plot card does. The
/// plot builder reads this instead of comparing type-name strings, so a new
/// plot primitive that forgets `plot_kind(...)` would silently fall through
/// to the `Curve` path.
#[test]
fn plot_cards_declare_their_plot_kind() {
    use animatix_core::caps::PlotKind;
    let mut declared: Vec<(&str, PlotKind)> = CATALOG
        .iter()
        .filter_map(|info| info.plot.map(|kind| (&*info.type_name, kind)))
        .collect();
    declared.sort_by_key(|(name, _)| *name);
    assert_eq!(
        declared,
        vec![
            ("BarChart", PlotKind::BarChart),
            ("ContourSet", PlotKind::ContourSet),
            ("Heatmap", PlotKind::Heatmap),
            ("NumberPlane", PlotKind::NumberPlane),
            ("PlotCurve", PlotKind::Curve),
            ("VectorField", PlotKind::VectorField),
        ],
        "the plot-family catalog data drifted from the six plot primitives"
    );
    // `Graph` is a plot *host*, not a family member.
    assert!(
        CATALOG
            .iter()
            .find(|i| &*i.type_name == "Graph")
            .is_some_and(|i| i.plot.is_none()),
        "Graph hosts plot children and must not declare a PlotKind"
    );
}
