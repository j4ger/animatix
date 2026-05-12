use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{Environment, VectorShapeState, Value};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VelloPath};

pub struct ArrowPrimitive;
pub const ARROW: ArrowPrimitive = ArrowPrimitive;

impl Primitive for ArrowPrimitive {
    fn type_name(&self) -> &'static str { "Arrow" }
    fn display_name(&self) -> &'static str { "Arrow" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "arrow-right" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Arrow) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = crate::timeline::shapes::build_arrow_path(
            ctx.state.line_from,
            ctx.state.line_to,
            ctx.state.size[0],
            ctx.state.size[1],
        );
        let path = KurboShape::Path { path }.to_path_default();
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "from".into(), value: Expr::Tuple(vec![Expr::Num(-50.0), Expr::Num(0.0)]), value_span: None, trailing_comment: None },
            Property { name: "to".into(), value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(0.0)]), value_span: None, trailing_comment: None },
            Property { name: "tip_length".into(), value: Expr::Num(24.0), value_span: None, trailing_comment: None },
            Property { name: "tip_width".into(), value: Expr::Num(18.0), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        if state.size == [50.0, 50.0] {
            state.size = [24.0, 18.0];
        }
    }

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn supports_fill(&self) -> bool { true }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "tip_length" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.size[0] as f64));
                state.size[0] = v.as_num() as f32;
                true
            }
            "tip_width" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.size[1] as f64));
                state.size[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }

    fn exposes_tip_size(&self) -> bool { true }
}

impl ArrowPrimitive {
    fn build_vello_path(&self, ctx: &RenderCtx, path: kurbo::BezPath) -> VelloPath {
        use vello::peniko::Color;
        VelloPath {
            path,
            fill: None,
            stroke: crate::timeline::shapes::shape_stroke(ctx.style.stroke_color, ctx.style.stroke_width)
                .or_else(|| Some((Color::from_rgba8(0, 0, 0, 255), 1.0))),
        }
    }
}
