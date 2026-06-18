//! Arrow shape primitive with a dedicated arrowhead.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::shapes::build_vello_path;
use crate::timeline::{
    Environment, SceneDimensions, TrackAccessor, VectorShapeState, VelloPath, evaluate_expr,
    lookup_parse_numeric_vec2_with_lookup_diagnostic as parse_numeric_vec2_with_lookup_diagnostic,
};

/// The `Arrow` primitive.
pub struct ArrowPrimitive;

/// Singleton instance of [`ArrowPrimitive`].
pub const ARROW: ArrowPrimitive = ArrowPrimitive;

impl Primitive for ArrowPrimitive {
    fn type_name(&self) -> &'static str {
        "Arrow"
    }
    fn display_name(&self) -> &'static str {
        "Arrow"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::ARROW_RIGHT
    }
    fn is_shape(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Arrow)
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
        let VectorShapeState::Arrow(state) = ctx.state else {
            return None;
        };

        let from_x = state.from[0] as f64;
        let from_y = state.from[1] as f64;
        let to_x = state.to[0] as f64;
        let to_y = state.to[1] as f64;
        let head_size = state.head_size.max(1.0) as f64;

        let dx = to_x - from_x;
        let dy = to_y - from_y;
        let length = (dx * dx + dy * dy).sqrt();

        let mut path = kurbo::BezPath::new();

        if length <= f64::EPSILON {
            // Zero-length arrow: just render a dot-sized line
            path.move_to(kurbo::Point::new(to_x, to_y));
            path.close_path();
            return Some(vec![build_vello_path(
                path,
                ctx.style.color,
                ctx.style.stroke_color,
                ctx.style.stroke_width,
                0.0,
                true,
            )]);
        }

        // Direction vector
        let dir_x = dx / length;
        let dir_y = dy / length;
        // Perpendicular vector (for arrowhead width)
        let perp_x = -dir_y;
        let perp_y = dir_x;

        // Arrowhead dimensions
        let tip_length = head_size;
        let half_tip_width = head_size * 0.4;

        // Base of arrowhead (where it meets the shaft)
        let base_x = to_x - dir_x * tip_length;
        let base_y = to_y - dir_y * tip_length;

        // Left and right points of arrowhead triangle
        let left_x = base_x + perp_x * half_tip_width;
        let left_y = base_y + perp_y * half_tip_width;
        let right_x = base_x - perp_x * half_tip_width;
        let right_y = base_y - perp_y * half_tip_width;

        // Draw shaft (line from `from` to the base of the arrowhead)
        path.move_to(kurbo::Point::new(from_x, from_y));
        path.line_to(kurbo::Point::new(base_x, base_y));

        // Draw the arrowhead as a filled triangle
        path.move_to(kurbo::Point::new(to_x, to_y));
        path.line_to(kurbo::Point::new(left_x, left_y));
        path.line_to(kurbo::Point::new(right_x, right_y));
        path.close_path();

        // Arrow is stroke-only (shaft) with a filled arrowhead.
        // The shaft uses stroke_color, the arrowhead triangle is filled with stroke_color.
        Some(vec![VelloPath {
            path,
            fill: Some(vello::peniko::Color::from_rgba8(
                (ctx.style.stroke_color[0] * 255.0) as u8,
                (ctx.style.stroke_color[1] * 255.0) as u8,
                (ctx.style.stroke_color[2] * 255.0) as u8,
                (ctx.style.stroke_color[3] * 255.0) as u8,
            )),
            stroke: crate::timeline::shapes::shape_stroke(
                ctx.style.stroke_color,
                ctx.style.stroke_width,
            )
            .or_else(|| {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (ctx.style.stroke_color[0] * 255.0) as u8,
                        (ctx.style.stroke_color[1] * 255.0) as u8,
                        (ctx.style.stroke_color[2] * 255.0) as u8,
                        (ctx.style.stroke_color[3] * 255.0) as u8,
                    ),
                    1.0,
                ))
            }),
            line_cap: 0,
            line_join: 0,
        }])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("from", Expr::Tuple(vec![Expr::Num(-100.0), Expr::Num(0.0)])),
            Property::new("to", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(0.0)])),
            Property::new("head_size", Expr::Num(10.0)),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        let VectorShapeState::Arrow(arrow) = state else {
            return;
        };
        if arrow.head_size <= 0.0 {
            arrow.head_size = 10.0;
        }
    }

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool {
        false
    }

    fn exposes_tip_size(&self) -> bool {
        false
    }

    fn supports_fill(&self) -> bool {
        false
    }

    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        match property {
            "stroke" | "stroke_color" => Some("stroke.default"),
            "color" => None,
            _ => None,
        }
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::evaluate_shape_render;
        use crate::timeline::Value;
        use crate::timeline::shapes::ArrowState;

        let mut line_from = ctx.track.shape.line_from.get(ctx.time_ms, [-50.0, 0.0]);
        let mut line_to = ctx.track.shape.line_to.get(ctx.time_ms, [50.0, 0.0]);
        let head_size = ctx.track.shape.head_size.get(ctx.time_ms, 10.0);

        if let Some(overrides) = ctx.overrides {
            if let Some(Value::Vec2(from)) = overrides.get("from") {
                line_from = [from[0] as f32, from[1] as f32];
            }
            if let Some(Value::Vec2(to)) = overrides.get("to") {
                line_to = [to[0] as f32, to[1] as f32];
            }
        }

        let state = VectorShapeState::Arrow(ArrowState {
            from: line_from,
            to: line_to,
            head_size,
        });

        evaluate_shape_render(self, ctx, &state)
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Arrow(arrow) = state else {
            return false;
        };
        match name {
            "from" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    arrow.from = parsed;
                }
                true
            },
            "to" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    arrow.to = parsed;
                }
                true
            },
            "head_size" => {
                if let crate::timeline::Value::Num(val) =
                    evaluate_expr(value, env).unwrap_or(crate::timeline::Value::Num(10.0))
                {
                    arrow.head_size = val.max(1.0) as f32;
                }
                true
            },
            _ => false,
        }
    }
}
