use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

pub struct ImagePrimitive;
pub const IMAGE: ImagePrimitive = ImagePrimitive;

impl Primitive for ImagePrimitive {
    fn type_name(&self) -> &'static str { "Image" }
    fn display_name(&self) -> &'static str { "Image" }
    fn category(&self) -> ActorCategory { ActorCategory::Media }
    fn icon_id(&self) -> &'static str { "image" }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Image }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        
        ctx.timeline.process_media_actor_decl(
            self.type_name(),
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property { name: "at".into(), value: Expr::Tuple(vec![Expr::Num(scene.width as f64 / 2.0), Expr::Num(scene.height as f64 / 2.0)]), value_span: None, trailing_comment: None },
            Property { name: "url".into(), value: Expr::Str(String::new()), value_span: None, trailing_comment: None },
            Property { name: "size".into(), value: Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(160.0)]), value_span: None, trailing_comment: None },
        ]
    }
}
