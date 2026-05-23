//! Grid layout container primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Grid` primitive.
pub struct GridPrimitive;

/// Singleton instance of [`GridPrimitive`].
pub const GRID: GridPrimitive = GridPrimitive;

impl Primitive for GridPrimitive {
    fn type_name(&self) -> &'static str { "Grid" }
    fn display_name(&self) -> &'static str { "Grid" }
    fn category(&self) -> ActorCategory { ActorCategory::Container }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::SQUARES_FOUR }
    fn is_container(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Grid }

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
            Property::new("gap", Expr::Num(0.0)),
            Property::new("padding", Expr::Num(0.0)),
            Property::new("align", Expr::Str("center".into())),
            Property::new("cols", Expr::Num(2.0)),
        ]
    }
}
