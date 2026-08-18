use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The singleton primitive descriptor for `Audio` actors.
pub struct AudioPrimitive;
/// Singleton instance of the audio primitive descriptor.
pub const AUDIO: AudioPrimitive = AudioPrimitive;

impl Primitive for AudioPrimitive {
    fn type_name(&self) -> &str {
        "Audio"
    }
    fn display_name(&self) -> &str {
        "Audio"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Media
    }
    fn icon_id(&self) -> &str {
        crate::icon_glyphs::SPEAKER_HIGH
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Audio
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        ctx.timeline.process_audio_actor_decl(
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("source", Expr::Str(String::new())),
            Property::new("volume", Expr::Num(1.0)),
        ]
    }
}
