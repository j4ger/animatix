use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath};
use crate::timeline::{
    lookup_evaluate_expr_with_lookup_diagnostic as evaluate_expr_with_lookup_diagnostic,
    Environment, Value,
};

pub struct ArcPrimitive;
pub const ARC: ArcPrimitive = ArcPrimitive;

impl Primitive for ArcPrimitive {
    fn type_name(&self) -> &'static str { "Arc" }
    fn display_name(&self) -> &'static str { "Arc" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "arrows-clockwise" }
    fn is_shape(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Shape(crate::timeline::ShapeKind::Arc) }

    fn build(&self, _ctx: &mut BuildCtx, _label: &str, _props: &[Property], _modifiers: &[Modifier], _children: &[InlineItem]) -> Result<(), Vec<Diagnostic>> {
        // Build handled by legacy dispatch
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let path = KurboShape::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(ctx.state.size[0] as f64, ctx.state.size[1] as f64),
            start_angle: ctx.state.arc_angles[0] as f64,
            sweep_angle: ctx.state.arc_angles[1] as f64,
            rotation: ctx.state.rotation as f64,
        }
        .to_path_default();
        Some(vec![self.build_vello_path(ctx, path)])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "at".into(), value: Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)]), value_span: None, trailing_comment: None },
            Property { name: "size".into(), value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]), value_span: None, trailing_comment: None },
            Property { name: "start_angle".into(), value: Expr::Num(0.0), value_span: None, trailing_comment: None },
            Property { name: "sweep_angle".into(), value: Expr::Num(1.5707963267948966), value_span: None, trailing_comment: None },
            Property { name: "color".into(), value: Expr::Ident("accent.primary".into()), value_span: None, trailing_comment: None },
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool { false }

    fn exposes_tip_size(&self) -> bool { false }

    fn supports_fill(&self) -> bool { false }

    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        match property {
            "stroke" | "stroke_color" => Some("stroke.default"),
            "color" => None,
            _ => None,
        }
    }

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
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.arc_angles[0] as f64));
                state.arc_angles[0] = v.as_num() as f32;
                true
            }
            "sweep_angle" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(state.arc_angles[1] as f64));
                state.arc_angles[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }
}

impl ArcPrimitive {
    fn build_vello_path(&self, ctx: &RenderCtx, path: kurbo::BezPath) -> VelloPath {
        use vello::peniko::Color;
        VelloPath {
            path,
            fill: None,
            stroke: crate::timeline::shapes::shape_stroke(ctx.style.stroke_color, ctx.style.stroke_width)
                .or_else(|| Some((Color::from_rgba8(0, 0, 0, 255), 1.0))),
        }
    }
}
