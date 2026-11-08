//! Typst document primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, Primitive};
use crate::timeline::{Environment, SceneDimensions};

/// The `Typst` primitive.
pub struct TypstPrimitive;

/// Singleton instance of `TypstPrimitive`.
pub const TYPST: TypstPrimitive = TypstPrimitive;

impl Primitive for TypstPrimitive {
    fn type_name(&self) -> &str {
        "Typst"
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
            &["text", "code", "text_anchor"],
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
        crate::timeline::behavior::builtin::text::evaluate_typst(ctx, text_ctx)
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        crate::primitives::text_default_props(scene, "*bold* and _italic_")
    }
}
