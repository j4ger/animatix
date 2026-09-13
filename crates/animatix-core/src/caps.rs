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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorCategory {
    /// Geometric shapes (rect, ellipse, etc.).
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
