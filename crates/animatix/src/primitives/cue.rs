//! Cue leaf primitive for subtitles inside a Captions container.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, CaptionCue, EvaluateCtx, Primitive, RenderCommand};
use crate::timeline::SceneDimensions;
use crate::timeline::TrackAccessor;

/// The `Cue` primitive.
pub struct CuePrimitive;

/// Singleton instance of `CuePrimitive`.
pub const CUE: CuePrimitive = CuePrimitive;

impl Primitive for CuePrimitive {
    fn type_name(&self) -> &str {
        "Cue"
    }

    fn caption_cue(&self, ctx: &EvaluateCtx) -> Option<CaptionCue> {
        let text = ctx.track.text.text_content.get(ctx.time_ms, String::new());
        let start_s = ctx.track.sample_f32_prop("start", ctx.time_ms, 0.0);
        let end_s = ctx.track.sample_f32_prop("end", ctx.time_ms, 0.0);
        let start_ms = (start_s * 1000.0).round() as u64;
        let end_ms = (end_s * 1000.0).round() as u64;
        let active_color =
            ctx.track
                .sample_property_named("active_color", ctx.time_ms)
                .and_then(|pv| match pv {
                    crate::timeline::PropertyValue::Color(c)
                    | crate::timeline::PropertyValue::Vec4(c) => Some(c),
                    _ => None,
                });

        Some(CaptionCue {
            text,
            start_ms,
            end_ms,
            active_color,
        })
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        let env = ctx.timeline.env().clone();
        ctx.timeline.add_node(label.to_string(), ctx.parent_label);
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| crate::timeline::AnimationTrack::new(label.to_string(), "Cue"));
        track.set_identity("Cue");
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = ctx.time_ms as u64;
        }

        for prop in props {
            if prop.name == "text" {
                let content = match &prop.value {
                    Expr::Str(s) => s.clone(),
                    other => crate::timeline::evaluate_expr(other, &env)
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_default(),
                };
                track.text.text_content.ensure(String::new()).add_keyframe(
                    ctx.time_ms as u64,
                    content,
                    crate::easing::Easing::Linear,
                );
            }
        }

        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Cue does not render independently; parent Captions aggregates all cues.
        Ok(None)
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![Property::new("text", Expr::Str(String::new()))]
    }
}
