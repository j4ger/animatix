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
    /// Emits plot geometry.
    pub plot_geometry: bool,
    /// Hosts plot-curve children in a math coordinate system.
    pub plot_host: bool,
    /// Is a container primitive.
    pub is_container: bool,
    /// Is a vector shape.
    pub is_shape: bool,
    /// Directly emits visual ink or content (stroke, fill, glyphs, raster pixels, or plot marks).
    pub has_visual_content: bool,
    /// Primarily drawn via stroke rather than fill (e.g. Line, Arrow, Callout, PlotCurve,
    /// VectorField, ContourSet).
    pub stroke_primary: bool,
    /// Has callout-style targeted leader line and label geometry.
    pub callout: bool,
    /// Manages and displays a series legend.
    pub legend_host: bool,
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
    /// Blur what the render target already holds behind the scope, then render
    /// the scope's own surface and everything after it above that blur. The
    /// `Glass` container; see `docs/spec.md` ("Glass").
    Glass,
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

impl ShapeKind {
    /// True when the primitive paints from `stroke_color` alone, so an
    /// authored `color:` has nothing else it could mean and the build/assign
    /// paths inherit one into the other.
    ///
    /// `Arrow` belongs here despite the filled head: `primitives/arrow.rs`
    /// builds both the head fill and the shaft stroke from `stroke_color`, so
    /// before this predicate existed an authored `color:` on an Arrow built
    /// cleanly, rendered grey, and warned about nothing.
    pub fn is_stroke_only(self) -> bool {
        matches!(self, Self::Line | Self::Arrow)
    }
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
    /// Directly renders visual stroke, fill, glyphs, raster pixels, or plot marks.
    pub has_visual_content: bool,
    /// Primarily drawn via stroke rather than fill (e.g. Line, Arrow, Callout, PlotCurve,
    /// VectorField, ContourSet).
    pub stroke_primary: bool,
    /// Has callout-style targeted leader line and label geometry.
    pub callout: bool,
    /// Manages and displays a series legend.
    pub legend_host: bool,
}

impl ActorCaps {
    /// `true` when this actor renders its children to an offscreen texture and
    /// applies an effect chain (a `Filter` compositing scope).
    pub fn is_effect_scope(&self) -> bool {
        matches!(self.child_processing, ChildProcessingKind::Filter | ChildProcessingKind::Glass)
    }

    /// `true` when the GUI should offer to nest new actors inside this
    /// container: it is a container AND children render through the generic
    /// scene-graph recursion (not Mask/Filter/Equation aggregation).
    pub fn is_nestable_container(&self) -> bool {
        self.is_container && self.child_processing == ChildProcessingKind::Generic
    }

    /// Returns `true` if this actor is a plain structural group (no layout semantics).
    #[inline]
    pub fn is_group_like(&self) -> bool {
        self.is_container
            && !self.layout_container
            && self.child_processing == ChildProcessingKind::Generic
    }

    /// Returns `true` if this actor directly renders visual ink or content
    /// of its own (shapes, text, media graphics, plots, or annotations).
    pub fn renders_visual_content(&self) -> bool {
        self.has_visual_content
    }

    /// Default stroke width for this actor kind.
    ///
    /// Stroke-primary actors (Line, Arrow, Callout, PlotCurve, VectorField, ContourSet)
    /// need a visible outline (2.0) by default; filled shapes default to 0.0 to avoid
    /// edge artifacts.
    pub fn default_stroke_width(&self) -> f32 {
        if self.stroke_primary { 2.0 } else { 0.0 }
    }
}

/// Declares which actors a property applies to.
///
/// One predicate per property, evaluated against [`ActorCaps`]. Capability
/// -shaped variants read the projection fields; [`Applicable::Actors`] matches
/// authored type names for genuinely name-specific properties (`code` only on
/// `Code`, `density` only on `VectorField`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicable {
    /// Applies to every actor kind including Group.
    Everything,
    /// Applies to all shape kinds.
    AllShapes,
    /// All actors with stroke-based path rendering (shapes + `PlotCurve`).
    AllStrokePaths,
    /// Applies to all shapes except Line (fill-related properties).
    AllShapesExceptLine,
    /// Applies to shapes and text-like actors with fillable/colorable content.
    AllDrawables,
    /// Applies to shapes, image, plots, layout containers, and effect scopes
    /// (actors with meaningful bounds).
    SizedActors,
    /// Applies to specific shape kinds.
    ShapeKinds(&'static [ShapeKind]),
    /// Applies to the listed authored actor type names.
    Actors(&'static [&'static str]),
    /// Applies to text-engine actors (Text, Code, Typst, Math).
    TextLike,
    /// Applies to every actor except text-engine actors.
    ExceptTextLike,
    /// Applies to actors hosting time-varying plot geometry.
    PlotGeometry,
    /// Applies to the math coordinate host (Graph).
    PlotHost,
    /// Applies to series legend hosts.
    LegendHost,
    /// Applies to callouts with targeted leader line and label geometry.
    Callout,
    /// Applies to layout containers (Row, Col, Stack, Grid).
    LayoutContainers,
    /// Applies when any child applicability matches.
    Any(&'static [Applicable]),
    /// Never shown in the inspector (build-time only, aliases, compounds).
    Never,
}

impl Applicable {
    /// Returns `true` if this applicability includes the given actor.
    pub fn includes(self, caps: &ActorCaps, actor_type: &str) -> bool {
        match self {
            Applicable::Everything => true,
            Applicable::AllShapes => caps.shape.is_some(),
            Applicable::AllStrokePaths => caps.stroke_path,
            Applicable::AllShapesExceptLine => caps.shape.is_some_and(|sk| sk != ShapeKind::Line),
            Applicable::AllDrawables => caps.has_visual_content,
            Applicable::SizedActors => {
                caps.is_shape
                    || caps.image_payload
                    || caps.plot_geometry
                    || (caps.layout_container
                        && caps.child_processing == ChildProcessingKind::Generic)
                    || caps.is_effect_scope()
            },
            Applicable::ShapeKinds(kinds) => caps.shape.is_some_and(|sk| kinds.contains(&sk)),
            Applicable::Actors(actors) => actors.contains(&actor_type),
            Applicable::TextLike => caps.text.is_some(),
            Applicable::ExceptTextLike => caps.text.is_none(),
            Applicable::PlotGeometry => caps.plot_geometry,
            Applicable::PlotHost => caps.plot_host,
            Applicable::LegendHost => caps.legend_host,
            Applicable::Callout => caps.callout,
            Applicable::LayoutContainers => {
                caps.layout_container && caps.child_processing == ChildProcessingKind::Generic
            },
            Applicable::Any(children) => {
                children.iter().any(|child| child.includes(caps, actor_type))
            },
            Applicable::Never => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_visual_content_classification() {
        let visual = ActorCaps {
            has_visual_content: true,
            ..Default::default()
        };
        assert!(visual.renders_visual_content());

        let non_visual = ActorCaps {
            has_visual_content: false,
            ..Default::default()
        };
        assert!(!non_visual.renders_visual_content());
    }

    #[test]
    fn default_stroke_width_classification() {
        let stroke_primary = ActorCaps {
            stroke_primary: true,
            ..Default::default()
        };
        assert_eq!(stroke_primary.default_stroke_width(), 2.0);

        let fill_primary = ActorCaps {
            stroke_primary: false,
            ..Default::default()
        };
        assert_eq!(fill_primary.default_stroke_width(), 0.0);
    }

    #[test]
    fn group_like_classification() {
        let group = ActorCaps {
            is_container: true,
            layout_container: false,
            child_processing: ChildProcessingKind::Generic,
            ..Default::default()
        };
        assert!(group.is_group_like());

        let layout = ActorCaps {
            is_container: true,
            layout_container: true,
            child_processing: ChildProcessingKind::Generic,
            ..Default::default()
        };
        assert!(!layout.is_group_like());

        let filter = ActorCaps {
            is_container: true,
            layout_container: false,
            child_processing: ChildProcessingKind::Filter,
            ..Default::default()
        };
        assert!(!filter.is_group_like());
    }

    #[test]
    fn applicable_capability_variants() {
        let plot_host = ActorCaps {
            plot_host: true,
            ..Default::default()
        };
        assert!(Applicable::PlotHost.includes(&plot_host, "Graph"));
        assert!(!Applicable::PlotHost.includes(&ActorCaps::default(), "Rect"));

        let legend = ActorCaps {
            legend_host: true,
            ..Default::default()
        };
        assert!(Applicable::LegendHost.includes(&legend, "Legend"));
        assert!(!Applicable::LegendHost.includes(&ActorCaps::default(), "Rect"));

        let callout = ActorCaps {
            callout: true,
            ..Default::default()
        };
        assert!(Applicable::Callout.includes(&callout, "Callout"));
        assert!(!Applicable::Callout.includes(&ActorCaps::default(), "Line"));

        let row = ActorCaps {
            layout_container: true,
            child_processing: ChildProcessingKind::Generic,
            ..Default::default()
        };
        assert!(Applicable::LayoutContainers.includes(&row, "Row"));

        let glass = ActorCaps {
            layout_container: true,
            child_processing: ChildProcessingKind::Glass,
            ..Default::default()
        };
        assert!(!Applicable::LayoutContainers.includes(&glass, "Glass"));
    }
}
