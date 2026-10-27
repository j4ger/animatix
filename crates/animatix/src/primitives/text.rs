//! Text primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{Environment, SceneDimensions, TrackAccessor, Value};

/// The `Text` primitive.
pub struct TextPrimitive;

/// Singleton instance of `TextPrimitive`.
pub const TEXT: TextPrimitive = TextPrimitive;

impl Primitive for TextPrimitive {
    fn type_name(&self) -> &str {
        "Text"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        if let Err(e) = ctx.timeline.process_text_actor_decl(
            self.type_name(),
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        ) {
            ctx.diagnostics.push(
                Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!("{}", e),
                )
                .with_subject(label),
            );
        }
        Ok(())
    }

    fn handle_assignment(
        &self,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        if !matches!(property, "text" | "latex" | "math" | "code" | "text_anchor") {
            return false;
        }
        let text = ctx.text.as_deref_mut().expect("assignment text tier");
        let geometry = ctx.geometry.as_mut().expect("Text assignment requires the geometry tier");
        let style = ctx.style.as_deref_mut().expect("assignment style tier");
        let target_text = if property == "text_anchor" {
            let anchor = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .unwrap_or(Value::Str("center".to_string()))
                .as_str()
                .to_string();
            text.text_anchor.ensure("center".to_string()).add_keyframe(
                ctx.t_end_ms,
                anchor,
                ctx.easing,
            );
            text.text_content.get(ctx.t_end_ms, String::new())
        } else {
            evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .unwrap_or(Value::Str(String::new()))
                .as_str()
                .to_string()
        };
        if let Err(e) = crate::timeline::recompile_text_at_assignment(
            ctx.caps,
            style,
            text,
            geometry,
            target_text,
            ctx.t_start_ms,
            ctx.t_end_ms,
            ctx.easing,
            ctx.instant_delayed,
            ctx.duration_ms,
            ctx.font_context,
            ctx.text_compiler,
        ) {
            diagnostics.push(
                Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("{e}"),
                )
                .with_subject(subject),
            );
        }
        true
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::{RenderCommand, evaluate_text_paths};
        use crate::renderer::text::TextKind;
        let style = ctx.style;

        let (paths, clip_path) = if let Some(text_ctx) = text_ctx {
            evaluate_text_paths(
                ctx,
                text_ctx,
                TextKind::Text,
                crate::renderer::text::default_font_size(TextKind::Text),
            )
        } else {
            let text = ctx.text.expect("text primitive requires text component");
            let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
            Ok((std::sync::Arc::from(paths), clip))
        }?;
        if paths.is_empty() {
            Ok(None)
        } else {
            let fill_gradient = if style.fill_gradient.is_some() {
                use crate::timeline::TrackAccessor;
                Some(Box::new(
                    style
                        .fill_gradient
                        .get(ctx.time_ms, crate::renderer::types::GradientSpec::default()),
                ))
            } else {
                None
            };
            use crate::timeline::TrackAccessor;
            let col = style.color.get(ctx.time_ms, [1.0, 1.0, 1.0, 1.0]);
            let tint_color = Some(crate::renderer::text::f32_color_to_rgba8(&col));
            let text = ctx.text.expect("text primitive requires text component");
            let hardware_layout = if text.char_progress.is_none()
                && text.text_anchor.get(ctx.time_ms, "center".to_string()) == "center"
            {
                text.hardware_layout.clone()
            } else {
                None
            };
            Ok(Some(vec![RenderCommand::Text {
                paths,
                fill_gradient,
                clip_path,
                tint_color,
                hardware_layout,
            }]))
        }
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "at",
                Expr::Tuple(vec![
                    Expr::Num(scene.width as f64 / 2.0),
                    Expr::Num(scene.height as f64 / 2.0),
                ]),
            ),
            Property::new("text", Expr::Str("Text".into())),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}
