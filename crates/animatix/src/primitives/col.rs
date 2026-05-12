use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

pub struct ColPrimitive;
pub const COL: ColPrimitive = ColPrimitive;

impl Primitive for ColPrimitive {
    fn type_name(&self) -> &'static str { "Col" }
    fn display_name(&self) -> &'static str { "Column" }
    fn category(&self) -> ActorCategory { ActorCategory::Container }
    fn icon_id(&self) -> &'static str { "columns" }
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

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "gap".into(), value: Expr::Num(0.0), value_span: None, trailing_comment: None },
            Property { name: "padding".into(), value: Expr::Num(0.0), value_span: None, trailing_comment: None },
            Property { name: "align".into(), value: Expr::Str("center".into()), value_span: None, trailing_comment: None },
        ]
    }
}
