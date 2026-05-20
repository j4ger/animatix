use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

pub struct GroupPrimitive;
pub const GROUP: GroupPrimitive = GroupPrimitive;

impl Primitive for GroupPrimitive {
    fn type_name(&self) -> &'static str { "Group" }
    fn display_name(&self) -> &'static str { "Group" }
    fn category(&self) -> ActorCategory { ActorCategory::Container }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::FOLDER }
    fn is_container(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Group }

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

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![]
    }
}
