use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{Environment, VectorShapeState};
use crate::timeline::shapes::{parse_point_list_expr, regular_polygon_points};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VelloPath};
use crate::timeline::{
    lookup_evaluate_expr_with_lookup_diagnostic as evaluate_expr_with_lookup_diagnostic,
    Value,
};

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
        } else if ctx.state.regular_polygon_sides >= 3 {
            // RegularPolygon compat: generate points from sides + radius
            let points = regular_polygon_points(
                ctx.state.regular_polygon_sides,
                ctx.state.regular_polygon_radius,
                ctx.state.rotation,
            );
            KurboShape::Polygon { points }.to_path_default()
        } else if let Some(custom_path) = &ctx.state.custom_path {
            KurboShape::Path { path: custom_path.clone() }.to_path_default()
        } else {
            KurboShape::Polygon { points: Vec::new() }.to_path_default()
        };
        Some(vec![crate::timeline::shapes::build_vello_path(
            path, ctx.style.color, ctx.style.stroke_color, ctx.style.stroke_width, ctx.style.fill_opacity, false,
        )])
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

    fn apply_defaults(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _actor_type: &str, state: &mut VectorShapeState) {
        // RegularPolygon compat: if sides is set but no custom path, generate the path
        if state.custom_path.is_none() && state.regular_polygon_sides >= 3 {
            state.custom_path = Some(
                KurboShape::Polygon {
                    points: regular_polygon_points(
                        state.regular_polygon_sides,
                        state.regular_polygon_radius,
                        state.rotation,
                    ),
                }
                .to_path_default(),
            );
        }
    }

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
            "points" => {
                if let Some(points) = parse_point_list_expr(value, env) {
                    state.custom_path = Some(KurboShape::Polygon { points }.to_path_default());
                }
                true
            }
            "sides" => {
                // RegularPolygon compat
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.regular_polygon_sides as f64));
                state.regular_polygon_sides = v.as_num().round().max(3.0) as usize;
                true
            }
            "radius" => {
                // RegularPolygon compat
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.regular_polygon_radius as f64));
                state.regular_polygon_radius = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }

    fn uses_custom_path(&self) -> bool { true }
}
