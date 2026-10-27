//! Captions family evaluation system (Captions, Cue).

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::{Timeline, TrackAccessor};

/// Evaluate draw commands for a `Captions` container actor.
///
/// Returns an empty command list (`Some(vec![])`) so hit regions are registered
/// for the container while active cues are rendered via caption layout.
pub fn evaluate_captions(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(Some(vec![]))
}

/// Evaluate draw commands for a `Cue` actor.
///
/// Returns `None` as cues do not render independently; the parent `Captions`
/// container aggregates active cues.
pub fn evaluate_cue(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(None)
}

/// Custom build logic for `Cue` actors.
pub fn build_extra_cue(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    _modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    let env = timeline.env().clone();
    timeline.add_node(label.to_string(), parent_label);
    let track_id = timeline.world.spawn(label, "Cue");
    timeline.world.set_first_seen(track_id, time_ms as u64);
    timeline.world.ensure_text(track_id);

    for prop in props {
        if prop.name == "text" {
            let content = match &prop.value {
                Expr::Str(s) => s.clone(),
                other => crate::timeline::evaluate_expr(other, &env)
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default(),
            };
            timeline
                .world
                .ensure_text(track_id)
                .text_content
                .ensure(String::new())
                .add_keyframe(time_ms as u64, content, crate::easing::Easing::Linear);
        }
    }

    let has_start = props.iter().any(|p| p.name == "start");
    let has_end = props.iter().any(|p| p.name == "end");
    if !has_start && !has_end {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::NeverRevealed,
            DiagnosticPhase::Build,
            format!(
                "Cue `{label}` declares no `start`/`end`, which default to 0, so its time \
                 window is empty and it is never shown. Give it a window, e.g. \
                 `{label}: Cue, text: \"…\", start: 0.0, end: 2.0`."
            ),
        ));
    }

    Ok(())
}
