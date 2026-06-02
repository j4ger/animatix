//! Column layout container primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Col` primitive.
pub struct ColPrimitive;

/// Singleton instance of [`ColPrimitive`].
pub const COL: ColPrimitive = ColPrimitive;

impl Primitive for ColPrimitive {
    fn type_name(&self) -> &'static str { "Col" }
    fn display_name(&self) -> &'static str { "Column" }
    fn category(&self) -> ActorCategory { ActorCategory::Container }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::COLUMNS }
    fn is_container(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Col }

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
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError> {
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("gap", Expr::Num(0.0)),
            Property::new("padding", Expr::Num(0.0)),
            Property::new("align", Expr::Str("center".into())),
        ]
    }
}
