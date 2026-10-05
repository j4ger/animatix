//! Glass container primitive — a backdrop-blur scope.
//!
//! A `Glass` scope blurs whatever the frame holds *behind* it and then draws its
//! own children above that blur, which is what makes a translucent card read as
//! glass rather than as a grey rectangle. That input source is why it is a
//! container and not a `Filter` stage: an effect chain's two inputs (binding 0 =
//! the previous pass, binding 5 = the scope's own pre-chain sub-scene) can only
//! ever see the scope's own content, never the main render target. See
//! `docs/spec.md` ("Glass") and `docs/effects.md`.
//!
//! It paints nothing of its own, like every other container. The frost is a copy
//! of the *finished* frame, so a fill or hairline border drawn by the scope would
//! be inside the pixels that get blurred — measured: a 4 px authored border lands
//! at (93,27,32) instead of (255,0,0). The panel surface is the scope's first
//! child, which composites above the blur and stays crisp.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive, RenderCtx};
use crate::renderer::error::RenderError;
use crate::timeline::SceneDimensions;

/// The `Glass` primitive.
pub struct GlassPrimitive;

/// Singleton instance of `GlassPrimitive`.
pub const GLASS: GlassPrimitive = GlassPrimitive;

impl Primitive for GlassPrimitive {
    fn type_name(&self) -> &str {
        "Glass"
    }

    fn render_children(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        _children: &[&str],
    ) -> Result<(), RenderError> {
        let timeline = ctx.timeline;
        timeline.render_glass_children_ctx(ctx);
        Ok(())
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Property and effect-stage handling is the generic walk's; the backdrop
        // chain is the scope's own effect stages, declared exactly like a
        // `Filter`'s (`frost: Blur, radius: 18`).
        Ok(())
    }

    /// The shell draws nothing: the region it frosts comes from the track's
    //  geometry, not from a path. Empty commands (not `None`) keep the scope
    /// clickable at its box and its bounds recorded, as every container does.
    fn render(&self, _ctx: &RenderCtx) -> Option<Vec<crate::timeline::VelloPath>> {
        None
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, RenderError> {
        Ok(Some(vec![]))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)])),
        ]
    }
}
