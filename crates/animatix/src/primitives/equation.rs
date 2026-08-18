//! Equation container primitive.
//!
//! `Equation` is a container that aggregates `Fragment` children for unified
//! Typst math rendering with per-fragment highlight support.
//!
//! The actual rendering is performed in `scene_eval.rs` (special branch in
//! `render_node_children`), so `evaluate()` returns an empty command list.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{
    ActorCategory, ActorKindId, BuildCtx, ChildProcessing, Primitive, RenderCommand,
};
use crate::timeline::SceneDimensions;

/// The `Equation` primitive.
pub struct EquationPrimitive;

/// Singleton instance of `EquationPrimitive`.
pub const EQUATION: EquationPrimitive = EquationPrimitive;

impl Primitive for EquationPrimitive {
    fn type_name(&self) -> &str {
        "Equation"
    }
    fn display_name(&self) -> &str {
        "Equation"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &str {
        crate::icon_glyphs::SIGMA
    }
    fn is_container(&self) -> bool {
        true
    }
    fn child_processing(&self) -> ChildProcessing {
        ChildProcessing::Equation
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Equation
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Child Fragment actors are processed by the generic build path
        // (process_actor_decl → process_inline_items). No extra work needed here.
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Equation rendering is handled by the special branch in
        // scene_eval.rs::render_node_children. Return empty commands so the
        // trait-dispatch path computes hit regions for the container itself.
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
