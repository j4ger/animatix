use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};

pub struct RegularPolygonPrimitive;
pub const REGULAR_POLYGON: RegularPolygonPrimitive = RegularPolygonPrimitive;

impl Primitive for RegularPolygonPrimitive {
    fn type_name(&self) -> &'static str { "RegularPolygon" }
    fn display_name(&self) -> &'static str { "Regular Polygon" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "hexagon" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::RegularPolygon) }

    fn build(&self, ctx: &mut BuildCtx, label: &str, props: &[Property], modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = if let Some(custom_path) = &ctx.state.custom_path {
            KurboShape::Path { path: custom_path.clone() }.to_path_default()
        } else {
            let points = crate::timeline::shapes::regular_polygon_points(
                ctx.state.regular_polygon_sides,
                ctx.state.regular_polygon_radius,
                ctx.state.rotation,
            );
            KurboShape::Polygon { points }.to_path_default()
        };
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "sides".into(), value: Expr::Num(5.0), value_span: None, trailing_comment: None },
            Property { name: "radius".into(), value: Expr::Num(60.0), value_span: None, trailing_comment: None },
            Property { name: "rotation".into(), value: Expr::Num(0.0), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }
}

impl RegularPolygonPrimitive {
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
