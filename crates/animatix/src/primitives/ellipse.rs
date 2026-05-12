use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};

pub struct EllipsePrimitive;
pub const ELLIPSE: EllipsePrimitive = EllipsePrimitive;

impl Primitive for EllipsePrimitive {
    fn type_name(&self) -> &'static str { "Ellipse" }
    fn display_name(&self) -> &'static str { "Ellipse" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "circle-notch" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Ellipse) }

    fn build(&self, ctx: &mut BuildCtx, label: &str, props: &[Property], modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = KurboShape::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(ctx.state.size[0] as f64, ctx.state.size[1] as f64),
            rotation: ctx.state.rotation as f64,
        }
        .to_path_default();
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "at".into(), value: Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)]), value_span: None, trailing_comment: None },
            Property { name: "size".into(), value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)]), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }
}

impl EllipsePrimitive {
    fn build_vello_path(&self, ctx: &RenderCtx, path: kurbo::BezPath) -> VelloPath {
        use vello::peniko::Color;
        VelloPath {
            path,
            fill: if ctx.style.fill_opacity > 0.0 {
                Some(Color::from_rgba8(
                    (ctx.style.color[0] * 255.0) as u8,
                    (ctx.style.color[1] * 255.0) as u8,
                    (ctx.style.color[2] * 255.0) as u8,
                    (ctx.style.color[3] * 255.0 * ctx.style.fill_opacity) as u8,
                ))
            } else { None },
            stroke: crate::timeline::shapes::shape_stroke(ctx.style.stroke_color, ctx.style.stroke_width),
        }
    }
}
