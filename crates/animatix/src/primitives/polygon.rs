//! Polygon shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::shapes::parse_point_list_expr;
use crate::timeline::{Environment, SceneDimensions, VectorShapeState, VelloPath};

/// The `Polygon` primitive.
pub struct PolygonPrimitive;

/// Singleton instance of `PolygonPrimitive`.
pub const POLYGON: PolygonPrimitive = PolygonPrimitive;

impl Primitive for PolygonPrimitive {
    fn type_name(&self) -> &str {
        "Polygon"
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
        crate::timeline::behavior::builtin::shape::render_polygon(ctx)
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "points",
                Expr::List(vec![
                    Expr::Tuple(vec![Expr::Num(-60.0), Expr::Num(60.0)]),
                    Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-60.0)]),
                    Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(60.0)]),
                ]),
            ),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::shape::evaluate_polygon(ctx, text_ctx)
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Polygon(polygon) = state else {
            return false;
        };
        match name {
            "points" => {
                if let Some(points) = parse_point_list_expr(value, env) {
                    polygon.custom_path = Some(KurboShape::Polygon { points }.to_path_default());
                }
                true
            },
            _ => false,
        }
    }
}
