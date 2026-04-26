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
    match ty {
        "Text" => Some(Box::new(TextActorKind)),
        "Math" => Some(Box::new(MathActorKind)),
        "Code" => Some(Box::new(CodeActorKind)),
        "Svg" => Some(Box::new(SvgActorKind)),
        "Image" => Some(Box::new(ImageActorKind)),
        "Graph" | "CartesianPlot" | "PolarPlot" | "ParametricPlot" | "ImplicitPlot" => {
            Some(Box::new(PlotActorKind))
        }
        // Note: Shape types (Circle, Rect, etc.) and container types (Row, Col, Grid, Stack, Group)
        // are NOT handled via ActorKind dispatch - they fall through to the existing
        // inline processing in process_body to avoid infinite recursion.
        _ => None,
    }
}

// --- Built-in implementations ---

struct TextActorKind;
impl ActorKind for TextActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        timeline.process_text_actor_decl(
            ty,
            label,
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}

struct MathActorKind;
impl ActorKind for MathActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        timeline.process_text_actor_decl(
            ty,
            label,
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}

struct CodeActorKind;
impl ActorKind for CodeActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        timeline.process_text_actor_decl(
            ty,
            label,
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}

struct SvgActorKind;
impl ActorKind for SvgActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        timeline.process_media_actor_decl(
            ty,
            label,
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}

struct ImageActorKind;
impl ActorKind for ImageActorKind {
    fn build(
        &self,
        timeline: &mut Timeline,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        timeline.process_media_actor_decl(
            ty,
            label,
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}

/// Handles Graph, CartesianPlot, PolarPlot, ParametricPlot, ImplicitPlot
struct PlotActorKind;
impl ActorKind for PlotActorKind {
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
    ) {
        timeline.process_plot_actor_dispatch(
            label,
            ty,
            props,
            modifiers,
            children,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}
