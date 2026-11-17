//! Measurement boundaries shared with the full-pipeline contract tests.

use std::path::Path;

use animatix::timeline::Timeline;
use animatix_syntax::{ast::Stmt, module::ModuleGraph};

pub const WORKLOADS: [&str; 3] = [
    "projects/fft_explain.amx",
    "components/09_components.amx",
    "components/10_modules.amx",
];

/// Required workloads must not disappear from a successful benchmark run.
pub fn required_source(path: &Path) -> String {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("required benchmark workload {}: {error}", path.display()));
    assert!(
        !source.trim().is_empty(),
        "required benchmark workload is empty: {}",
        path.display()
    );
    source
}

pub fn parsed(source: &str) -> Vec<Stmt> {
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "benchmark source parse diagnostics: {errors:?}");
    stmts.expect("benchmark source did not parse")
}

/// Parse is deliberately outside this boundary; no AST clone is required.
pub fn build_only(stmts: &[Stmt]) -> Timeline {
    Timeline::build(stmts)
}

/// Fresh module graph with an in-memory entry override. Imported modules still
/// come from disk; this is not a cached GUI session or a pure CPU benchmark.
/// No entry write/delete is needed: SourceMap supports virtual entry paths.
pub fn source_pipeline(path: &Path, source: &str) -> Timeline {
    let mut graph = ModuleGraph::new();
    let mut program =
        graph.load_program_with_source(path, Some(source)).expect("module load failed");
    let _type_diags = program.typecheck();
    let expanded = program.expand_components(&mut Vec::new());
    Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        animatix::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    )
    .output
}

/// Keep a separate real-file I/O path, including the entry read each iteration.
pub fn file_pipeline(path: &Path) -> Timeline {
    let source = std::fs::read_to_string(path).expect("benchmark entry read failed");
    source_pipeline(path, &source)
}
