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
//! | `api` | Public parse entry points and the assembled file parser |
//!
//! ## Relationship to Other Systems
//!
//! - [`crate::ast`] defines the AST nodes this parser produces.
//! - The lossless tokenizer in [`crate::token`] is the single lexical definition.
//! - The parser test corpus lives in `crates/animatix-syntax/src/parser/tests.rs`;
//!   it is the authority on accepted syntax.

mod api;
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

// The parser test corpus (`tests.rs`) pulls its AST vocabulary in through
// `use super::*`; rustc's unused-import analysis does not follow that glob
// chain, so these reads look unused on their own.
#[allow(unused_imports)]
use chumsky::prelude::*;
#[allow(unused_imports)]
use std::cell::RefCell;
#[allow(unused_imports)]
use std::rc::Rc;

#[allow(unused_imports)]
use crate::ast::*;
#[allow(unused_imports)]
use crate::diagnostics::Diagnostic;

pub use api::{
    parse_snippet, parse_source, parse_source_complete, parse_source_with_occurrences, parser,
    strip_snippet_tabstops,
};
pub use entry::ParsedSource;
pub use error::ParseError;
