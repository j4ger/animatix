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
    ActorCaps, ActorCategory, ChildProcessingKind, PrimitiveCapabilities, ShapeKind, TextKind,
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
    /// Renders as a stroke-based path (shapes and `PlotCurve`).
    pub stroke_path: bool,
}

const fn caps(
    text_paths: bool,
    vector_paths: bool,
    image_payload: bool,
    layout_container: bool,
    morphable_paths: bool,
    vector_reveal_target: bool,
    plot_geometry: bool,
    plot_host: bool,
    is_container: bool,
    is_shape: bool,
) -> PrimitiveCapabilities {
    PrimitiveCapabilities {
        text_paths,
        vector_paths,
        image_payload,
        layout_container,
        morphable_paths,
        vector_reveal_target,
        plot_geometry,
        plot_host,
        is_container,
        is_shape,
    }
}

const SHAPE_CAPS: PrimitiveCapabilities =
    caps(false, true, false, false, true, true, false, false, false, true);
const TEXT_CAPS: PrimitiveCapabilities =
    caps(true, false, false, false, true, true, false, false, false, false);
const PLOT_CAPS: PrimitiveCapabilities =
    caps(false, true, false, false, true, true, true, false, false, false);
const CONTAINER_CAPS: PrimitiveCapabilities =
    caps(false, false, false, true, false, false, false, false, true, false);
/// `Glass` is a container with a rectangular *region*, not a rectangular
/// surface: `layout_container` and the shape bits are what keep `size` and
/// `corner_radius` (the frosted clip) applicable, while `vector_paths` stays
/// false because the scope itself draws nothing — the card is its child.
const GLASS_CAPS: PrimitiveCapabilities =
    caps(false, false, false, true, false, false, false, false, true, true);
const GROUP_CAPS: PrimitiveCapabilities =
    caps(false, false, false, false, false, false, false, false, true, false);
const NO_CAPS: PrimitiveCapabilities =
    caps(false, false, false, false, false, false, false, false, false, false);
const IMAGE_CAPS: PrimitiveCapabilities =
    caps(false, false, true, false, false, false, false, false, false, false);
const SVG_CAPS: PrimitiveCapabilities =
    caps(false, true, false, false, true, true, false, false, false, false);
const GRAPH_CAPS: PrimitiveCapabilities =
    caps(false, true, false, false, true, true, true, true, false, false);
const ANNOTATION_CAPS: PrimitiveCapabilities =
    caps(false, true, false, false, false, false, false, false, false, false);

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
            stroke_path: false,
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
        Self {
            type_name: Cow::Borrowed(type_name),
            display_name: Cow::Borrowed(display_name),
            category: ActorCategory::Shape,
            icon_id: Cow::Borrowed(icon),
            advanced: false,
            capabilities: SHAPE_CAPS,
            child_processing: ChildProcessingKind::Generic,
            shape: Some(kind),
            text: None,
            stroke_path: true,
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
            stroke_path: false,
        }
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
    PrimitiveInfo::shape("Rect", "Rectangle", icon_glyphs::SQUARE, ShapeKind::Rect);
pub static ELLIPSE: PrimitiveInfo =
    PrimitiveInfo::shape("Ellipse", "Ellipse", icon_glyphs::CIRCLE_NOTCH, ShapeKind::Ellipse);
pub static LINE: PrimitiveInfo =
    PrimitiveInfo::shape("Line", "Line", icon_glyphs::MINUS, ShapeKind::Line);
pub static ARROW: PrimitiveInfo =
    PrimitiveInfo::shape("Arrow", "Arrow", icon_glyphs::ARROW_RIGHT, ShapeKind::Arrow);
pub static POLYGON: PrimitiveInfo =
    PrimitiveInfo::shape("Polygon", "Polygon", icon_glyphs::POLYGON, ShapeKind::Polygon);
pub static PATH: PrimitiveInfo =
    PrimitiveInfo::shape("Path", "Path", icon_glyphs::PEN, ShapeKind::Path);

// Text
pub static TEXT: PrimitiveInfo =
    PrimitiveInfo::text("Text", "Text", icon_glyphs::TEXT_T, TextKind::Text);
pub static CODE: PrimitiveInfo =
    PrimitiveInfo::text("Code", "Code", icon_glyphs::CODE, TextKind::Code).advanced();
pub static MATH: PrimitiveInfo =
    PrimitiveInfo::text("Math", "Math", icon_glyphs::FUNCTION, TextKind::Math).advanced();
pub static TYPST: PrimitiveInfo =
    PrimitiveInfo::text("Typst", "Typst", icon_glyphs::ARTICLE, TextKind::Typst).advanced();

// Media
pub static IMAGE: PrimitiveInfo =
    PrimitiveInfo::new("Image", "Image", icon_glyphs::IMAGE, ActorCategory::Media, IMAGE_CAPS);
pub static SVG: PrimitiveInfo =
    PrimitiveInfo::new("Svg", "SVG", icon_glyphs::VECTOR_THREE, ActorCategory::Media, SVG_CAPS)
        .advanced();
pub static AUDIO: PrimitiveInfo =
    PrimitiveInfo::new("Audio", "Audio", icon_glyphs::SPEAKER_HIGH, ActorCategory::Media, NO_CAPS)
        .advanced();

// Plots
pub static GRAPH: PrimitiveInfo =
    PrimitiveInfo::new("Graph", "Graph", icon_glyphs::CHART_BAR, ActorCategory::Plot, GRAPH_CAPS);
pub static PLOT_CURVE: PrimitiveInfo = PrimitiveInfo::new(
    "PlotCurve",
    "Plot Curve",
    icon_glyphs::CHART_LINE_UP,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.advanced()
.stroked();
pub static VECTOR_FIELD: PrimitiveInfo = PrimitiveInfo::new(
    "VectorField",
    "Vector Field",
    icon_glyphs::ARROWS_OUT_CARDINAL,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.advanced();
pub static HEATMAP: PrimitiveInfo =
    PrimitiveInfo::new("Heatmap", "Heatmap", icon_glyphs::GRADIENT, ActorCategory::Plot, PLOT_CAPS)
        .advanced();
pub static CONTOUR_SET: PrimitiveInfo = PrimitiveInfo::new(
    "ContourSet",
    "Contour Set",
    icon_glyphs::CHART_DONUT,
    ActorCategory::Plot,
    PLOT_CAPS,
)
.advanced();
pub static NUMBER_PLANE: PrimitiveInfo = PrimitiveInfo::new(
    "NumberPlane",
    "Number Plane",
    icon_glyphs::SQUARES_FOUR,
    ActorCategory::Plot,
    PLOT_CAPS,
);
pub static BAR_CHART: PrimitiveInfo = PrimitiveInfo::new(
    "BarChart",
    "Bar Chart",
    icon_glyphs::CHART_BAR,
    ActorCategory::Plot,
    PLOT_CAPS,
);

// Containers
pub static ROW: PrimitiveInfo =
    PrimitiveInfo::new("Row", "Row", icon_glyphs::ROWS, ActorCategory::Container, CONTAINER_CAPS);
pub static COL: PrimitiveInfo = PrimitiveInfo::new(
    "Col",
    "Column",
    icon_glyphs::COLUMNS,
    ActorCategory::Container,
    CONTAINER_CAPS,
);
pub static GRID: PrimitiveInfo = PrimitiveInfo::new(
    "Grid",
    "Grid",
    icon_glyphs::SQUARES_FOUR,
    ActorCategory::Container,
    CONTAINER_CAPS,
);
pub static STACK: PrimitiveInfo = PrimitiveInfo::new(
    "Stack",
    "Stack",
    icon_glyphs::STACK,
    ActorCategory::Container,
    CONTAINER_CAPS,
);
pub static GROUP: PrimitiveInfo =
    PrimitiveInfo::new("Group", "Group", icon_glyphs::FOLDER, ActorCategory::Container, GROUP_CAPS);
pub static MASK: PrimitiveInfo = PrimitiveInfo::new(
    "Mask",
    "Mask",
    icon_glyphs::MASK_HAPPY,
    ActorCategory::Container,
    GROUP_CAPS,
)
.advanced()
.with_child_processing(ChildProcessingKind::Mask);
pub static FILTER: PrimitiveInfo = PrimitiveInfo::new(
    "Filter",
    "Filter",
    icon_glyphs::FILTERS,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_child_processing(ChildProcessingKind::Filter);
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
.with_shape(ShapeKind::Rect)
.with_child_processing(ChildProcessingKind::Glass);
pub static EQUATION: PrimitiveInfo = PrimitiveInfo::new(
    "Equation",
    "Equation",
    icon_glyphs::SIGMA,
    ActorCategory::Container,
    CONTAINER_CAPS,
)
.with_child_processing(ChildProcessingKind::Equation);

// Equation fragment sub-item (text-like caps, but no single TextKind engine)
pub static FRAGMENT: PrimitiveInfo = PrimitiveInfo::new(
    "Fragment",
    "Fragment",
    icon_glyphs::HIGHLIGHTER,
    ActorCategory::Text,
    TEXT_CAPS,
);

// Annotations
pub static CALLOUT: PrimitiveInfo = PrimitiveInfo::new(
    "Callout",
    "Callout",
    icon_glyphs::TEXT_T,
    ActorCategory::Annotation,
    ANNOTATION_CAPS,
);
pub static LEGEND: PrimitiveInfo = PrimitiveInfo::new(
    "Legend",
    "Legend",
    icon_glyphs::CHART_LINE_UP,
    ActorCategory::Annotation,
    ANNOTATION_CAPS,
);

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
    // Annotations
    &CALLOUT,
    &LEGEND,
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
            stroke_path: false,
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
    ActorCaps {
        category: info.category,
        child_processing: info.child_processing,
        shape: info.shape,
        text: info.text,
        is_shape: info.capabilities.is_shape,
        stroke_path: info.stroke_path,
        text_paths: info.capabilities.text_paths,
        vector_paths: info.capabilities.vector_paths,
        image_payload: info.capabilities.image_payload,
        layout_container: info.capabilities.layout_container,
        is_container: info.capabilities.is_container,
        plot_geometry: info.capabilities.plot_geometry,
        plot_host: info.capabilities.plot_host,
        morphable_paths: info.capabilities.morphable_paths,
        vector_reveal_target: info.capabilities.vector_reveal_target,
        group_like: info.capabilities.is_container
            && !info.capabilities.layout_container
            && info.child_processing == ChildProcessingKind::Generic,
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
    use super::*;
    use std::collections::HashSet;

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
