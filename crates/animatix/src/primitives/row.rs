//! Row layout container primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{SceneDimensions, Value};

/// The `Row` primitive.
pub struct RowPrimitive;

/// Singleton instance of `RowPrimitive`.
pub const ROW: RowPrimitive = RowPrimitive;

/// The linear layout kinds whose container metadata parses identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutKind {
    Row,
    Col,
}

impl LayoutKind {
    fn type_name(self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Col => "Col",
        }
    }
}

/// Parse the `gap` / `padding` / `align` / `vertical_align` properties shared
/// by `Row` and `Col` and register the resulting container metadata.
///
/// `Row` and `Col` differ only in their type name and layout axis; keeping one
/// parser here means their tuple handling and diagnostics cannot drift.
pub(crate) fn finalize_container_build(
    kind: LayoutKind,
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
            "gap" => match &prop.value {
                Expr::Tuple(items) if items.len() == 2 => {
                    match (
                        crate::timeline::utils::evaluate_expr(&items[0], env),
                        crate::timeline::utils::evaluate_expr(&items[1], env),
                    ) {
                        (Ok(Value::Num(a)), Ok(Value::Num(b))) => gap = [a as f32, b as f32],
                        _ => ctx.diagnostics.push(super::unreadable_layout_value(label, "gap")),
                    }
                },
                _ => match crate::timeline::utils::evaluate_expr(&prop.value, env) {
                    Ok(Value::Num(n)) => gap = [n as f32, n as f32],
                    _ => ctx.diagnostics.push(super::unreadable_layout_value(label, "gap")),
                },
            },
            "padding" => {
                match &prop.value {
                    Expr::Tuple(items) if items.len() == 4 => {
                        if let (
                            Ok(Value::Num(a)),
                            Ok(Value::Num(b)),
                            Ok(Value::Num(c)),
                            Ok(Value::Num(d)),
                        ) = (
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
                if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, ctx.diagnostics, label)
                {
                    align = Some(v.as_str().to_string());
                }
            },
            _ => {},
        }
    }

    // Parse vertical_align property
    let mut vertical_align: Option<String> = None;
    for prop in props {
        if prop.name == "vertical_align"
            && let Some(v) =
                evaluate_expr_with_lookup_diagnostic(&prop.value, env, ctx.diagnostics, label)
        {
            vertical_align = Some(v.as_str().to_string());
        }
    }

    ctx.timeline.register_container_metadata_and_apply_layout(
        label,
        kind.type_name(),
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

impl Primitive for RowPrimitive {
    fn type_name(&self) -> &str {
        "Row"
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        finalize_container_build(LayoutKind::Row, ctx, label, props)
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
    use crate::timeline::{DebugRenderOptions, LayoutType, SceneDimensions, Timeline};

    fn build(source: &str) -> BuildReport<Timeline> {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        Timeline::build_with_diagnostics(
            &ast.expect("parsed source"),
            &std::collections::HashMap::new(),
        )
    }

    #[test]
    fn row_container_layout_reads_gap_padding_align_and_vertical_align() {
        let report = build(
            r#"row: Row, gap: (16, 8), padding: (1, 2, 3, 4), align: "start", vertical_align: "bottom" {
                a: Rect, size: (10, 10)
            }"#,
        );
        let metadata = report.output.container_layout("row").expect("row metadata");
        assert_eq!(metadata.layout_type, LayoutType::Row);
        assert_eq!(metadata.gap, [16.0, 8.0]);
        assert_eq!(metadata.padding, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(metadata.align, "start");
        assert_eq!(metadata.vertical_align, "bottom");
    }

    #[test]
    fn row_container_layout_expands_scalar_gap_and_two_value_padding() {
        let report = build(r#"row: Row, gap: 12, padding: (3, 5) { a: Rect, size: (10, 10) }"#);
        let metadata = report.output.container_layout("row").expect("row metadata");
        assert_eq!(metadata.gap, [12.0, 12.0]);
        assert_eq!(metadata.padding, [3.0, 5.0, 3.0, 5.0]);
    }

    #[test]
    fn row_container_layout_reports_unreadable_gap() {
        let report = build(r#"row: Row, gap: "auto" { a: Rect, size: (10, 10) }"#);
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == DiagnosticCode::InvalidPropertyValue && d.message.contains("gap")
            }),
            "unreadable gap must warn: {:?}",
            report.diagnostics
        );
        let metadata = report.output.container_layout("row").expect("row metadata");
        assert_eq!(metadata.gap, [0.0, 0.0], "the default gap must survive the dropped value");
    }

    #[test]
    fn row_container_layout_reports_unknown_align_lookups() {
        let report = build(
            r#"row: Row, align: missing.align, vertical_align: missing.valign {
                a: Rect, size: (10, 10)
            }"#,
        );
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.code == DiagnosticCode::UnknownLookupPath)
                .count(),
            2,
            "each unresolvable align lookup must be reported: {:?}",
            report.diagnostics
        );
        let metadata = report.output.container_layout("row").expect("row metadata");
        assert_eq!(metadata.align, "center");
        assert_eq!(metadata.vertical_align, "center");
    }

    #[test]
    fn row_container_paints_no_commands_of_its_own() {
        // The visual "capsule" in a zero-gap bar lane is not a container
        // background: the layout container must stay command-free, and only
        // its children paint.
        let report = build(
            r#"#0s
row: Row, gap: 10 {
    a: Rect, size: (20, 40), color: accent.primary
}"#,
        );
        let mut filter_backend = None;
        let program = report.output.evaluate_program_with_debug(
            0.0,
            SceneDimensions {
                width: 640,
                height: 360,
            },
            DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let row = program
            .items
            .iter()
            .find(|item| item.label == "row")
            .expect("the row actor must be evaluated");
        assert!(row.commands.is_empty(), "Row must not paint a background of its own");
        let child = program
            .items
            .iter()
            .find(|item| item.label == "a")
            .expect("the child rect must be evaluated");
        assert!(!child.commands.is_empty(), "the child still paints");
    }
}
