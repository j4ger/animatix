//! Stack layout container primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::container_props::parse_container_props;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Stack` primitive.
pub struct StackPrimitive;

/// Singleton instance of `StackPrimitive`.
pub const STACK: StackPrimitive = StackPrimitive;

impl Primitive for StackPrimitive {
    fn type_name(&self) -> &str {
        "Stack"
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        let env = ctx.timeline.env();
        let container = parse_container_props(props, env, label, ctx.diagnostics, false);

        // A Stack overlaps its layers, so `gap` cannot do anything. Say so as a
        // build warning (it used to be a tracing line nobody saw) so an author
        // does not spend time tuning a value that is dropped.
        if container.saw_gap {
            ctx.diagnostics.push(
                Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::IgnoredStackGap,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!(
                        "Stack container '{label}' declares 'gap' which is ignored: Stack is an overlap container and positions its layers by alignment, not by gaps. Remove the 'gap', or use Row/Col for a spaced layout."
                    ),
                )
                .with_subject(label),
            );
        }

        ctx.timeline.register_container_metadata_and_apply_layout(
            label,
            self.type_name(),
            ctx.time_ms as u64,
            container.gap,
            container.padding,
            container.align.as_deref(),
            None,
            ctx.diagnostics,
            None, // vertical_align not applicable for Stack
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

#[cfg(test)]
mod tests {
    use crate::diagnostics::{BuildReport, DiagnosticCode};
    use crate::timeline::{LayoutType, Timeline};

    fn build(source: &str) -> BuildReport<Timeline> {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        Timeline::build_with_diagnostics(
            &ast.expect("parsed source"),
            &std::collections::HashMap::new(),
        )
    }

    #[test]
    fn stack_container_parses_shared_container_props_and_warns_on_gap() {
        let report = build(
            r#"stack: Stack, gap: (4, 6), padding: (1, 2), align: "end" {
                a: Rect, size: (10, 10)
            }"#,
        );
        let metadata = report.output.container_layout("stack").expect("stack metadata");
        assert_eq!(metadata.layout_type, LayoutType::Stack);
        assert_eq!(metadata.gap, [4.0, 6.0], "gap is parsed even though it is ignored");
        assert_eq!(metadata.padding, [1.0, 2.0, 1.0, 2.0]);
        assert_eq!(metadata.align, "end");
        assert_eq!(metadata.cols, None);
        assert_eq!(metadata.vertical_align, "center");
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.code == DiagnosticCode::IgnoredStackGap)
                .count(),
            1,
            "declaring gap must warn exactly once: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn stack_silently_keeps_the_default_for_an_unreadable_gap() {
        let report = build(r#"stack: Stack, gap: "auto" { a: Rect, size: (10, 10) }"#);
        let metadata = report.output.container_layout("stack").expect("stack metadata");
        assert_eq!(metadata.gap, [0.0, 0.0], "the default gap must survive the dropped value");
        assert!(
            !report.diagnostics.iter().any(|d| {
                d.code == DiagnosticCode::InvalidPropertyValue && d.message.contains("gap")
            }),
            "Stack never reported unreadable gaps: {:?}",
            report.diagnostics
        );
    }
}
