use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::timeline::shapes::ShapeType;
use crate::timeline::Timeline;

/// Trait for actor type dispatch. Each primitive type implements this trait
/// to provide its build logic.
pub trait ActorKind {
    /// Build the actor into the timeline. Called during `Timeline::build()`.
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    );
}

/// Look up an actor kind by name. Returns None if no handler is registered.
pub fn find_actor_kind(ty: &str) -> Option<Box<dyn ActorKind + Send + Sync>> {
    let primitive = crate::primitives::find_primitive(ty)?;
    // Shapes and containers are handled inline by process_body, not via ActorKind dispatch
    match primitive.category() {
        ActorCategory::Shape | ActorCategory::Container => None,
        _ => Some(Box::new(PrimitiveActorKind(primitive)) as Box<dyn ActorKind + Send + Sync>),
    }
}

struct PrimitiveActorKind(&'static dyn crate::primitives::Primitive);

impl ActorKind for PrimitiveActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        _ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut ctx = crate::primitives::BuildCtx {
            timeline,
            time_ms,
            parent_label,
            diagnostics,
        };
        if let Err(mut diags) = self.0.build(&mut ctx, label, props, modifiers, children) {
            diagnostics.append(&mut diags);
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Actor kind identification
// ─────────────────────────────────────────────────────────────

/// Stable, compile-time constant identifying an actor's type.
/// Set once at first declaration and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorKindId {
    /// Geometric shape (rect, ellipse, line, polygon, path).
    Shape(ShapeKind),
    /// Plain text actor.
    Text,
    /// Code block actor.
    Code,
    /// Typst document actor.
    Typst,
    /// Raster image actor.
    Image,
    /// SVG graphic actor.
    Svg,
    /// Graph / chart actor.
    Graph,
    /// Single curve plot actor.
    PlotCurve,
    /// Vector field visualization actor.
    VectorField,
    /// Heatmap visualization actor.
    Heatmap,
    /// Contour set visualization actor.
    ContourSet,
    /// Number plane / coordinate grid actor.
    NumberPlane,
    /// Bar chart / column chart actor.
    BarChart,
    /// Horizontal row layout container.
    Row,
    /// Vertical column layout container.
    Col,
    /// Grid layout container.
    Grid,
    /// Stack layout container.
    Stack,
    /// Generic group container.
    Group,
    /// Mask / clip container.
    Mask,
    /// Filter / post-processing container.
    Filter,
    /// Audio track actor.
    Audio,
    /// Equation container (Typst math with fragment highlighting).
    Equation,
    /// Fragment sub-item within an Equation.
    Fragment,
    /// Callout / annotation bubble.
    Callout,
    /// Legend with auto-generated color swatches and labels.
    Legend,
}

impl ActorKindId {
    /// Parse an actor kind from its type name (e.g. `"rect"`, `"text"`).
    pub fn from_type_name(ty: &str) -> Option<Self> {
        crate::primitives::find_primitive(ty).map(|p| p.kind_id())
    }
}

/// Specific shape geometry variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

impl From<ShapeType> for ShapeKind {
    fn from(st: ShapeType) -> Self {
        match st {
            ShapeType::Rect => Self::Rect,
            ShapeType::Ellipse => Self::Ellipse,
            ShapeType::Line => Self::Line,
            ShapeType::Polygon => Self::Polygon,
            ShapeType::Path => Self::Path,
            ShapeType::Graph => Self::Rect,
            ShapeType::Plot => Self::Rect,
            ShapeType::Arrow => Self::Arrow,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Actor kind metadata registry
// ─────────────────────────────────────────────────────────────

/// High-level category for grouping actor kinds in UI palettes and docs.
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

pub use crate::primitives::ActorKindMeta;

/// Global registry of all supported actor kinds.
pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    crate::primitives::actor_kind_registry()
}

/// Lookup metadata for a specific [`ActorKindId`].
pub fn actor_kind_meta(kind: ActorKindId) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta(kind)
}

/// Lookup metadata by the actor's type name (e.g. `"rect"`, `"text"`).
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta_by_name(name)
}
