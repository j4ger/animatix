//! Filter container primitive.
//!
//! Renders children to an offscreen texture and applies post-processing
//! filters (blur, brightness, contrast, saturate, hue-rotate, sepia).

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Filter` primitive.
pub struct FilterPrimitive;

/// Singleton instance of [`FilterPrimitive`].
pub const FILTER: FilterPrimitive = FilterPrimitive;

impl Primitive for FilterPrimitive {
    fn type_name(&self) -> &'static str {
        "Filter"
    }
    fn display_name(&self) -> &'static str {
        "Filter"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::FILTERS
    }
    fn is_container(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Filter
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

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        // Filter has no visual content of its own; children are rendered
        // to a sub-scene and post-processed in render_node_children.
        // Return empty commands so the trait-dispatch path computes hit regions.
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![]
    }
}
