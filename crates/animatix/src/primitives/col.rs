//! Column layout container primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::{SceneDimensions, Value};

/// The `Col` primitive.
pub struct ColPrimitive;

/// Singleton instance of [`ColPrimitive`].
pub const COL: ColPrimitive = ColPrimitive;

impl Primitive for ColPrimitive {
    fn type_name(&self) -> &'static str {
        "Col"
    }
    fn display_name(&self) -> &'static str {
        "Column"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Container
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::COLUMNS
    }
    fn is_container(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Col
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Container setup happens in finalize_container_build (after children are processed)
        Ok(())
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        let mut gap = 0.0f32;
        let mut padding = 0.0f32;
        let mut align: Option<String> = None;

        let env = ctx.timeline.env();
        for prop in props {
            match prop.name.as_str() {
                "gap" => {
                    if let Ok(Value::Num(n)) =
                        crate::timeline::utils::evaluate_expr(&prop.value, env)
                    {
                        gap = n as f32;
                    }
                },
                "padding" => {
                    if let Ok(Value::Num(n)) =
                        crate::timeline::utils::evaluate_expr(&prop.value, env)
                    {
                        padding = n as f32;
                    }
                },
                "align" => {
                    if let Expr::Str(s) = &prop.value {
                        align = Some(s.clone());
                    } else if let Expr::Ident(s) = &prop.value {
                        align = Some(s.clone());
                    }
                },
                _ => {},
            }
        }

        ctx.timeline.register_container_metadata_and_apply_layout(
            label,
            self.type_name(),
            ctx.time_ms as u64,
            gap,
            padding,
            align.as_deref(),
            None,
            ctx.diagnostics,
        );

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
        vec![
            Property::new("gap", Expr::Num(0.0)),
            Property::new("padding", Expr::Num(0.0)),
            Property::new("align", Expr::Str("center".into())),
        ]
    }
}
