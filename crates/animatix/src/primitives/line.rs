use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};

pub struct LinePrimitive;
pub const LINE: LinePrimitive = LinePrimitive;

impl Primitive for LinePrimitive {
    fn type_name(&self) -> &'static str { "Line" }
    fn display_name(&self) -> &'static str { "Line" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "minus" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Line) }

    fn build(&self, ctx: &mut BuildCtx, label: &str, props: &[Property], modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
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
