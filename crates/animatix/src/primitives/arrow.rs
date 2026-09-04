//! Arrow shape primitive with a dedicated arrowhead.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    Environment, SceneDimensions, TrackAccessor, VectorShapeState, VelloPath, evaluate_expr,
    lookup_parse_numeric_vec2_with_lookup_diagnostic as parse_numeric_vec2_with_lookup_diagnostic,
};

/// Build a BezPath for an arrow from `from` to `to` with given `head_size`.
///
/// The shaft runs from `from` to the base of the arrowhead;
/// the arrowhead is a filled triangle at `to`.
pub(crate) fn build_arrow_path(from: [f32; 2], to: [f32; 2], head_size: f32) -> kurbo::BezPath {
    let from_x = from[0] as f64;
    let from_y = from[1] as f64;
    let to_x = to[0] as f64;
    let to_y = to[1] as f64;
    let head_size = head_size.max(1.0) as f64;

    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let length = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();

    if length <= f64::EPSILON {
        // Zero-length arrow: just a dot
        path.move_to(kurbo::Point::new(to_x, to_y));
        path.close_path();
        return path;
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

    path
}

/// The `Arrow` primitive.
pub struct ArrowPrimitive;

/// Singleton instance of `ArrowPrimitive`.
pub const ARROW: ArrowPrimitive = ArrowPrimitive;

impl Primitive for ArrowPrimitive {
    fn type_name(&self) -> &str {
        "Arrow"
    }
    fn display_name(&self) -> &str {
        "Arrow"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &str {
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

        let path = build_arrow_path(state.from, state.to, state.head_size);

        // Arrow is stroke-only (shaft) with a filled arrowhead.
        // The shaft uses stroke_color, the arrowhead triangle is filled with stroke_color.
        Some(vec![VelloPath {
            path: std::sync::Arc::new(path),
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
        use crate::timeline::callout_geometry::bounds_anchor_point;
        use crate::timeline::shapes::ArrowState;

        let mut line_from = ctx.track.shape.line_from.get(ctx.time_ms, [-50.0, 0.0]);
        let mut line_to = ctx.track.shape.line_to.get(ctx.time_ms, [50.0, 0.0]);
        let head_size = ctx.track.shape.head_size.get(ctx.time_ms, 10.0);

        // G6: Resolve anchor-point refs from the side-channel.
        // Anchor refs are resolved first, then overrides may replace them.
        if let Some((actor, anchor)) = ctx.track.shape.from_anchor.as_ref() {
            if let Some(resolver) = ctx.target_resolver {
                if let Some((centre, half)) =
                    resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
                {
                    line_from = bounds_anchor_point(*anchor, centre, half);
                }
            }
        }
        if let Some((actor, anchor)) = ctx.track.shape.to_anchor.as_ref() {
            if let Some(resolver) = ctx.target_resolver {
                if let Some((centre, half)) =
                    resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
                {
                    line_to = bounds_anchor_point(*anchor, centre, half);
                }
            }
        }

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
