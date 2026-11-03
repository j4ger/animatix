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

    /// Forward the children's union extent to the layout.
    ///
    /// A Group is a structural wrapper — it paints nothing and lays nothing
    /// out — so the extent it should publish to a parent Row/Col is exactly the
    /// placed extent of its children. Without this the group contributes its
    /// default 100×100 box, inflating the parent's main axis and reporting a
    /// cross-extent unrelated to the content (`*_wrap: Group` nesting).
    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.publish_group_measured_extent(label, ctx.time_ms as u64);
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
