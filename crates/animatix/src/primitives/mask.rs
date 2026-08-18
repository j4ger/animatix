//! Mask container primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, ChildProcessing, Primitive};
use crate::timeline::SceneDimensions;

/// The `Mask` primitive.
pub struct MaskPrimitive;

/// Singleton instance of `MaskPrimitive`.
pub const MASK: MaskPrimitive = MaskPrimitive;

impl Primitive for MaskPrimitive {
    fn type_name(&self) -> &'static str {
        "Mask"
    }
    fn display_name(&self) -> &'static str {
        "Mask"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::MASK_HAPPY
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn is_container(&self) -> bool {
        true
    }
    fn child_processing(&self) -> ChildProcessing {
        ChildProcessing::Mask
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Mask
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

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        // Mask has no visual content of its own; children are handled
        // by render_node_children with clipping. Return empty commands
        // so the trait-dispatch path computes hit regions.
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![Property::new(
            "size",
            Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(200.0)]),
        )]
    }
}
