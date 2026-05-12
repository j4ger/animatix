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
    // Shapes and containers are handled inline by process_body, not via ActorKind dispatch
    match primitive.category() {
        crate::timeline::ActorCategory::Shape | crate::timeline::ActorCategory::Container => None,
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
