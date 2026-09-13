//! The capability vocabulary shared by the built-in catalog, the engine, and
//! tooling.
//!
//! These enums are deliberately a small closed set: they grow only when the
//! engine learns a new *behaviour*, which is exactly when the compiler should
//! force every dispatch site to be revisited. Individual primitives are data
//! (catalog rows), never enum variants.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ChildProcessingKind {
    /// Render children through the normal scene graph recursion.
    #[default]
    Generic,
    /// Render children to an offscreen texture and apply an effect chain.
    Filter,
    /// Clip children to a mask shape.
    Mask,
    /// Aggregate typst fragments with highlight ranges.
    Equation,
}

/// Specific shape geometry variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ShapeKind {
    /// Axis-aligned rectangle.
    Rect,
    /// Ellipse (or circle).
    Ellipse,
    /// Straight line segment.
    Line,
    /// Closed polygon.
    Polygon,
    /// Arbitrary Bézier path.
    Path,
    /// Arrow with a dedicated arrowhead.
    Arrow,
}

/// The text engine backing a text-like actor.
///
/// Mirrors the renderer's compile-time text kinds (which are
/// `text`-feature-gated) so capabilities stay available in render-independent
/// builds; conversion to the renderer kind happens at the text compile sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TextKind {
    /// Plain styled text.
    Text,
    /// Typst markup.
    Typst,
    /// Typst math (`Math` primitive).
    Math,
    /// Code block.
    Code,
}

/// High-level category for grouping actors in UI palettes and docs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActorCategory {
    /// Geometric shapes (rect, ellipse, etc.).
    #[default]
    Shape,
    /// Text and typographic actors.
    Text,
    /// Image, SVG, and audio actors.
    Media,
    /// Plot and graph actors.
    Plot,
    /// Layout containers (row, column, grid, etc.).
    Container,
    /// Annotations and callouts.
    Annotation,
}

impl ActorCategory {
    /// Human-readable label for this category.
    pub const fn label(&self) -> &'static str {
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

/// What the engine needs to know about an actor's kind.
///
/// Derived once at identity time (from a catalog row for built-ins, from the
/// registration info for extensions), so frame-time checks are `Copy` field
/// reads and adding a primitive never requires extending an enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ActorCaps {
    /// UI category (Shapes, Text, Media, Plots, Containers, Annotations).
    pub category: ActorCategory,
    /// How children of this actor are processed
    /// (generic / mask / filter / equation).
    pub child_processing: ChildProcessingKind,
    /// The concrete shape geometry, for the six shape primitives.
    pub shape: Option<ShapeKind>,
    /// The text engine backing this actor, for text-like primitives.
    pub text: Option<TextKind>,
    /// Renders as a vector shape.
    pub is_shape: bool,
    /// Renders as a stroke-based path (shapes and `PlotCurve`).
    pub stroke_path: bool,
    /// Produces text paths (text, code, typst, math).
    pub text_paths: bool,
    /// Produces morphable vector paths.
    pub vector_paths: bool,
    /// Carries a raster image payload (`Image`).
    pub image_payload: bool,
    /// Participates in parent layout as a row/column/grid/stack.
    pub layout_container: bool,
    /// Is a container of children.
    pub is_container: bool,
    /// Hosts time-varying plot geometry.
    pub plot_geometry: bool,
    /// Is the `Graph` coordinate host.
    pub plot_host: bool,
    /// Can be a morph target between vector paths.
    pub morphable_paths: bool,
    /// Can be revealed by tracing vector paths.
    pub vector_reveal_target: bool,
    /// Is a plain structural group (no layout semantics).
    pub group_like: bool,
}

impl ActorCaps {
    /// `true` when this actor renders its children to an offscreen texture and
    /// applies an effect chain (a `Filter` compositing scope).
    pub fn is_effect_scope(&self) -> bool {
        self.child_processing == ChildProcessingKind::Filter
    }

    /// `true` when the GUI should offer to nest new actors inside this
    /// container: it is a container AND children render through the generic
    /// scene-graph recursion (not Mask/Filter/Equation aggregation).
    pub fn is_nestable_container(&self) -> bool {
        self.is_container && self.child_processing == ChildProcessingKind::Generic
    }
}
