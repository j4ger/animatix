use std::cell::RefCell;
use std::rc::Rc;

use chumsky::prelude::*;

use crate::ast::*;
use crate::diagnostics::Diagnostic;

use super::entry::{ParsedSource, parse_impl};
use super::error::ParseError;
use super::token_parser;
use super::{common, expr, inline, stmt, top_level};

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
