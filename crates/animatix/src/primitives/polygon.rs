//! Polygon shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::shapes::parse_point_list_expr;
use crate::timeline::{Environment, SceneDimensions, TrackAccessor, VectorShapeState, VelloPath};

/// The `Polygon` primitive.
pub struct PolygonPrimitive;

/// Singleton instance of `PolygonPrimitive`.
pub const POLYGON: PolygonPrimitive = PolygonPrimitive;

impl Primitive for PolygonPrimitive {
    fn type_name(&self) -> &str {
        "Polygon"
    }
    fn display_name(&self) -> &str {
        "Polygon"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &str {
        crate::icon_glyphs::POLYGON
    }
    fn is_shape(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Polygon)
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
        let VectorShapeState::Polygon(state) = ctx.state else {
            return None;
        };
        let path = if !state.points.is_empty() {
            let points = state
                .points
                .iter()
                .map(|p| kurbo::Point::new(p[0] as f64, p[1] as f64))
                .collect();
            KurboShape::Polygon { points }.to_path_default()
        } else if let Some(custom_path) = &state.custom_path {
            KurboShape::Path {
                path: custom_path.clone(),
            }
            .to_path_default()
        } else {
            KurboShape::Polygon { points: Vec::new() }.to_path_default()
        };
        Some(vec![crate::timeline::shapes::build_vello_path(
            path,
            ctx.style.color,
            ctx.style.stroke_color,
            ctx.style.stroke_width,
            ctx.style.fill_opacity,
            false,
        )])
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
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::evaluate_shape_render;
        use crate::timeline::shapes::PolygonState;

        let half_size = ctx
            .track
            .geometry
            .size
            .get(ctx.time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
        let points = ctx.track.shape.points.get(ctx.time_ms, Vec::new());
        let rot = ctx.track.geometry.rotation.get(ctx.time_ms, 0.0);
        let vector_paths = ctx.track.evaluate_vector_paths(ctx.time_ms);

        let mut state = PolygonState {
            size: half_size,
            regular_polygon_sides: 0,
            regular_polygon_radius: half_size[0],
            custom_path: vector_paths.first().map(|vp| vp.path.clone()),
            rotation: if rot != 0.0 { rot } else { 0.0 },
            points,
        };

        if let Some(overrides) = ctx.overrides {
            if let Some(crate::timeline::Value::Vec2(s)) = overrides.get("size") {
                state.size[0] = s[0] as f32;
                state.size[1] = s[1] as f32;
            }
            if let Some(crate::timeline::Value::Num(r)) = overrides.get("rotation") {
                state.rotation = *r as f32;
            }
        }

        evaluate_shape_render(self, ctx, &VectorShapeState::Polygon(state))
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
