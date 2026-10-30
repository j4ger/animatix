//! Code block primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, Primitive};
use crate::timeline::{Environment, SceneDimensions};

/// The `Code` primitive.
pub struct CodePrimitive;

/// Singleton instance of `CodePrimitive`.
pub const CODE: CodePrimitive = CodePrimitive;

impl Primitive for CodePrimitive {
    fn type_name(&self) -> &str {
        "Code"
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
        super::text::handle_text_assignment(
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
        crate::timeline::behavior::builtin::text::evaluate_code(ctx, text_ctx)
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
    fn code_text_assignment_recompiles_content() {
        let report = build(
            r#"code: Code, text: "before"
#0.5s
code.text = "after""#,
        );
        let id = report.output.get_entity_id("code").expect("code actor");
        let text = report.output.world().text_or_default(id);
        assert_eq!(text.text_content.get(0, String::new()), "before");
        assert_eq!(text.text_content.get(500, String::new()), "after");
    }
}
