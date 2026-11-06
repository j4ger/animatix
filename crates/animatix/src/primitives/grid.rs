//! Grid layout container primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::container_props::parse_container_props;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::utils::evaluate_expr;
use crate::timeline::{SceneDimensions, Value};

/// The `Grid` primitive.
pub struct GridPrimitive;

/// Singleton instance of `GridPrimitive`.
pub const GRID: GridPrimitive = GridPrimitive;

impl Primitive for GridPrimitive {
    fn type_name(&self) -> &str {
        "Grid"
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        let env = ctx.timeline.env();
        let container = parse_container_props(props, env, label, ctx.diagnostics, true);

        let mut cols: Option<usize> = None;
        for prop in props {
            if prop.name == "cols"
                && let Ok(Value::Num(n)) = evaluate_expr(&prop.value, env)
            {
                cols = Some((n as usize).max(1));
            }
        }

        if cols.is_none() {
            ctx.diagnostics.push(
                crate::diagnostics::Diagnostic::warning(
                    crate::diagnostics::DiagnosticCode::MissingGridCols,
                    animatix_syntax::diagnostics::DiagnosticPhase::Build,
                    format!(
                        "Grid '{label}' has no explicit `cols`; it lays out in a single \
                         column. Set `cols: N` for a multi-column layout."
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
            cols,
            ctx.diagnostics,
            None, // vertical_align not applicable for Grid
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
            Property::new("cols", Expr::Num(2.0)),
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
    fn grid_container_parses_shared_container_props() {
        let report = build(
            r#"grid: Grid, gap: (4, 6), padding: (1, 2), align: "end", cols: 3 {
                a: Rect, size: (10, 10)
            }"#,
        );
        let metadata = report.output.container_layout("grid").expect("grid metadata");
        assert_eq!(metadata.layout_type, LayoutType::Grid);
        assert_eq!(metadata.gap, [4.0, 6.0]);
        assert_eq!(metadata.padding, [1.0, 2.0, 1.0, 2.0]);
        assert_eq!(metadata.align, "end");
        assert_eq!(metadata.cols, Some(3));
        assert_eq!(metadata.vertical_align, "center");
        assert!(
            !report.diagnostics.iter().any(|d| d.code == DiagnosticCode::MissingGridCols),
            "explicit cols must not warn: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn grid_keeps_unreadable_gap_and_missing_cols_warnings() {
        let report = build(r#"grid: Grid, gap: "auto" { a: Rect, size: (10, 10) }"#);
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == DiagnosticCode::InvalidPropertyValue && d.message.contains("gap")
            }),
            "unreadable gap must still warn: {:?}",
            report.diagnostics
        );
        assert!(
            report.diagnostics.iter().any(|d| d.code == DiagnosticCode::MissingGridCols),
            "missing cols must still warn: {:?}",
            report.diagnostics
        );
        let metadata = report.output.container_layout("grid").expect("grid metadata");
        assert_eq!(metadata.gap, [0.0, 0.0], "the default gap must survive the dropped value");
        assert_eq!(metadata.cols, None);
    }
}
