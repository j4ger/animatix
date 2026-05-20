use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::{Environment, SceneDimensions, Value};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;

pub struct AudioPrimitive;
pub const AUDIO: AudioPrimitive = AudioPrimitive;

impl Primitive for AudioPrimitive {
    fn type_name(&self) -> &'static str { "Audio" }
    fn display_name(&self) -> &'static str { "Audio" }
    fn category(&self) -> ActorCategory { ActorCategory::Media }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::SPEAKER_HIGH }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Audio }

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

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("source", Expr::Str(String::new())),
            Property::new("volume", Expr::Num(1.0)),
        ]
    }
}