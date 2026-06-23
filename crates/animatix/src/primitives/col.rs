//! Column layout container primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
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
        let mut gap = [0.0f32; 2];
        let mut padding = [0.0f32; 4];
        let mut align: Option<String> = None;

        let env = ctx.timeline.env();
        for prop in props {
            match prop.name.as_str() {
                "gap" => {
                    match &prop.value {
                        Expr::Tuple(items) if items.len() == 2 => {
                            if let (Ok(Value::Num(a)), Ok(Value::Num(b))) = (
                                crate::timeline::utils::evaluate_expr(&items[0], env),
                                crate::timeline::utils::evaluate_expr(&items[1], env),
                            ) {
                                gap = [a as f32, b as f32];
                            }
                        },
                        _ => {
                            if let Ok(Value::Num(n)) =
                                crate::timeline::utils::evaluate_expr(&prop.value, env)
                            {
                                gap = [n as f32, n as f32];
                            }
                        },
                    }
                },
                "padding" => {
                    match &prop.value {
                        Expr::Tuple(items) if items.len() == 4 => {
                            if let (Ok(Value::Num(a)), Ok(Value::Num(b)), Ok(Value::Num(c)), Ok(Value::Num(d))) = (
                                crate::timeline::utils::evaluate_expr(&items[0], env),
                                crate::timeline::utils::evaluate_expr(&items[1], env),
                                crate::timeline::utils::evaluate_expr(&items[2], env),
                                crate::timeline::utils::evaluate_expr(&items[3], env),
                            ) {
                                padding = [a as f32, b as f32, c as f32, d as f32];
                            }
                        },
                        Expr::Tuple(items) if items.len() == 2 => {
                            // (horizontal, vertical) → [h, v, h, v]
                            if let (Ok(Value::Num(h)), Ok(Value::Num(v))) = (
                                crate::timeline::utils::evaluate_expr(&items[0], env),
                                crate::timeline::utils::evaluate_expr(&items[1], env),
                            ) {
                                padding = [h as f32, v as f32, h as f32, v as f32];
                            }
                        },
                        _ => {
                            if let Ok(Value::Num(n)) =
                                crate::timeline::utils::evaluate_expr(&prop.value, env)
                            {
                                let v = n as f32;
                                padding = [v, v, v, v];
                            }
                        },
                    }
                },
                "align" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(&prop.value, env, ctx.diagnostics, label) {
                        align = Some(v.as_str().to_string());
                    }
                },
                _ => {},
            }
        }

        // Parse vertical_align property
        let mut vertical_align: Option<String> = None;
        for prop in props {
            if prop.name == "vertical_align" {
                if let Some(v) = evaluate_expr_with_lookup_diagnostic(&prop.value, env, ctx.diagnostics, label) {
                    vertical_align = Some(v.as_str().to_string());
                }
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
            vertical_align.as_deref(),
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
