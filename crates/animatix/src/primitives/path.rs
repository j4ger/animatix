use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{Environment, VectorShapeState};
use crate::timeline::shapes::parse_path_commands_expr;
use crate::timeline::{SceneDimensions, VelloPath};

pub struct PathPrimitive;
pub const PATH: PathPrimitive = PathPrimitive;

impl Primitive for PathPrimitive {
    fn type_name(&self) -> &'static str { "Path" }
    fn display_name(&self) -> &'static str { "Path" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "pen" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Path) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = ctx.state.custom_path.clone().unwrap_or_else(kurbo::BezPath::new);
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property {
                name: "commands".into(),
                value: Expr::Tuple(vec![
                    Expr::Call("move_to".into(), vec![Expr::Num(-50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(-50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    Expr::Call("line_to".into(), vec![Expr::Num(-50.0), Expr::Num(50.0)]),
                    Expr::Call("close".into(), vec![]),
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
        if name != "commands" { return false; }
        state.custom_path = parse_path_commands_expr(value, env);
        true
    }

    fn uses_custom_path(&self) -> bool { true }
}

impl PathPrimitive {
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
