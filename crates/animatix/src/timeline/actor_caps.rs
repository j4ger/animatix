//! Actor capabilities: the engine-visible projection of a primitive kind.
//!
//! Identity lives in `AnimationTrack::actor_type` — the authored type name.
//! [`ActorCaps`] is the matchable projection the engine dispatches on,
//! derived once at identity time from the primitive, so frame-time checks are
//! `Copy` field reads and adding a primitive never requires extending an
//! enum. Capability *vocabulary* (`ActorCategory`, `ShapeKind`,
//! `ChildProcessing`) stays a small closed set: it only grows when the engine
//! learns a new behaviour, which is exactly when the compiler should force
//! every dispatch site to be revisited.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::primitives::ChildProcessing;
use crate::timeline::shapes::ShapeType;

/// What the engine needs to know about an actor's kind.
///
/// Derived once at identity time via [`ActorCaps::of`]; unregistered
/// (extension) type names derive all-false caps until their primitive's build
/// path refines them from the registry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ActorCaps {
    /// How children of this actor are processed
    /// (generic / mask / filter / equation).
    pub child_processing: ChildProcessing,
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
    /// Derive the capability projection from a primitive.
    pub fn of(primitive: &dyn crate::primitives::Primitive) -> Self {
        let capabilities = primitive.capabilities();
        ActorCaps {
            child_processing: primitive.child_processing(),
            shape: primitive.shape_kind(),
            text: primitive.text_kind(),
            is_shape: capabilities.is_shape,
            stroke_path: primitive.has_stroke_path(),
            text_paths: capabilities.text_paths,
            vector_paths: capabilities.vector_paths,
            image_payload: capabilities.image_payload,
            layout_container: capabilities.layout_container,
            is_container: capabilities.is_container,
            plot_geometry: capabilities.plot_geometry,
            plot_host: capabilities.plot_host,
            morphable_paths: capabilities.morphable_paths,
            vector_reveal_target: capabilities.vector_reveal_target,
            group_like: primitive.is_group_like(),
        }
    }

    /// Derive caps for an authored type name from the built-in registry.
    /// Returns `None` for unregistered names — callers decide the fallback
    /// (the all-false [`ActorCaps::default()`] mirrors the old `Extension`
    /// behaviour).
    pub fn of_type(actor_type: &str) -> Option<Self> {
        crate::primitives::find_primitive(actor_type).map(Self::of)
    }

    /// `true` when this actor renders its children to an offscreen texture and
    /// applies an effect chain (a `Filter` compositing scope).
    pub fn is_effect_scope(&self) -> bool {
        self.child_processing == ChildProcessing::Filter
    }
}

/// The text engine backing a text-like actor.
///
/// Mirrors `renderer::text::TextKind` (which is `text`-feature-gated) so
/// capabilities stay available in render-independent builds; conversion to the
/// renderer kind happens at the text compile sites.
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

// ── Capability vocabulary ───────────────────────────────────────────────────

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
// Actor kind dispatch (legacy build path)
// ─────────────────────────────────────────────────────────────

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
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
    // Shapes, containers, and Callout are handled inline by process_body, not via ActorKind
    // dispatch. Callout is an annotation but its properties now use the generic build path.
    match primitive.category() {
        ActorCategory::Shape | ActorCategory::Container => None,
        _ if primitive.type_name() == "Callout" => None,
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

pub use crate::primitives::ActorKindMeta;

/// Global registry of all supported actor kinds.
pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    crate::primitives::actor_kind_registry()
}

/// Lookup metadata by the actor's type name (e.g. `"rect"`, `"text"`).
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta_by_name(name)
}
