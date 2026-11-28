//! Emitter actor identity and primitive adapter.

use super::{EvaluateCtx, Primitive, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;

pub(super) struct EmitterPrimitive;
pub(super) static EMITTER: EmitterPrimitive = EmitterPrimitive;

impl Primitive for EmitterPrimitive {
    fn type_name(&self) -> &str {
        "Emitter"
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx<'_>,
        text_ctx: Option<&mut TextCompileCtx<'_>>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        crate::timeline::particles::evaluate(ctx, text_ctx)
    }
}
