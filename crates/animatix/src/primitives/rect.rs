//! Rectangle shape primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive, RenderCtx};
use crate::timeline::{
    kurbo_shapes::KurboShape, SceneDimensions, VectorShapeState, VelloPath,
};
use crate::timeline::Environment;

/// The `Rect` primitive.
pub struct RectPrimitive;

/// Singleton instance of [`RectPrimitive`].
pub const RECT: RectPrimitive = RectPrimitive;

impl Primitive for RectPrimitive {
    fn type_name(&self) -> &'static str {
        "Rect"
    }

    fn display_name(&self) -> &'static str {
        "Rectangle"
    }

    fn category(&self) -> ActorCategory {
        ActorCategory::Shape
    }

    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::SQUARE
    }

    fn is_shape(&self) -> bool {
        true
    }

    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(crate::timeline::ShapeKind::Rect)
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        let VectorShapeState::Rect(state) = ctx.state else {
            return None;
        };
        let path = KurboShape::Rect {
            x0: -(state.size[0] as f64),
            y0: -(state.size[1] as f64),
            x1: state.size[0] as f64,
            y1: state.size[1] as f64,
        }
        .to_path_default();
        Some(vec![crate::timeline::shapes::build_vello_path(
            path, ctx.style.color, ctx.style.stroke_color, ctx.style.stroke_width, ctx.style.fill_opacity, false,
        )])
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool { false }

    fn exposes_tip_size(&self) -> bool { false }

    fn supports_fill(&self) -> bool { true }

    fn apply_property(
        &self,
        _name: &str,
        _value: &Expr,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        _state: &mut VectorShapeState,
    ) -> bool {
        false
    }
}
