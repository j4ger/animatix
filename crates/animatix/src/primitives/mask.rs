//! Mask container primitive.

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Mask` primitive.
pub struct MaskPrimitive;

/// Singleton instance of [`MaskPrimitive`].
pub const MASK: MaskPrimitive = MaskPrimitive;

impl Primitive for MaskPrimitive {
    fn type_name(&self) -> &'static str { "Mask" }
    fn display_name(&self) -> &'static str { "Mask" }
    fn category(&self) -> ActorCategory { ActorCategory::Container }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::MASK_HAPPY }
    fn is_advanced(&self) -> bool { true }
    fn is_container(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Mask }

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