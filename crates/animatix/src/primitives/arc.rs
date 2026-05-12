use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};

pub struct ArcPrimitive;
pub const ARC: ArcPrimitive = ArcPrimitive;

impl Primitive for ArcPrimitive {
    fn type_name(&self) -> &'static str { "Arc" }
    fn display_name(&self) -> &'static str { "Arc" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "arrows-clockwise" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Arc) }

    fn build(&self, ctx: &mut BuildCtx, label: &str, props: &[Property], modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = KurboShape::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(ctx.state.size[0] as f64, ctx.state.size[1] as f64),
            start_angle: ctx.state.arc_angles[0] as f64,
            sweep_angle: ctx.state.arc_angles[1] as f64,
            rotation: ctx.state.rotation as f64,
        }
        .to_path_default();
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "at".into(), value: Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)]), value_span: None, trailing_comment: None },
            Property { name: "size".into(), value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]), value_span: None, trailing_comment: None },
            Property { name: "start_angle".into(), value: Expr::Num(0.0), value_span: None, trailing_comment: None },
            Property { name: "sweep_angle".into(), value: Expr::Num(1.5707963267948966), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }
}

impl ArcPrimitive {
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
