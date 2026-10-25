use super::*;

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
    PrimitiveInfo::text("Counter", "Counter", icon_glyphs::HASH, TextKind::Counter)
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
pub static AUDIO: PrimitiveInfo = PrimitiveInfo::new(
    "Audio",
    "Audio",
    icon_glyphs::SPEAKER_HIGH,
    ActorCategory::Media,
    AUDIO_CAPS,
)
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
