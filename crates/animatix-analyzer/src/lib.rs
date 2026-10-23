#![warn(missing_docs)]

//! # animatix-analyzer
//!
//! Shared language intelligence for the Animatix DSL.
//!
//! This crate provides completions, diagnostics, hover info, and go-to-definition
//! for `.amx` files. It is consumed directly by the GUI and via LSP by external editors.
//!
//! ## Design
//!
//! - **No I/O** — all functions take `&str` or AST data, return results
//! - **Position-based** — `(line, col)` inputs matching LSP's `Position` type
//! - **Incremental** — `Analyzer::update()` re-parses only when source changes

mod analyzer;
mod completer;
mod definition;
mod diagnostics;
mod document_symbol;
mod duplicates;
mod extension_discovery;
mod extension_manifest;
mod fixes;
mod hover;
mod positions;
mod queries;
mod references;
mod rename;
mod symbol_table;
#[cfg(test)]
mod tests;
mod types;
mod unresolved;
mod workspace;

// chumsky::Parser trait is not needed directly; parser functions are called via module API.
use std::path::PathBuf;

use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::ParseError;
use animatix_syntax::token::Token;
pub use completer::{CompletionItem, CompletionKind, all_snippets, completions_at};
pub use diagnostics::{
    Diagnostic, DiagnosticSeverity, LintConfig, collect_diagnostics,
    collect_diagnostics_with_config,
};
pub use extension_discovery::{
    ManifestIssue, ManifestSource, discover_manifest_paths, discover_manifest_sources,
    fingerprint_sources, load_manifest_source,
};
pub use extension_manifest::{
    ExtensionManifest, ManifestEffect, ManifestEffectParam, ManifestPrimitive, ManifestProperty,
};
pub use fixes::{
    SpellingCandidate, batch_removal_ranges, declaration_insertion_line, first_declaration_byte,
    is_top_level_position, missing_actor_statement, statement_removal_range, suggest_action_names,
    suggest_label_names,
};
pub use rename::{RenameRejection, RenameTarget, rename_at, validate_new_name};
pub use symbol_table::{
    ComponentInfo, ImportInfo, LabelInfo, LabelKind, ParamInfo, SceneInfo, SymbolTable,
};
pub use types::{DocumentSymbol, HoverInfo, Location, SymbolKind};
pub use workspace::Workspace;

/// The main entry point for language intelligence.
///
/// Holds parsed source, AST, token stream, and extracted symbols.
/// Call `update()` when source changes; query methods are cheap.
pub struct Analyzer {
    source: String,
    path: Option<PathBuf>,
    ast: Option<Vec<Stmt>>,
    parse_errors: Vec<ParseError>,
    /// Non-fatal parse-time warnings (e.g. `BracedPropertySilentDrop`).
    /// Dropped by older parse entry points; surfaced in
    /// [`Analyzer::diagnostics_with_config`] so silent drops become visible.
    parse_warnings: Vec<animatix_syntax::diagnostics::Diagnostic>,
    tokens: Vec<Token>,
    /// Lexical issues (currently unterminated string literals) that explain
    /// otherwise-confusing downstream parse errors.
    token_issues: Vec<animatix_syntax::token::TokenIssue>,
    occurrences: Vec<animatix_syntax::occurrence::Occurrence>,
    symbols: SymbolTable,
    /// Resolved symbols from this file's imports, cached between symbol
    /// rebuilds. Populated by [`Analyzer::merge_import_symbols`]; edits to the
    /// file itself reuse the cache so per-keystroke rebuilds stay cheap.
    import_symbols: Option<SymbolTable>,
    type_diagnostics: Vec<diagnostics::Diagnostic>,
    lint_config: diagnostics::LintConfig,
    extension_manifest: ExtensionManifest,
    /// Memoized diagnostics for the current source.
    ///
    /// The GUI calls `diagnostics()` every frame on unchanged text, and the
    /// full run costs ~0.3 ms on a large file (semantic checks, duplicate
    /// detection, unresolved-name analysis). Every input is fixed once
    /// `update()` returns, so the result only varies with the lint config —
    /// which this caches by fingerprint.
    diagnostics_cache:
        std::sync::Mutex<Option<(u64, std::sync::Arc<Vec<diagnostics::Diagnostic>>)>>,
}
