//! Group container primitive.

use crate::ast::Property;
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Group` primitive.
pub struct GroupPrimitive;

/// Singleton instance of `GroupPrimitive`.
pub const GROUP: GroupPrimitive = GroupPrimitive;

impl Primitive for GroupPrimitive {
    fn type_name(&self) -> &str {
        "Group"
    }

    /// Resolve explicit relative child sizes and publish the group's extent.
    ///
    /// A Group is a structural wrapper — it paints nothing and lays nothing
    /// out. Definite axes constrain explicit relative children; auto axes use
    /// the placed content union instead of a phantom default 100×100 box.
    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.finalize_group_layout(label, ctx.time_ms as u64, ctx.diagnostics);
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
