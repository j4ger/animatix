//!
//! # Animatix Parser
//!
//! Combinator-based recursive descent parser using [`chumsky`]. This parser is the
//! executable source of truth for accepted `.amx` syntax.
//!
//! ## Entry Points
//!
//! - [`parser()`] — parses a full `.amx` file into `Vec<Stmt>`
//!
//! ## Key Design Notes
//!
//! - The grammar is expression-heavy with prefix/infix operator precedence handled via combinator
//!   chaining in `chumsky`.
//! - Actor declarations, actions, and assignments share a generic modifier syntax in brackets
//!   `[...]`.
//! - `Text`, `Math`, `Code` are parsed as generic actor declarations.
//! - The parser accepts some syntax that the runtime may reject (e.g., method/index/construct
//!   expressions) — honest runtime diagnostics handle the mismatch.
//!
//! ## Submodules
//!
//! | Module | Contents |
//! |--------|----------|
//! | `common` | Shared parser type aliases and factory helpers (`ident`, `property`, `modifier`, `time`, etc.) |
//! | `expr` | Expression parser with operator precedence, closures, and conditionals |
//! | `inline` | Inline item parser (`InlineItem`, `FlatItem`, slot markers/fills) |
//! | `stmt` | Recursive statement parser (declarations, assignments, actions, composition) |
//! | `top_level` | Top-level parser (`config`, scenes, keyframes, `group_scenes`) |
//! | `comments` | Trailing-comment attachment and property-span collection |
//! | `entry` | Parse entry implementation (`ParsedSource`, `ParseResult`) |
//! | `error` | Structured parse errors (`ParseError`) |
//!
//! ## Relationship to Other Systems
//!
//! - [`crate::ast`] defines the AST nodes this parser produces.
//! - The lossless tokenizer in [`crate::token`] is the single lexical definition.
//! - Parser tests in `tests/parser_tests.rs` are the authority on accepted syntax.

mod comments;
pub(crate) mod common;
mod control;
mod entry;
mod error;
pub(crate) mod expr;
pub(crate) mod inline;
pub(crate) mod stmt;
#[cfg(test)]
mod tests;
pub(crate) mod token_parser;
pub(crate) mod top_level;

use std::cell::RefCell;
use std::rc::Rc;

use chumsky::prelude::*;

use crate::ast::*;
use crate::diagnostics::Diagnostic;

use entry::parse_impl;

pub use entry::{ParseResult, ParsedSource, parse_canonical};
pub use error::ParseError;

/// Parse source and return AST, parse errors, and diagnostics (warnings).
///
/// This is the full-diagnostics entry point that includes both syntax errors
/// and semantic warnings (e.g. silently dropped brace-style properties).
pub fn parse_source_diagnostics(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<Diagnostic>) {
    let parsed = parse_impl(source);
    (parsed.ast, parsed.errors, parsed.warnings)
}

/// Parse source and return AST, errors, and parser-recorded occurrences.
pub fn parse_source_with_occurrences(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<crate::occurrence::Occurrence>) {
    let parsed = parse_impl(source);
    (parsed.ast, parsed.errors, parsed.occurrences)
}

/// Parse source and return AST, errors, parser warnings, and occurrences.
///
/// This is the entry point for consumers that surface parse-time warnings
/// (currently `BracedPropertySilentDrop`) alongside the rest of the
/// diagnostics pipeline.
pub fn parse_source_full(
    source: &str,
) -> (
    Option<Vec<Stmt>>,
    Vec<ParseError>,
    Vec<Diagnostic>,
    Vec<crate::occurrence::Occurrence>,
) {
    let parsed = parse_impl(source);
    (parsed.ast, parsed.errors, parsed.warnings, parsed.occurrences)
}

/// Parse source and return every artifact, including the token stream.
///
/// Use this when the caller needs the tokens as well: the alternative is
/// [`parse_source_full`] followed by a second `tokenize`, which lexes the
/// same source twice.
pub fn parse_source_complete(source: &str) -> ParsedSource {
    parse_impl(source)
}

/// Parse source into an AST and structured parse errors.
///
/// The tokenizer handles whitespace and comments, so this entry point accepts
/// comments in all the same positions the lexer does. Test code may call
/// [`parser_simple`] directly when warnings are not needed.
pub fn parse_source(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    let (ast, errors, _warnings) = parse_source_diagnostics(source);
    (ast, errors)
}

/// Parse source into an AST and structured parse errors without warnings.
///
/// This is a thin wrapper over [`parse_source`] kept for callers that only need
/// the AST plus errors.
pub fn parse_simple(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    parse_source(source)
}

/// Strip VS Code–style tab-stop placeholders from snippet text.
///
/// Placeholders have the form `${N:default}` or `${N}` (where N is a
/// non-negative integer).  The function returns the text with every
/// placeholder replaced by its default value (or removed if empty).
///
/// This is used by the GUI insertion palette so that snippet templates
/// can be parsed as valid `.amx` source.
pub fn strip_snippet_tabstops(snippet: &str) -> String {
    let mut result = String::with_capacity(snippet.len());
    let mut chars = snippet.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            // Skip the tab-stop number
            while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                chars.next();
            }
            // If there's a ':', collect the default text
            if chars.peek() == Some(&':') {
                chars.next(); // consume ':'
                let mut depth = 1i32;
                while let Some(&ch) = chars.peek() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            chars.next(); // consume '}'
                            break;
                        }
                    }
                    result.push(ch);
                    chars.next();
                }
            } else {
                // No default — just skip to closing '}'
                while chars.next_if(|&ch| ch != '}').is_some() {}
                chars.next(); // consume '}'
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse a snippet template into a list of statements.
///
/// The snippet text may contain VS Code–style tab-stop placeholders
/// (`${1:label}`, `${2:}`, etc.). These are stripped before parsing.
///
/// Returns `Some(stmts)` if parsing succeeds, `None` otherwise.
pub fn parse_snippet(snippet: &str) -> Option<Vec<Stmt>> {
    let cleaned = strip_snippet_tabstops(snippet);
    let (ast, _errors) = parse_source(&cleaned);
    ast
}

/// Build the top-level `.amx` file parser that discards warnings.
///
/// Convenience wrapper for tests and contexts where diagnostics are not needed.
pub fn parser_simple<'src>()
-> impl Parser<'src, token_parser::TokInput<'src>, Vec<Stmt>, token_parser::TokErr<'src>> {
    parser(Rc::new(RefCell::new(Vec::new())))
}

/// Build the top-level `.amx` file parser.
///
/// Parses a full source file into a `Vec<Stmt>`, grouping statements into scenes
/// via `group_scenes`. Accepts a shared warnings collector for emitting semantic
/// diagnostics during parsing.
pub fn parser<'src>(
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
) -> impl Parser<'src, token_parser::TokInput<'src>, Vec<Stmt>, token_parser::TokErr<'src>> {
    let expr = expr::parser();
    let property = common::property(expr.clone());
    let modifier = common::modifier(expr.clone(), common::time());
    let modifiers = common::modifiers(modifier);
    let inline_items = inline::parser(expr.clone(), property.clone(), modifiers.clone(), warnings);
    let stmt = stmt::parser(expr.clone(), property.clone(), modifiers.clone(), inline_items);
    top_level::parser(stmt, property, modifiers)
}
