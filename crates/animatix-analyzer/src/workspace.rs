//! Workspace management for cross-file analysis.
//!
//! This is a thin facade over [`animatix_syntax::module::ModuleGraph`] in
//! in-memory (`SourcesOnly`) mode. Parsing, symbol extraction, import identity,
//! and namespace resolution all come from the shared module graph so the
//! analyzer/LSP and runtime/module pipelines cannot drift.

use std::path::{Path, PathBuf};

use animatix_syntax::module::source_map::resolve_import;
use animatix_syntax::module::{ModuleGraph, SourceAccess};

use crate::symbol_table::SymbolTable;

/// A workspace holds multiple files and their cross-file relationships.
///
/// Used for cross-file analysis: each file is parsed independently,
/// but imports are resolved against the workspace to provide completions,
/// hover, and go-to-definition across file boundaries.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    graph: ModuleGraph,
}

impl Workspace {
    /// Create an empty workspace.
    pub fn new() -> Self {
        Self {
            graph: ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly),
        }
    }

    /// Add or update a file in the workspace.
    ///
    /// Symbols are built by the shared canonical semantic parser in
    /// `ModuleGraph`, so cross-file analysis agrees with the runtime/module
    /// pipeline; tree-sitter is not used here.
    pub fn add_file(&mut self, path: PathBuf, source: &str) {
        self.graph.upsert_source(path.clone(), source.to_string());
        // Parse the file even when an import is missing so local symbols stay
        // available. Best-effort load then resolves imports when possible.
        let _ = self.graph.load_file_standalone(&path);
        let _ = self.graph.load_program(&path);
    }

    /// Remove a file from the workspace.
    pub fn remove_file(&mut self, path: &Path) {
        self.graph.remove_source_for_path(path);
    }

    /// Check if a file exists in the workspace.
    pub fn has_file(&self, path: &Path) -> bool {
        self.graph.file_symbols(path).is_some()
    }

    /// Get the symbol table for a specific file.
    pub fn file_symbols(&self, path: &Path) -> Option<SymbolTable> {
        self.graph.file_symbols(path)
    }

    /// Resolve imports for a file and return a merged symbol table
    /// containing local symbols plus exported symbols from imported files.
    pub fn resolve_symbols(&self, path: &Path) -> SymbolTable {
        self.graph.resolve_symbols(path)
    }

    /// Resolve an import path relative to a base file path.
    ///
    /// Delegates to the shared source-map path resolver so analyzer and module
    /// loading agree on file identity.
    pub fn resolve_import_path(base: &Path, import_path: &str) -> PathBuf {
        resolve_import(base, import_path)
    }

    /// Return the in-memory source for a path, if any.
    pub fn source(&self, path: &Path) -> Option<&str> {
        self.graph.source(path)
    }
}
