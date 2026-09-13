//! The built-in primitive catalog: one [`PrimitiveInfo`] identity card per
//! built-in actor type.
//!
//! This is the single author-visible source for primitive metadata — display
//! name, category, icon, capabilities, child processing, and the shape/text
//! projections. The engine's behaviour implementations are keyed by
//! `type_name` and pinned to this catalog by tests; the parser crate derives
//! its contract tables from it. Adding a built-in primitive is one behaviour
//! file in the engine plus one row here plus one `PRIMITIVES` registration
//! line — no parallel declarations anywhere else.

use std::borrow::Cow;

use animatix_core::caps::{
    ActorCaps, ActorCategory, ChildProcessingKind, PrimitiveCapabilities, ShapeKind, TextKind,
};

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
    /// Opaque icon identifier (a [`crate::icon_glyphs`] constant).
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
const GROUP_CAPS: PrimitiveCapabilities =
    caps(false, false, false, false, false, false, false, false, true, false);
const NO_CAPS: PrimitiveCapabilities =
    caps(false, false, false, false, false, false, false, false, false, false);

/// The built-in primitive catalog, in registration order.
pub static CATALOG: &[PrimitiveInfo] = &[
    // Shapes
    PrimitiveInfo {
        type_name: Cow::Borrowed("Rect"),
        display_name: Cow::Borrowed("Rectangle"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::SQUARE),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Rect),
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Ellipse"),
        display_name: Cow::Borrowed("Ellipse"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CIRCLE_NOTCH),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Ellipse),
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Line"),
        display_name: Cow::Borrowed("Line"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::MINUS),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Line),
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Arrow"),
        display_name: Cow::Borrowed("Arrow"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::ARROW_RIGHT),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Arrow),
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Polygon"),
        display_name: Cow::Borrowed("Polygon"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::POLYGON),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Polygon),
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Path"),
        display_name: Cow::Borrowed("Path"),
        category: ActorCategory::Shape,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::PEN),
        advanced: false,
        capabilities: SHAPE_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: Some(ShapeKind::Path),
        text: None,
        stroke_path: true,
    },
    // Text
    PrimitiveInfo {
        type_name: Cow::Borrowed("Text"),
        display_name: Cow::Borrowed("Text"),
        category: ActorCategory::Text,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::TEXT_T),
        advanced: false,
        capabilities: TEXT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: Some(TextKind::Text),
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Code"),
        display_name: Cow::Borrowed("Code"),
        category: ActorCategory::Text,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CODE),
        advanced: true,
        capabilities: TEXT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: Some(TextKind::Code),
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Math"),
        display_name: Cow::Borrowed("Math"),
        category: ActorCategory::Text,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::FUNCTION),
        advanced: true,
        capabilities: TEXT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: Some(TextKind::Math),
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Typst"),
        display_name: Cow::Borrowed("Typst"),
        category: ActorCategory::Text,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::ARTICLE),
        advanced: true,
        capabilities: TEXT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: Some(TextKind::Typst),
        stroke_path: false,
    },
    // Media
    PrimitiveInfo {
        type_name: Cow::Borrowed("Image"),
        display_name: Cow::Borrowed("Image"),
        category: ActorCategory::Media,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::IMAGE),
        advanced: false,
        capabilities: caps(false, false, true, false, false, false, false, false, false, false),
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Svg"),
        display_name: Cow::Borrowed("SVG"),
        category: ActorCategory::Media,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::VECTOR_THREE),
        advanced: true,
        capabilities: caps(false, true, false, false, true, true, false, false, false, false),
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Audio"),
        display_name: Cow::Borrowed("Audio"),
        category: ActorCategory::Media,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::SPEAKER_HIGH),
        advanced: true,
        capabilities: NO_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    // Plots
    PrimitiveInfo {
        type_name: Cow::Borrowed("Graph"),
        display_name: Cow::Borrowed("Graph"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CHART_BAR),
        advanced: false,
        capabilities: caps(false, true, false, false, true, true, true, true, false, false),
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("PlotCurve"),
        display_name: Cow::Borrowed("Plot Curve"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CHART_LINE_UP),
        advanced: true,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: true,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("VectorField"),
        display_name: Cow::Borrowed("Vector Field"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::ARROWS_OUT_CARDINAL),
        advanced: true,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Heatmap"),
        display_name: Cow::Borrowed("Heatmap"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::GRADIENT),
        advanced: true,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("ContourSet"),
        display_name: Cow::Borrowed("Contour Set"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CHART_DONUT),
        advanced: true,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("NumberPlane"),
        display_name: Cow::Borrowed("Number Plane"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::SQUARES_FOUR),
        advanced: false,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("BarChart"),
        display_name: Cow::Borrowed("Bar Chart"),
        category: ActorCategory::Plot,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CHART_BAR),
        advanced: false,
        capabilities: PLOT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    // Containers
    PrimitiveInfo {
        type_name: Cow::Borrowed("Row"),
        display_name: Cow::Borrowed("Row"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::ROWS),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Col"),
        display_name: Cow::Borrowed("Column"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::COLUMNS),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Grid"),
        display_name: Cow::Borrowed("Grid"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::SQUARES_FOUR),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Stack"),
        display_name: Cow::Borrowed("Stack"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::STACK),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Group"),
        display_name: Cow::Borrowed("Group"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::FOLDER),
        advanced: false,
        capabilities: GROUP_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Mask"),
        display_name: Cow::Borrowed("Mask"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::MASK_HAPPY),
        advanced: true,
        capabilities: GROUP_CAPS,
        child_processing: ChildProcessingKind::Mask,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Filter"),
        display_name: Cow::Borrowed("Filter"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::FILTERS),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Filter,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Equation"),
        display_name: Cow::Borrowed("Equation"),
        category: ActorCategory::Container,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::SIGMA),
        advanced: false,
        capabilities: CONTAINER_CAPS,
        child_processing: ChildProcessingKind::Equation,
        shape: None,
        text: None,
        stroke_path: false,
    },
    // Equation fragment sub-item
    PrimitiveInfo {
        type_name: Cow::Borrowed("Fragment"),
        display_name: Cow::Borrowed("Fragment"),
        category: ActorCategory::Text,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::HIGHLIGHTER),
        advanced: false,
        capabilities: TEXT_CAPS,
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    // Annotations
    PrimitiveInfo {
        type_name: Cow::Borrowed("Callout"),
        display_name: Cow::Borrowed("Callout"),
        category: ActorCategory::Annotation,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::TEXT_T),
        advanced: false,
        capabilities: caps(false, true, false, false, false, false, false, false, false, false),
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
    PrimitiveInfo {
        type_name: Cow::Borrowed("Legend"),
        display_name: Cow::Borrowed("Legend"),
        category: ActorCategory::Annotation,
        icon_id: Cow::Borrowed(animatix_core::icon_glyphs::CHART_LINE_UP),
        advanced: false,
        capabilities: caps(false, true, false, false, false, false, false, false, false, false),
        child_processing: ChildProcessingKind::Generic,
        shape: None,
        text: None,
        stroke_path: false,
    },
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
    CATALOG.iter().find(|info| info.type_name.as_ref() == name)
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
