//! Line shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    ActorCaps, Environment, SceneAnchor, SceneDimensions, VectorShapeState, VelloPath,
    lookup_parse_numeric_vec2_with_lookup_diagnostic as parse_numeric_vec2_with_lookup_diagnostic,
};

/// The `Line` primitive.
pub struct LinePrimitive;

/// Singleton instance of `LinePrimitive`.
pub const LINE: LinePrimitive = LinePrimitive;

impl Primitive for LinePrimitive {
    fn type_name(&self) -> &str {
        "Line"
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
        crate::timeline::behavior::builtin::shape::render_line(ctx)
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
        crate::timeline::behavior::builtin::shape::evaluate_line(ctx, text_ctx)
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
                if let Expr::Path(segments) = value {
                    if segments.len() == 2 && SceneAnchor::from_str(&segments[1]).is_some() {
                        return true;
                    }
                }
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    line.line_from = parsed;
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
                    line.line_to = parsed;
                }
                true
            },
            _ => false,
        }
    }
}
