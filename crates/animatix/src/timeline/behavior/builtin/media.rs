//! Media family evaluation and build systems (Image, Svg, Audio).

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::{DEFAULT_LAYOUT_HALF_SIZE, Timeline, TrackAccessor};

// ── Evaluate functions ───────────────────────────────────────────────────────

/// Evaluate draw commands for an `Image` actor.
pub fn evaluate_image(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    if let Some(image) = ctx.image_track().and_then(|t| t.evaluate(ctx.time_ms)) {
        let half_size = crate::primitives::resolve_half_size(
            ctx.geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE),
            ctx.overrides,
        );
        let natural_size = [half_size[0] * 2.0, half_size[1] * 2.0];
        // Actor local space is centered on the position (every geometry
        // track stores half-sizes), so shift the box back by half its
        // display size to keep `at`/`anchor` meaning "the image is here",
        // not "the image's top-left is here".
        Ok(Some(vec![RenderCommand::Image {
            image,
            natural_size,
            offset: [(-half_size[0]) as f64, (-half_size[1]) as f64],
        }]))
    } else {
        Ok(None)
    }
}

/// Evaluate draw commands for an `Svg` actor.
pub fn evaluate_svg(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let Some(paths) = ctx.svg_paths_at(ctx.time_ms) else {
        return Ok(None);
    };
    if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(vec![RenderCommand::Paths { paths }]))
    }
}

/// Evaluate draw commands for an `Audio` actor.
pub fn evaluate_audio(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(None)
}

// ── Build extra functions ────────────────────────────────────────────────────

/// Custom build logic for `Image` actors.
pub fn build_extra_image(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    timeline.process_media_actor_decl(
        "Image",
        label,
        props,
        modifiers,
        time_ms,
        parent_label,
        diagnostics,
    );
    Ok(())
}

/// Custom build logic for `Svg` actors.
pub fn build_extra_svg(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    timeline.process_media_actor_decl(
        "Svg",
        label,
        props,
        modifiers,
        time_ms,
        parent_label,
        diagnostics,
    );
    Ok(())
}

/// Custom build logic for `Audio` actors.
pub fn build_extra_audio(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    timeline.process_audio_actor_decl(label, props, modifiers, time_ms, parent_label, diagnostics);
    Ok(())
}
