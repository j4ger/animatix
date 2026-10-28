//! Equation container primitive.
//!
//! `Equation` is a container that aggregates `Fragment` children for unified
//! Typst math rendering with per-fragment highlight support.
//!
//! The actual rendering is performed in `scene_eval.rs` (special branch in
//! `render_node_children`), so `evaluate()` returns an empty command list.

use crate::ast::{Expr, Property};
use crate::primitives::{Primitive, RenderCommand};
use crate::timeline::SceneDimensions;

/// The `Equation` primitive.
pub struct EquationPrimitive;

/// Singleton instance of `EquationPrimitive`.
pub const EQUATION: EquationPrimitive = EquationPrimitive;

impl Primitive for EquationPrimitive {
    fn type_name(&self) -> &str {
        "Equation"
    }

    fn render_children_by_id(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        _children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        timeline.render_equation_children_ctx(ctx);
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Equation rendering is handled by `Timeline::render_equation_children`
        // (reached through this primitive's `render_children_by_id` override). Return
        // empty commands so the trait-dispatch path computes hit regions for the
        // container itself.
        Ok(Some(vec![]))
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![Property::new(
            "at",
            Expr::Tuple(vec![
                Expr::Num(scene.width as f64 / 2.0),
                Expr::Num(scene.height as f64 / 2.0),
            ]),
        )]
    }
}
