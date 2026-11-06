//! Shared test fixtures visible to every timeline test submodule.
//!
//! Only helpers reused by more than one sibling test module live here;
//! anything narrower stays next to its test.

use crate::ast::{Property, Stmt};
use crate::timeline::{BuildReport, Timeline};

/// Parse a fixture, returning its statements.
pub(crate) fn parse(source: &str) -> Vec<Stmt> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    ast.expect("parsed AST")
}

/// Parse and build a fixture, keeping every diagnostic.
pub(crate) fn build(source: &str) -> BuildReport<Timeline> {
    build_ast(&parse(source))
}

/// Build statements that were assembled by hand.
pub(crate) fn build_ast(ast: &[Stmt]) -> BuildReport<Timeline> {
    Timeline::build_with_diagnostics(ast, &std::collections::HashMap::new())
}

/// Build a fixture and require that no diagnostic is error-level.
pub(crate) fn build_timeline(source: &str) -> Timeline {
    let report = build(source);
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "Unexpected build errors: {:?}",
        report.diagnostics
    );
    report.output
}

pub(crate) fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}

pub(crate) fn keyframe(seconds: f64, body: Vec<Stmt>) -> Stmt {
    Stmt::Keyframe {
        time: crate::ast::Time::Seconds(seconds),
        body,
        span: None,
    }
}
