//! Actor capabilities: the engine-visible projection of a primitive kind.
//!
//! Identity lives in `AnimationTrack::actor_type` — the authored type name.
//! [`ActorCaps`] is the matchable projection the engine dispatches on,
//! derived once at identity time from the primitive, so frame-time checks are
//! `Copy` field reads and adding a primitive never requires extending an
//! enum. The capability *vocabulary* ([`ActorCategory`], [`ShapeKind`],
//! [`TextKind`], `ChildProcessing`) lives in `animatix-core::caps` and stays
//! a small closed set: it only grows when the engine learns a new behaviour,
//! which is exactly when the compiler should force every dispatch site to be
//! revisited.

pub use animatix_core::caps::{ActorCaps, ActorCategory, ShapeKind, TextKind};

use crate::timeline::shapes::ShapeType;

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
    let info = animatix_std::catalog_lookup(ty)
        .expect("find_actor_kind only dispatches catalog-registered built-ins");
    match info.category {
        ActorCategory::Shape | ActorCategory::Container => None,
        _ if info.capabilities.callout || ty == "Connector" => None,
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
