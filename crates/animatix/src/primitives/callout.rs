use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::{AnimationTrack, SceneDimensions};

/// The singleton primitive descriptor for `Callout` actors.
pub struct CalloutPrimitive;
/// Singleton instance of the callout primitive descriptor.
pub const CALLOUT: CalloutPrimitive = CalloutPrimitive;

impl Primitive for CalloutPrimitive {
    fn type_name(&self) -> &'static str {
        "Callout"
    }
    fn display_name(&self) -> &'static str {
        "Callout"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Annotation
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::TEXT_T
    }
    fn is_advanced(&self) -> bool {
        false
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Callout
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        // Ensure track exists and set kind
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = ActorKindId::Callout;

        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = ctx.time_ms as u64;
        }

        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        // Placeholder: callout rendering will be added in a follow-up
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("label", Expr::Str("Callout".to_string())),
            Property::new("label_at", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
        ]
    }
}
