//! Captions container primitive.
//!
//! `Captions` is a container primitive that aggregates speech-synced subtitle
//! cues (`Cue` actors or loaded from an external transcript file) with karaoke,
//! word-pop, and kinetic line styling.
//!
//! The actual rendering is performed in `scene_eval.rs` (`render_captions_children`),
//! reached through `render_children()`.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderChildrenCtx, RenderCommand};
use crate::timeline::SceneDimensions;

/// The `Captions` primitive.
pub struct CaptionsPrimitive;

/// Singleton instance of `CaptionsPrimitive`.
pub const CAPTIONS: CaptionsPrimitive = CaptionsPrimitive;

impl Primitive for CaptionsPrimitive {
    fn type_name(&self) -> &str {
        "Captions"
    }

    fn render_children(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        _children: &[&str],
    ) -> Result<(), crate::renderer::error::RenderError> {
        ctx.timeline.render_captions_children_ctx(ctx);
        Ok(())
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.add_node(label.to_string(), ctx.parent_label);
        let track =
            ctx.timeline.tracks.entry(label.to_string()).or_insert_with(|| {
                crate::timeline::AnimationTrack::new(label.to_string(), "Captions")
            });
        track.set_identity("Captions");
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = ctx.time_ms as u64;
        }

        // Check for direct transcript property on Captions
        for prop in props {
            if prop.name == "transcript" {
                if let Expr::Str(ref path) = prop.value {
                    let normalized = ctx.timeline.asset_cache.normalize_asset_url(path);
                    if let Ok(content) = std::fs::read_to_string(&normalized) {
                        if let Ok(transcript) =
                            animatix_text::transcript::parse_transcript_auto(&content)
                        {
                            ctx.timeline.transcripts.insert(label.to_string(), transcript);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Captions rendering is handled by `Timeline::render_captions_children_ctx`
        // via `render_children()`. Return empty commands so hit regions are registered.
        Ok(Some(vec![]))
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "at",
                Expr::Tuple(vec![
                    Expr::Num(scene.width as f64 / 2.0),
                    Expr::Num(scene.height as f64 * 0.85),
                ]),
            ),
            Property::new("font_size", Expr::Num(32.0)),
        ]
    }
}
