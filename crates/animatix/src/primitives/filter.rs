//! Filter container primitive.
//!
//! Renders children to an offscreen texture and applies post-processing
//! filters (blur, brightness, contrast, saturate, hue-rotate, sepia).

use crate::ast::Property;
use crate::primitives::Primitive;
use crate::timeline::SceneDimensions;

/// The `Filter` primitive.
pub struct FilterPrimitive;

/// Singleton instance of `FilterPrimitive`.
pub const FILTER: FilterPrimitive = FilterPrimitive;

impl Primitive for FilterPrimitive {
    fn type_name(&self) -> &str {
        "Filter"
    }

    fn render_children_by_id(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        _children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        timeline.render_filter_children_ctx(ctx);
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        // Filter has no render commands of its own; its children are rendered to
        // a sub-scene and post-processed by the Timeline container path selected
        // from the parent's `ChildProcessing` capability.
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![]
    }
}
