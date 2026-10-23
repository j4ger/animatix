//! Parse entry implementation: tokenize, run the grammar, and assemble
//! [`ParsedSource`] / [`ParseResult`].

use std::cell::RefCell;
use std::rc::Rc;

use chumsky::prelude::*;

use super::comments::attach_trailing_comments;
use super::error::ParseError;
use super::{parse_source_diagnostics, parser, token_parser};
use crate::ast::Stmt;
use crate::diagnostics::Diagnostic;

/// Everything a parse produces, including the token stream.
///
/// The lexer runs once here; handing the tokens back lets a caller that also
/// needs them (the analyzer keeps a token stream for hover/completion/highlight
/// queries) avoid tokenizing the same source a second time.
pub struct ParsedSource {
    /// Parsed statements, absent when the source could not be parsed at all.
    pub ast: Option<Vec<Stmt>>,
    /// Structured parse errors.
    pub errors: Vec<ParseError>,
    /// Non-fatal parse-time warnings.
    pub warnings: Vec<Diagnostic>,
    /// Parser-recorded identifier occurrences.
    pub occurrences: Vec<crate::occurrence::Occurrence>,
    /// The token stream the parse consumed.
    pub tokens: Vec<crate::token::Token>,
    /// Lexical issues found while tokenizing.
    pub token_issues: Vec<crate::token::TokenIssue>,
}

pub(super) fn parse_impl(source: &str) -> ParsedSource {
    let warnings = Rc::new(RefCell::new(Vec::new()));
    let (tokens, token_issues) = crate::token::tokenize_with_issues(source);
    let spanned = token_parser::spanned(&tokens);
    let input = token_parser::as_input(&spanned);
    let ((ast, errors), occurrences) = crate::occurrence::with_occurrences(|| {
        parser(Rc::clone(&warnings)).parse(input).into_output_errors()
    });
    let mut ast = ast;
    if let Some(statements) = ast.as_mut() {
        attach_trailing_comments(statements, source, &tokens);
    }
    let owned_errors: Vec<ParseError> =
        errors.iter().map(|e| ParseError::from_rich_tokens(source, e)).collect();
    let owned_warnings = match Rc::try_unwrap(warnings) {
        Ok(cell) => cell.into_inner(),
        Err(_) => Vec::new(),
    };
    ParsedSource {
        ast,
        errors: owned_errors,
        warnings: owned_warnings,
        occurrences,
        tokens,
        token_issues,
    }
}

/// A parse result with AST, parse errors, and diagnostics.
///
/// This is the canonical result type for source parsing.
/// best-effort AST even when malformed nodes are present; chumsky returns no AST
/// when the source cannot be parsed. Callers should treat `statements` as
/// `Option` to cover both backends.
#[derive(Debug)]
pub struct ParseResult {
    /// Best-effort parsed statements, if any were produced.
    pub statements: Option<Vec<Stmt>>,
    /// Structured errors, converted from either backend into a common shape.
    pub parse_errors: Vec<ParseError>,
    /// Non-fatal parse diagnostics (currently produced by chumsky).
    pub warnings: Vec<Diagnostic>,
}

/// Parse source through the canonical semantic parse pipeline.
pub fn parse_canonical(source: &str) -> ParseResult {
    let (statements, parse_errors, warnings) = parse_source_diagnostics(source);
    ParseResult {
        statements,
        parse_errors,
        warnings,
    }
}
