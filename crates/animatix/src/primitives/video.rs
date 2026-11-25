//! Video actor identity and primitive adapter.

use super::{EvaluateCtx, Primitive, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;

pub(super) struct VideoPrimitive;
pub(super) static VIDEO: VideoPrimitive = VideoPrimitive;

impl Primitive for VideoPrimitive {
    fn type_name(&self) -> &str {
        "Video"
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx<'_>,
        text_ctx: Option<&mut TextCompileCtx<'_>>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        crate::timeline::video::evaluate_builtin(ctx, text_ctx)
    }
}
