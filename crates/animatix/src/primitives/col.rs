//! Column layout container primitive.

use super::row::{LayoutKind, finalize_container_build as finalize_row_col_build};
use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Col` primitive.
pub struct ColPrimitive;

/// Singleton instance of `ColPrimitive`.
pub const COL: ColPrimitive = ColPrimitive;

impl Primitive for ColPrimitive {
    fn type_name(&self) -> &str {
        "Col"
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        finalize_row_col_build(LayoutKind::Col, ctx, label, props)
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
    use crate::timeline::{LayoutType, Timeline};

    #[test]
    fn col_uses_the_shared_layout_parser() {
        let (ast, errors) = animatix_syntax::parser::parse_source(
            r#"col: Col, gap: 6, padding: 4, align: "end", vertical_align: "top" {
                a: Rect, size: (10, 10)
            }"#,
        );
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let report = Timeline::build_with_diagnostics(
            &ast.expect("parsed source"),
            &std::collections::HashMap::new(),
        );
        let metadata = report.output.container_layout("col").expect("col metadata");
        assert_eq!(metadata.layout_type, LayoutType::Col);
        assert_eq!(metadata.gap, [6.0, 6.0]);
        assert_eq!(metadata.padding, [4.0, 4.0, 4.0, 4.0]);
        assert_eq!(metadata.align, "end");
        assert_eq!(metadata.vertical_align, "top");
    }
}
