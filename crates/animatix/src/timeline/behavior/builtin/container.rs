//! Container family evaluation system (Row, Col, Grid, Stack, Group).

use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;

/// Evaluate draw commands for container actors (Row, Col, Grid, Stack, Group).
///
/// Returns an empty command list (`Some(vec![])`) so hit regions and bounds
/// are computed for the container frame box without drawing direct content.
pub fn evaluate_container(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(Some(vec![]))
}
