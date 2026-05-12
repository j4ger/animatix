use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{Environment, VectorShapeState};
use crate::timeline::shapes::parse_point_list_expr;
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VelloPath};

pub struct PolygonPrimitive;
pub const POLYGON: PolygonPrimitive = PolygonPrimitive;

impl Primitive for PolygonPrimitive {
    fn type_name(&self) -> &'static str { "Polygon" }
    fn display_name(&self) -> &'static str { "Polygon" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "polygon" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Polygon) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = if !ctx.state.points.is_empty() {
            let points = ctx
                .state
                .points
                .iter()
                .map(|p| kurbo::Point::new(p[0] as f64, p[1] as f64))
                .collect();
            KurboShape::Polygon { points }.to_path_default()
        } else if let Some(custom_path) = &ctx.state.custom_path {
            KurboShape::Path { path: custom_path.clone() }.to_path_default()
        } else {
            KurboShape::Polygon { points: Vec::new() }.to_path_default()
        };
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property {
                name: "points".into(),
                value: Expr::Tuple(vec![
                    Expr::Tuple(vec![Expr::Num(-60.0), Expr::Num(60.0)]),
                    Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-60.0)]),
                    Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(60.0)]),
                ]),
                value_span: None,
                trailing_comment: None,
            },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn supports_fill(&self) -> bool { true }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        if name != "points" { return false; }
        if let Some(points) = parse_point_list_expr(value, env) {
            state.custom_path = Some(KurboShape::Polygon { points }.to_path_default());
        }
        true
    }

    fn uses_custom_path(&self) -> bool { true }
}

impl PolygonPrimitive {
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
