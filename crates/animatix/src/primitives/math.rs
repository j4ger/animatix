//! Math formula primitive (first-class Typst math).

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};

/// The `Math` primitive.
pub struct MathPrimitive;

/// Singleton instance of `MathPrimitive`.
pub const MATH: MathPrimitive = MathPrimitive;

impl Primitive for MathPrimitive {
    fn type_name(&self) -> &str {
        "Math"
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
                    format!("{e}"),
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
        if !matches!(property, "text") {
            return false;
        }
        let target = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
        if let Err(e) = crate::timeline::recompile_text_at_assignment(
            track,
            target,
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

        let (paths, clip_path) = if let Some(text_ctx) = text_ctx {
            evaluate_text_paths(
                ctx,
                text_ctx,
                TextKind::Math,
                crate::renderer::text::default_font_size(TextKind::Math),
            )
        } else {
            let (paths, clip) = ctx.track.evaluate_text_paths_and_clip(ctx.time_ms);
            Ok((std::sync::Arc::from(paths), clip))
        }?;
        if paths.is_empty() {
            Ok(None)
        } else {
            let fill_gradient = if ctx.track.style.fill_gradient.is_some() {
                use crate::timeline::TrackAccessor;
                Some(Box::new(ctx.track.style.fill_gradient.get(ctx.time_ms, crate::renderer::types::GradientSpec::default())))
            } else {
                None
            };
            Ok(Some(vec![RenderCommand::Text { paths, fill_gradient, clip_path }]))
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
            Property::new("text", Expr::Str("x^2 + y^2".into())),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}
