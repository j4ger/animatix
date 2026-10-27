//! Plot family evaluation and build systems (Graph, PlotCurve, VectorField, Heatmap, ContourSet, NumberPlane, BarChart).

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::path_progress::trim_path_by_progress;
use crate::timeline::{Timeline, TrackAccessor};

// ── Evaluate function ────────────────────────────────────────────────────────

/// Unified evaluation for all plot-family actors.
///
/// Evaluates plot vector paths and trims stroke by `stroke_progress`.
pub fn evaluate_plot(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    if ctx.vector_paths.is_empty() {
        return Ok(None);
    }

    let style = ctx.style;
    let progress = f64::from(style.stroke_progress.get(ctx.time_ms, 1.0)).clamp(0.0, 1.0);

    let paths: Vec<_> = if progress < 1.0 {
        ctx.vector_paths
            .iter()
            .map(|vp| {
                let mut trimmed = vp.clone();
                trimmed.path = std::sync::Arc::new(trim_path_by_progress(&vp.path, progress));
                if progress <= 0.0 {
                    trimmed.stroke = None;
                }
                trimmed
            })
            .collect()
    } else {
        ctx.vector_paths.to_vec()
    };

    let mut commands = vec![RenderCommand::Paths { paths }];

    let wants_dash = !style.dash_pattern.get(ctx.time_ms, Vec::new()).is_empty();
    let wants_gradient = style.fill_gradient.is_some() || style.stroke_gradient.is_some();
    if wants_dash || wants_gradient {
        crate::primitives::stamp_stroke_decoration(&mut commands, ctx);
    }

    Ok(Some(commands))
}

// ── Build extra functions ────────────────────────────────────────────────────

fn build_extra_plot_named(
    ty: &'static str,
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
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
    Ok(())
}

/// Custom build logic for `Graph` actors.
pub fn build_extra_graph(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "Graph",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `PlotCurve` actors.
pub fn build_extra_plot_curve(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "PlotCurve",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `VectorField` actors.
pub fn build_extra_vector_field(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "VectorField",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `Heatmap` actors.
pub fn build_extra_heatmap(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "Heatmap",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `ContourSet` actors.
pub fn build_extra_contour_set(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "ContourSet",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `NumberPlane` actors.
pub fn build_extra_number_plane(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "NumberPlane",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `BarChart` actors.
pub fn build_extra_bar_chart(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_plot_named(
        "BarChart",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}
