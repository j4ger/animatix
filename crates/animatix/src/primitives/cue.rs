//! Cue leaf primitive for subtitles inside a Captions container.

use crate::ast::{Expr, Property};
use crate::primitives::{CaptionCue, EvaluateCtx, Primitive, RenderCommand};
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
        let text = ctx
            .text
            .map(|t| t.text_content.get(ctx.time_ms, String::new()))
            .unwrap_or_default();
        let start_s = ctx.sample_f32_prop("start", ctx.time_ms, 0.0);
        let end_s = ctx.sample_f32_prop("end", ctx.time_ms, 0.0);
        let start_ms = (start_s * 1000.0).round() as u64;
        let end_ms = (end_s * 1000.0).round() as u64;
        let active_color =
            ctx.sample_property_named("active_color", ctx.time_ms).and_then(|pv| match pv {
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
