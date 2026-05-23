//! Text primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;

/// The `Text` primitive.
pub struct TextPrimitive;

/// Singleton instance of [`TextPrimitive`].
pub const TEXT: TextPrimitive = TextPrimitive;

impl Primitive for TextPrimitive {
    fn type_name(&self) -> &'static str { "Text" }
    fn display_name(&self) -> &'static str { "Text" }
    fn category(&self) -> ActorCategory { ActorCategory::Text }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::TEXT_T }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Text }

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
        let target_text = evaluate_expr_with_lookup_diagnostic(
            value, env, diagnostics, subject,
        )
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

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(scene.width as f64 / 2.0), Expr::Num(scene.height as f64 / 2.0)])),
            Property::new("text", Expr::Str("Text".into())),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}