//! Fixtures shared by the renderer's unit and integration tests.

use animatix::timeline::Timeline;

/// Parse test source and require that parsing produced no diagnostics.
pub fn parse(source: &str) -> Vec<animatix_syntax::ast::Stmt> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    ast.expect("AST")
}

/// Build a fixture without asserting on build diagnostics.
pub fn build(source: &str) -> Timeline {
    Timeline::build_with_diagnostics(&parse(source), &std::collections::HashMap::new()).output
}

/// Build a fixture and require that it produced no diagnostics.
pub fn build_checked(source: &str) -> Timeline {
    let report =
        Timeline::build_with_diagnostics(&parse(source), &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    report.output
}

/// Minimal solid-rect scene parsed from source.
pub fn solid_rect_timeline() -> Timeline {
    let source = r#"
#0s
r: Rect, at: (50, 50), size: (100, 100), color: (1, 1, 1, 1)
"#;
    build_checked(source)
}
