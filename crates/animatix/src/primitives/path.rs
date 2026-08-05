//! Custom path shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::shapes::parse_path_commands_expr;
use crate::timeline::{Environment, SceneDimensions, TrackAccessor, VectorShapeState, VelloPath};

/// The `Path` primitive.
pub struct PathPrimitive;

/// Singleton instance of [`PathPrimitive`].
pub const PATH: PathPrimitive = PathPrimitive;

impl Primitive for PathPrimitive {
    fn type_name(&self) -> &'static str {
        "Path"
    }
    fn display_name(&self) -> &'static str {
        "Path"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::PEN
    }
    fn is_shape(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Path)
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let VectorShapeState::Path(state) = ctx.state else {
            return None;
        };
        let path = state.custom_path.clone().unwrap_or_else(kurbo::BezPath::new);
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
                "commands",
                Expr::List(vec![
                    Expr::Call("move_to".into(), vec![Expr::Num(-50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(-50.0), Expr::Num(50.0)]),
                    Expr::Call("close".into(), vec![]),
                ]),
            ),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn supports_fill(&self) -> bool {
        true
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::evaluate_shape_render;
        use crate::timeline::shapes::PathState;

        let half_size = ctx
            .track
            .geometry
            .size
            .get(ctx.time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
        let vector_paths = ctx.track.evaluate_vector_paths(ctx.time_ms);

        let mut state = PathState {
            size: half_size,
            custom_path: vector_paths.first().map(|vp| vp.path.clone()),
        };

        if let Some(overrides) = ctx.overrides {
            if let Some(crate::timeline::Value::Vec2(s)) = overrides.get("size") {
                state.size[0] = s[0] as f32;
                state.size[1] = s[1] as f32;
            }
        }

        evaluate_shape_render(self, ctx, &VectorShapeState::Path(state))
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
        let VectorShapeState::Path(path) = state else {
            return false;
        };
        if name != "commands" {
            return false;
        }
        path.custom_path = parse_path_commands_expr(value, env);
        true
    }

    fn uses_custom_path(&self) -> bool {
        true
    }
}
