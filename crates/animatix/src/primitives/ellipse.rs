//! Ellipse shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::{
    DEFAULT_LAYOUT_HALF_SIZE, Environment, SceneDimensions, TrackAccessor, Value, VectorShapeState,
    VelloPath, lookup_evaluate_expr_with_lookup_diagnostic as evaluate_expr_with_lookup_diagnostic,
};

/// The `Ellipse` primitive.
pub struct EllipsePrimitive;

/// Singleton instance of [`EllipsePrimitive`].
pub const ELLIPSE: EllipsePrimitive = EllipsePrimitive;

impl Primitive for EllipsePrimitive {
    fn type_name(&self) -> &'static str {
        "Ellipse"
    }
    fn display_name(&self) -> &'static str {
        "Ellipse"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::CIRCLE_NOTCH
    }
    fn is_shape(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Ellipse)
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
        let VectorShapeState::Ellipse(state) = ctx.state else {
            return None;
        };
        let path = KurboShape::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(state.size[0] as f64, state.size[1] as f64),
            rotation: state.rotation as f64,
        }
        .to_path_default();
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
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {
        // Ellipse has no actor-type-specific defaults
    }

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool {
        false
    }

    fn exposes_tip_size(&self) -> bool {
        false
    }

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
        use crate::timeline::shapes::EllipseState;

        let half_size = ctx.track.geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let arc_angles = ctx.track.shape.arc_angles.get(ctx.time_ms, [0.0, 0.0]);
        let rot = ctx.track.geometry.rotation.get(ctx.time_ms, 0.0);

        let mut state = EllipseState {
            size: half_size,
            arc_angles,
            rotation: if rot != 0.0 { rot } else { 0.0 },
        };

        if let Some(overrides) = ctx.overrides {
            if let Some(Value::Vec2(s)) = overrides.get("size") {
                state.size[0] = s[0] as f32;
                state.size[1] = s[1] as f32;
            }
            if let Some(Value::Num(r)) = overrides.get("rotation") {
                state.rotation = *r as f32;
            }
        }

        evaluate_shape_render(self, ctx, &VectorShapeState::Ellipse(state))
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
        let VectorShapeState::Ellipse(ellipse) = state else {
            return false;
        };
        match name {
            "radius_x" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(ellipse.size[0] as f64));
                ellipse.size[0] = v.as_num() as f32;
                true
            },
            "radius_y" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(Value::Num(ellipse.size[1] as f64));
                ellipse.size[1] = v.as_num() as f32;
                true
            },
            _ => false,
        }
    }
}
