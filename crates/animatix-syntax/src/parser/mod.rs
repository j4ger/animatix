//!
//! # Animatix Parser
//!
//! Combinator-based recursive descent parser using [`chumsky`]. This parser is the
//! executable source of truth for accepted `.amx` syntax.
//!
//! ## Entry Points
//!
//! Source parsing has exactly three entry points; each returns more of the
//! same parse instead of re-lexing:
//!
//! - [`parse_source`] — AST plus structured errors
//! - [`parse_source_with_occurrences`] — the same plus identifier occurrences
//! - [`parse_source_complete`] — every artifact, including warnings and the
//!   token stream
//!
//! [`parse_snippet`] and [`parser()`] are composition helpers, not additional
//! source-parsing pipelines.
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
//! | `entry` | Parse entry implementation (`ParsedSource` and the tokenizer-once pipeline) |
//! | `error` | Structured parse errors (`ParseError`) |
//!
//! ## Relationship to Other Systems
//!
//! - [`crate::ast`] defines the AST nodes this parser produces.
//! - The lossless tokenizer in [`crate::token`] is the single lexical definition.
//! - The parser test corpus lives in `crates/animatix/tests/parser_tests/` and
//!   `crates/animatix-syntax/src/parser/tests.rs`; it is the authority on
//!   accepted syntax.

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

pub use entry::ParsedSource;
pub use error::ParseError;

/// Parse source and return AST, errors, and parser-recorded occurrences.
pub fn parse_source_with_occurrences(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<crate::occurrence::Occurrence>) {
    let parsed = parse_impl(source);
    (parsed.ast, parsed.errors, parsed.occurrences)
}

/// Parse source and return every artifact, including the token stream.
///
/// Use this when the caller needs the tokens as well: the alternative is
/// [`parse_source`] followed by a second `tokenize`, which lexes the same
/// source twice.
pub fn parse_source_complete(source: &str) -> ParsedSource {
    parse_impl(source)
}

/// Parse source into an AST and structured parse errors.
///
/// The tokenizer handles whitespace and comments, so this entry point accepts
/// comments in all the same positions the lexer does. Diagnostics-free: use
/// [`parse_source_complete`] when the caller also needs warnings, occurrences,
/// or the token stream.
pub fn parse_source(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    let parsed = parse_impl(source);
    (parsed.ast, parsed.errors)
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
