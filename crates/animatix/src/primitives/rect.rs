//! Rectangle shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::{SceneDimensions, TrackAccessor, VectorShapeState, VelloPath};

/// The `Rect` primitive.
pub struct RectPrimitive;

/// Singleton instance of `RectPrimitive`.
pub const RECT: RectPrimitive = RectPrimitive;

impl Primitive for RectPrimitive {
    fn type_name(&self) -> &str {
        "Rect"
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
        let VectorShapeState::Rect(state) = ctx.state else {
            return None;
        };
        // PF-6: static-size rects share one memoized BezPath per track. The
        // memo key is the sampled `KurboShape`, so a change in the corner
        // radius (animated or not) rebuilds the path instead of reusing it.
        let (x0, y0) = (-(state.size[0] as f64), -(state.size[1] as f64));
        let (x1, y1) = (state.size[0] as f64, state.size[1] as f64);
        let shape = if state.corner_radius > 0.0 {
            KurboShape::rounded_rect(x0, y0, x1, y1, state.corner_radius as f64)
        } else {
            KurboShape::Rect { x0, y0, x1, y1 }
        };
        let path = ctx.track.shape_path_memoized(&shape);
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

    /// `corner_radius` arrives as a shape property, like `arc_angles`: it lands
    /// on the shape state here, and the build writes the state into the shape
    /// track that [`Self::evaluate`] samples.
    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &crate::timeline::Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Rect(rect) = state else {
            return false;
        };
        match name {
            "corner_radius" => {
                match crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
                    value,
                    env,
                    diagnostics,
                    subject,
                ) {
                    Some(crate::timeline::Value::Num(radius)) => rect.corner_radius = radius as f32,
                    Some(other) => diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("corner_radius expects a number, got {other:?}"),
                    )),
                    None => {},
                }
                true
            },
            _ => false,
        }
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::evaluate_shape_render;
        use crate::timeline::shapes::RectState;
        use crate::timeline::{DEFAULT_LAYOUT_HALF_SIZE, VectorShapeState};

        let half_size = ctx.track.geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let mut state = RectState {
            size: half_size,
            corner_radius: ctx.track.shape.corner_radius.get(ctx.time_ms, 0.0),
        };

        if let Some(overrides) = ctx.overrides {
            if let Some(s) = crate::primitives::override_size(overrides) {
                state.size = s;
            }
            if let Some(crate::timeline::Value::Num(radius)) = overrides.get("corner_radius") {
                state.corner_radius = *radius as f32;
            }
        }

        evaluate_shape_render(self, ctx, &VectorShapeState::Rect(state))
    }
}
