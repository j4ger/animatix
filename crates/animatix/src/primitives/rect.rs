//! Rectangle shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::{SceneDimensions, VectorShapeState, VelloPath};

/// The `Rect` primitive.
pub struct RectPrimitive;

/// Singleton instance of `RectPrimitive`.
pub const RECT: RectPrimitive = RectPrimitive;

impl Primitive for RectPrimitive {
    fn type_name(&self) -> &str {
        "Rect"
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
        crate::timeline::behavior::builtin::shape::render_rect(ctx)
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    /// `corner_radius` arrives as a shape property, like `arc_angles`: it lands
    /// on the shape state here, and the build writes the state into the shape
    /// track that [`Self::evaluate`] samples.
    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &crate::timeline::Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Rect(rect) = state else {
            return false;
        };
        match name {
            "corner_radius" => {
                match crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
                    value,
                    env,
                    diagnostics,
                    subject,
                ) {
                    Some(crate::timeline::Value::Num(radius)) => rect.corner_radius = radius as f32,
                    Some(other) => diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("corner_radius expects a number, got {other:?}"),
                    )),
                    None => {},
                }
                true
            },
            _ => false,
        }
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::shape::evaluate_rect(ctx, text_ctx)
    }
}
