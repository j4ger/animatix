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
        crate::timeline::behavior::builtin::text::evaluate_text(ctx, text_ctx)
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
