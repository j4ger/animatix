use std::path::Path;

use animatix_syntax::ast::Stmt;
use animatix_syntax::module::{ModuleError, ModuleGraph};
use animatix_syntax::parser::parse_source_complete;
use tracing::{error, info, warn};

pub fn run(input: &Path, compact: bool, force: bool) {
    info!("Parsing Animatix file: {}", input.display());
    match load_ast(input, force) {
        Ok(ast) => print_ast(&ast, compact),
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        },
    }
}

/// Load the entry AST.
///
/// Normally module-graph expansion is authoritative. With `force`, a module
/// graph parse error falls back to tolerant parsing of the entry file and
/// returns the recovered partial AST, reporting the individual parse errors as
/// warnings.
pub(crate) fn load_ast(input: &Path, force: bool) -> Result<Vec<Stmt>, String> {
    match ModuleGraph::new().load_entry(input) {
        Ok(statements) => Ok(statements),
        Err(ModuleError::ParseErrors(_)) if force => load_partial_ast(input),
        Err(err) => Err(err.to_string()),
    }
}

fn load_partial_ast(input: &Path) -> Result<Vec<Stmt>, String> {
    let source = std::fs::read_to_string(input)
        .map_err(|err| format!("Cannot read {}: {}", input.display(), err))?;
    let parsed = parse_source_complete(&source);
    for parse_error in &parsed.errors {
        warn!(
            "Parse error ignored by --force at {}:{}: {}",
            parse_error.line, parse_error.column, parse_error.message
        );
    }
    parsed
        .ast
        .ok_or_else(|| "unable to recover a partial AST from the parse errors".to_string())
}

/// Render statements in the `ast` command's compact or pretty form.
pub(crate) fn format_ast(statements: &[Stmt], compact: bool) -> String {
    if compact {
        format!("{statements:?}")
    } else {
        statements
            .iter()
            .map(|stmt| format!("{stmt:#?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn print_ast(statements: &[Stmt], compact: bool) {
    println!("{}", format_ast(statements, compact));
}
