//! Arrow shape primitive with a dedicated arrowhead.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    ActorCaps, Environment, SceneAnchor, SceneDimensions, VectorShapeState, VelloPath,
    evaluate_expr,
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
        crate::timeline::behavior::builtin::shape::render_arrow(ctx)
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

    fn default_color_key(&self, property: &str, _caps: &ActorCaps) -> Option<&'static str> {
        match property {
            "stroke" => Some("stroke.default"),
            "color" => None,
            _ => None,
        }
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::shape::evaluate_arrow(ctx, text_ctx)
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
                if let Expr::Path(segments) = value {
                    if segments.len() == 2 && SceneAnchor::from_str(&segments[1]).is_some() {
                        return true;
                    }
                }
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    arrow.from = parsed;
                }
                true
            },
            "to" => {
                if let Expr::Path(segments) = value {
                    if segments.len() == 2 && SceneAnchor::from_str(&segments[1]).is_some() {
                        return true;
                    }
                }
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
