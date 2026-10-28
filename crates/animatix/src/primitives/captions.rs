//! Captions container primitive.
//!
//! `Captions` is a container primitive that aggregates speech-synced subtitle
//! cues (`Cue` actors or loaded from an external transcript file) with karaoke,
//! word-pop, and kinetic line styling.
//!
//! The actual rendering is performed in `scene_eval.rs` (`render_captions_children`),
//! reached through `render_children_by_id()`.

use crate::ast::{Expr, Property};
use crate::primitives::{Primitive, RenderChildrenCtx, RenderCommand};
use crate::timeline::SceneDimensions;

/// Opacity multiplier for the `transition_time` caption fade-in.
///
/// `1.0` once the cue is `transition_s` past its own start (and always `1.0`
/// when no transition is configured, which is the binding default). Kept
/// separate from the render path so the ramp itself is unit-testable.
pub(crate) fn cue_transition_alpha(time_s: f64, start_s: f64, transition_s: f64) -> f32 {
    if transition_s <= 0.0 {
        return 1.0;
    }
    ((time_s - start_s) / transition_s).clamp(0.0, 1.0) as f32
}

/// The `Captions` primitive.
pub struct CaptionsPrimitive;

/// Singleton instance of `CaptionsPrimitive`.
pub const CAPTIONS: CaptionsPrimitive = CaptionsPrimitive;

impl Primitive for CaptionsPrimitive {
    fn type_name(&self) -> &str {
        "Captions"
    }

    fn render_children_by_id(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        _children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), crate::renderer::error::RenderError> {
        ctx.timeline.render_captions_children_ctx(ctx);
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Captions rendering is handled by `Timeline::render_captions_children_ctx`
        // via `render_children_by_id()`. Return empty commands so hit regions are registered.
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

#[cfg(test)]
mod tests {
    use super::cue_transition_alpha;

    #[test]
    fn transition_alpha_is_one_without_a_transition() {
        assert_eq!(cue_transition_alpha(5.0, 1.0, 0.0), 1.0);
        assert_eq!(cue_transition_alpha(5.0, 1.0, -1.0), 1.0);
    }

    #[test]
    fn transition_alpha_ramps_over_the_window() {
        // 200 ms window starting at 1 s.
        assert_eq!(cue_transition_alpha(1.0, 1.0, 0.2), 0.0);
        assert_eq!(cue_transition_alpha(1.1, 1.0, 0.2), 0.5);
        assert_eq!(cue_transition_alpha(1.2, 1.0, 0.2), 1.0);
        // Before the cue, and after the window, it clamps.
        assert_eq!(cue_transition_alpha(0.5, 1.0, 0.2), 0.0);
        assert_eq!(cue_transition_alpha(9.0, 1.0, 0.2), 1.0);
    }
}
