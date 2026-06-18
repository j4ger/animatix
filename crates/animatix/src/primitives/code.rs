//! Code block primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};

/// The `Code` primitive.
pub struct CodePrimitive;

/// Singleton instance of [`CodePrimitive`].
pub const CODE: CodePrimitive = CodePrimitive;

impl Primitive for CodePrimitive {
    fn type_name(&self) -> &'static str {
        "Code"
    }
    fn display_name(&self) -> &'static str {
        "Code"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Text
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::CODE
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Code
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
        track: &mut AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        if !matches!(property, "text" | "latex" | "math" | "code") {
            return false;
        }
        let target_text = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
        if let Err(e) = crate::timeline::recompile_text_at_assignment(
            track,
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

        let paths = if let Some(text_ctx) = text_ctx {
            evaluate_text_paths(ctx, text_ctx, TextKind::Code, 24.0)
        } else {
            Ok(std::sync::Arc::from(ctx.track.evaluate_text_paths(ctx.time_ms)))
        }?;
        if paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Text { paths }]))
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
            Property::new("text", Expr::Str("fn main() {}".into())),
            Property::new("font_size", Expr::Num(24.0)),
        ]
    }
}
