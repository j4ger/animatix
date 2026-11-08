//! Cue leaf primitive for subtitles inside a Captions container.

use crate::ast::{Expr, Property};
use crate::primitives::{EvaluateCtx, Primitive, RenderCommand};
use crate::timeline::SceneDimensions;

/// The `Cue` primitive.
pub struct CuePrimitive;

/// Singleton instance of `CuePrimitive`.
pub const CUE: CuePrimitive = CuePrimitive;

impl Primitive for CuePrimitive {
    fn type_name(&self) -> &str {
        "Cue"
    }

    fn evaluate(
        &self,
        _ctx: &EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Cue does not render independently; parent Captions aggregates all cues.
        Ok(None)
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![Property::new("text", Expr::Str(String::new()))]
    }
}
