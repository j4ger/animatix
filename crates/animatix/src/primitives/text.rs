//! Text primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, Primitive};
use crate::timeline::{Environment, SceneDimensions, TrackAccessor};

/// Shared `handle_assignment` implementation for text-family primitives.
///
/// `accepted` is the primitive's assigned-property allowlist; `type_name`
/// names it in the geometry-tier invariant message so each primitive keeps its
/// existing wording ("Code assignment requires the geometry tier", etc.).
pub(crate) fn handle_text_assignment(
    accepted: &[&str],
    type_name: &str,
    property: &str,
    value: &Expr,
    ctx: &mut AssignmentCtx,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> bool {
    if !accepted.contains(&property) {
        return false;
    }
    let text = ctx.text.as_deref_mut().expect("assignment text tier");
    let geometry = match ctx.geometry.as_mut() {
        Some(geometry) => geometry,
        None => panic!("{type_name} assignment requires the geometry tier"),
    };
    let style = ctx.style.as_deref_mut().expect("assignment style tier");
    let target_text = if property == "text_anchor" {
        let anchor = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
            value,
            env,
            diagnostics,
            subject,
        )
        .unwrap_or(crate::timeline::Value::Str("center".to_string()))
        .as_str()
        .to_string();
        text.text_anchor.ensure("center".to_string()).add_keyframe(
            ctx.t_end_ms,
            anchor,
            ctx.easing,
        );
        text.text_content.get(ctx.t_end_ms, String::new())
    } else {
        crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
            value,
            env,
            diagnostics,
            subject,
        )
        .unwrap_or(crate::timeline::Value::Str(String::new()))
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

/// The `Text` primitive.
pub struct TextPrimitive;

/// Singleton instance of `TextPrimitive`.
pub const TEXT: TextPrimitive = TextPrimitive;

impl Primitive for TextPrimitive {
    fn type_name(&self) -> &str {
        "Text"
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
        handle_text_assignment(
            &["text", "latex", "math", "code", "text_anchor"],
            self.type_name(),
            property,
            value,
            ctx,
            env,
            diagnostics,
            subject,
        )
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
        crate::primitives::text_default_props(scene, "Text")
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::BuildReport;
    use crate::timeline::{Timeline, TrackAccessor};

    fn build(source: &str) -> BuildReport<Timeline> {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        Timeline::build_with_diagnostics(
            &ast.expect("parsed source"),
            &std::collections::HashMap::new(),
        )
    }

    #[test]
    fn text_assignment_recompiles_content() {
        let report = build(
            r#"label: Text, text: "before"
#0.5s
label.text = "after""#,
        );
        let id = report.output.get_entity_id("label").expect("text actor");
        let text = report.output.world().text_or_default(id);
        assert_eq!(text.text_content.get(0, String::new()), "before");
        assert_eq!(text.text_content.get(500, String::new()), "after");
        // The assignment recompiles paths; the declaration's static glyph
        // caches must not shadow them for the rest of the timeline.
        assert!(text.static_paths.is_none(), "static paths must be invalidated");
        assert!(text.hardware_layout.is_none(), "hardware layout must be invalidated");
    }
}
