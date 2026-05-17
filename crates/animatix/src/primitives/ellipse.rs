use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};
use crate::timeline::{
    lookup_evaluate_expr_with_lookup_diagnostic as evaluate_expr_with_lookup_diagnostic,
    Environment, Value,
};

pub struct EllipsePrimitive;
pub const ELLIPSE: EllipsePrimitive = EllipsePrimitive;

impl Primitive for EllipsePrimitive {
    fn type_name(&self) -> &'static str { "Ellipse" }
    fn display_name(&self) -> &'static str { "Ellipse" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "circle-notch" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Ellipse) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        // If arc angles are specified (non-default), render as an arc.
        // This absorbs the Arc primitive.
        let has_arc_angles = ctx.state.arc_angles[1] != 0.0;
        let path = if has_arc_angles {
            KurboShape::Arc {
                center: kurbo::Point::new(0.0, 0.0),
                radii: kurbo::Vec2::new(ctx.state.size[0] as f64, ctx.state.size[1] as f64),
                start_angle: ctx.state.arc_angles[0] as f64,
                sweep_angle: ctx.state.arc_angles[1] as f64,
                rotation: ctx.state.rotation as f64,
            }
        } else {
            KurboShape::Ellipse {
                center: kurbo::Point::new(0.0, 0.0),
                radii: kurbo::Vec2::new(ctx.state.size[0] as f64, ctx.state.size[1] as f64),
                rotation: ctx.state.rotation as f64,
            }
        }
        .to_path_default();
        // Arcs don't have fill; override fill_opacity to 0 for arc mode
        let fill_opacity = if has_arc_angles { 0.0 } else { ctx.style.fill_opacity };
        Some(vec![crate::timeline::shapes::build_vello_path(
            path, ctx.style.color, ctx.style.stroke_color, ctx.style.stroke_width, fill_opacity,
            has_arc_angles, // Arc mode: force stroke when no explicit stroke set
        )])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "at".into(), value: Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)]), value_span: None, trailing_comment: None },
            Property { name: "size".into(), value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)]), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }

    fn apply_defaults(&self, _actor_type: &str, _state: &mut VectorShapeState) {
        // Ellipse has no actor-type-specific defaults
    }

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool { false }

    fn exposes_tip_size(&self) -> bool { false }

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
            "radius" => {
                // Circle compat: radius sets both axes
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.size[0] as f64));
                let r = v.as_num() as f32;
                state.size = [r, r];
                true
            }
            "radius_x" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.size[0] as f64));
                state.size[0] = v.as_num() as f32;
                true
            }
            "radius_y" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.size[1] as f64));
                state.size[1] = v.as_num() as f32;
                true
            }
            "start_angle" => {
                // Arc compat
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.arc_angles[0] as f64));
                state.arc_angles[0] = v.as_num() as f32;
                true
            }
            "sweep_angle" => {
                // Arc compat
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.arc_angles[1] as f64));
                state.arc_angles[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }
}
