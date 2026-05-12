use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};
use crate::timeline::{
    lookup_parse_numeric_vec2_with_lookup_diagnostic as parse_numeric_vec2_with_lookup_diagnostic,
    Environment,
};

pub struct LinePrimitive;
pub const LINE: LinePrimitive = LinePrimitive;

impl Primitive for LinePrimitive {
    fn type_name(&self) -> &'static str { "Line" }
    fn display_name(&self) -> &'static str { "Line" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "minus" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Line) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = KurboShape::Line {
            p0: kurbo::Point::new(ctx.state.line_from[0] as f64, ctx.state.line_from[1] as f64),
            p1: kurbo::Point::new(ctx.state.line_to[0] as f64, ctx.state.line_to[1] as f64),
        }
        .to_path_default();
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "from".into(), value: Expr::Tuple(vec![Expr::Num(-100.0), Expr::Num(0.0)]), value_span: None, trailing_comment: None },
            Property { name: "to".into(), value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(0.0)]), value_span: None, trailing_comment: None },
            Property { name: "stroke_width".into(), value: Expr::Num(4.0), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool { false }

    fn exposes_tip_size(&self) -> bool { false }

    fn supports_fill(&self) -> bool { false }

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
            "from" => {
                if let Some(parsed) = parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject) {
                    state.line_from = parsed;
                }
                true
            }
            "to" => {
                if let Some(parsed) = parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject) {
                    state.line_to = parsed;
                }
                true
            }
            _ => false,
        }
    }
}

impl LinePrimitive {
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
