//! Ellipse shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    Environment, SceneDimensions, Value, VectorShapeState, VelloPath,
    lookup_evaluate_expr_with_lookup_diagnostic as evaluate_expr_with_lookup_diagnostic,
};

/// The `Ellipse` primitive.
pub struct EllipsePrimitive;

/// Singleton instance of `EllipsePrimitive`.
pub const ELLIPSE: EllipsePrimitive = EllipsePrimitive;

impl Primitive for EllipsePrimitive {
    fn type_name(&self) -> &str {
        "Ellipse"
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        crate::timeline::behavior::builtin::shape::render_ellipse(ctx)
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {
        // Ellipse has no actor-type-specific defaults
    }

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::shape::evaluate_ellipse(ctx, text_ctx)
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Ellipse(ellipse) = state else {
            return false;
        };
        match name {
            "radius_x" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(ellipse.size[0] as f64));
                ellipse.size[0] = v.as_num() as f32;
                true
            },
            "radius_y" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(ellipse.size[1] as f64));
                ellipse.size[1] = v.as_num() as f32;
                true
            },
            _ => false,
        }
    }
}
