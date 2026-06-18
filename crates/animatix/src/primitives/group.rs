//! Group container primitive.

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Group` primitive.
pub struct GroupPrimitive;

/// Singleton instance of [`GroupPrimitive`].
pub const GROUP: GroupPrimitive = GroupPrimitive;

impl Primitive for GroupPrimitive {
    fn type_name(&self) -> &'static str {
        "Group"
    }
    fn display_name(&self) -> &'static str {
        "Group"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::FOLDER
    }
    fn is_container(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Group
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
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![]
    }
}
