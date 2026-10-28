use crate::ast::{Expr, Property};
use crate::primitives::Primitive;
use crate::timeline::SceneDimensions;

/// The singleton primitive descriptor for `Audio` actors.
pub struct AudioPrimitive;
/// Singleton instance of the audio primitive descriptor.
pub const AUDIO: AudioPrimitive = AudioPrimitive;

impl Primitive for AudioPrimitive {
    fn type_name(&self) -> &str {
        "Audio"
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
