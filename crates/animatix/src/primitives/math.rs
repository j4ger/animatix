//! Math formula primitive (first-class Typst math).

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, Primitive};
use crate::timeline::{Environment, SceneDimensions};

/// The `Math` primitive.
pub struct MathPrimitive;

/// Singleton instance of `MathPrimitive`.
pub const MATH: MathPrimitive = MathPrimitive;

impl Primitive for MathPrimitive {
    fn type_name(&self) -> &str {
        "Math"
    }

    fn handle_assignment(
        &self,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        super::text::handle_text_assignment(
            &["text", "text_anchor"],
            self.type_name(),
            property,
            value,
            ctx,
            env,
            diagnostics,
            subject,
        )
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::text::evaluate_math(ctx, text_ctx)
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        crate::primitives::text_default_props(scene, "x^2 + y^2")
    }
}
