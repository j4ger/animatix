//! Shared helpers for SourceEdit unit tests.

use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::parse_source;

/// Parse test source and return its AST when parsing produced one.
pub(super) fn parse(source: &str) -> Vec<Stmt> {
    parse_source(source).0.expect("failed to parse test source")
}
