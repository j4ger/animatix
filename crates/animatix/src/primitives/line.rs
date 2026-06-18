//! Line shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    Environment, Value,
    lookup_parse_numeric_vec2_with_lookup_diagnostic as parse_numeric_vec2_with_lookup_diagnostic,
};
use crate::timeline::{
    SceneDimensions, TrackAccessor, VectorShapeState, VelloPath, kurbo_shapes::KurboShape,
};

/// The `Line` primitive.
pub struct LinePrimitive;

/// Singleton instance of [`LinePrimitive`].
pub const LINE: LinePrimitive = LinePrimitive;

impl Primitive for LinePrimitive {
    fn type_name(&self) -> &'static str {
        "Line"
    }
    fn display_name(&self) -> &'static str {
        "Line"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::MINUS
    }
    fn is_shape(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Line)
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
        let VectorShapeState::Line(state) = ctx.state else {
            return None;
        };
        let path = KurboShape::Line {
            p0: kurbo::Point::new(state.line_from[0] as f64, state.line_from[1] as f64),
            p1: kurbo::Point::new(state.line_to[0] as f64, state.line_to[1] as f64),
        }
        .to_path_default();
        // Line is stroke-only; override fill_opacity to 0
        Some(vec![crate::timeline::shapes::build_vello_path(
            path,
            ctx.style.color,
            ctx.style.stroke_color,
            ctx.style.stroke_width,
            0.0,
            true,
        )])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("from", Expr::Tuple(vec![Expr::Num(-100.0), Expr::Num(0.0)])),
            Property::new("to", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(0.0)])),
            Property::new("stroke_width", Expr::Num(4.0)),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        let VectorShapeState::Line(line) = state else {
            return;
        };
        // Default to no arrow tips; tips are only present when explicitly set
        line.size = [0.0, 0.0];
    }

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool {
        false
    }

    fn exposes_tip_size(&self) -> bool {
        true
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
        use crate::timeline::shapes::LineState;

        let mut line_from = ctx.track.shape.line_from.get(ctx.time_ms, [-50.0, 0.0]);
        let mut line_to = ctx.track.shape.line_to.get(ctx.time_ms, [50.0, 0.0]);

        if let Some(overrides) = ctx.overrides {
            if let Some(Value::Vec2(from)) = overrides.get("from") {
                line_from = [from[0] as f32, from[1] as f32];
            }
            if let Some(Value::Vec2(to)) = overrides.get("to") {
                line_to = [to[0] as f32, to[1] as f32];
            }
        }

        let state = VectorShapeState::Line(LineState {
            size: [0.0, 0.0],
            line_from,
            line_to,
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
        let VectorShapeState::Line(line) = state else {
            return false;
        };
        match name {
            "from" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    line.line_from = parsed;
                }
                true
            },
            "to" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    line.line_to = parsed;
                }
                true
            },
            _ => false,
        }
    }
}
