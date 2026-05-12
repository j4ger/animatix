use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

pub struct TextPrimitive;
pub const TEXT: TextPrimitive = TextPrimitive;

impl Primitive for TextPrimitive {
    fn type_name(&self) -> &'static str { "Text" }
    fn display_name(&self) -> &'static str { "Text" }
    fn category(&self) -> ActorCategory { ActorCategory::Text }
    fn icon_id(&self) -> &'static str { "text-t" }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Text }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        
        ctx.timeline.process_text_actor_decl(
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
            Property { name: "text".into(), value: Expr::Str("Text".into()), value_span: None, trailing_comment: None },
            Property { name: "font_size".into(), value: Expr::Num(48.0), value_span: None, trailing_comment: None },
        ]
    }
}
