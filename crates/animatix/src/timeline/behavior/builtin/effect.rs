//! Effect scope family evaluation system (Filter, Glass, Mask).

use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;

/// Evaluate draw commands for effect scope actors (Filter, Glass, Mask).
///
/// Returns an empty command list (`Some(vec![])`) so hit regions and bounds
/// are computed for the effect scope while children are rendered via the
/// compositing pipeline.
pub fn evaluate_effect(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(Some(vec![]))
}
