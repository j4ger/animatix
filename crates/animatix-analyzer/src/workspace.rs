//! Workspace management for cross-file analysis.

use crate::symbol_table::SymbolTable;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A workspace holds multiple files and their cross-file relationships.
///
/// Used for cross-file analysis: each file is parsed independently,
/// but imports are resolved against the workspace to provide completions,
/// hover, and go-to-definition across file boundaries.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    files: HashMap<PathBuf, FileEntry>,
}

#[derive(Debug, Clone)]
struct FileEntry {
    symbols: SymbolTable,
}

impl Workspace {
    /// Create an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a file in the workspace.
    pub fn add_file(&mut self, path: PathBuf, source: &str) {
        use animatix_syntax::parser::parser;
        use chumsky::Parser;

        let (ast, _) = parser().parse(source).into_output_errors();
        let symbols = ast
            .as_ref()
            .map(|stmts| SymbolTable::build_from_ast(stmts))
            .unwrap_or_default();
        self.files.insert(path, FileEntry { symbols });
    }

    /// Remove a file from the workspace.
    pub fn remove_file(&mut self, path: &Path) {
        self.files.remove(path);
    }

    /// Check if a file exists in the workspace.
    pub fn has_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    /// Get the symbol table for a specific file.
    pub fn file_symbols(&self, path: &Path) -> Option<SymbolTable> {
        self.files.get(path).map(|e| e.symbols.clone())
    }

    /// Resolve imports for a file and return a merged symbol table
    /// containing local symbols plus exported symbols from imported files.
    pub fn resolve_symbols(&self, path: &Path) -> SymbolTable {
        let mut merged = self
            .files
            .get(path)
            .map(|e| e.symbols.clone())
            .unwrap_or_default();

        for import in &merged.imports.clone() {
            // Try to find the imported file by path
            let import_path = Self::resolve_import_path(path, &import.path);
            if let Some(entry) = self.files.get(&import_path) {
                if import.alias.is_some() {
                    // Aliased import: symbols are accessed via alias.namespace
                    // For now, include all symbols but track the alias for qualified access
                    merged.merge(&entry.symbols);
                } else {
                    // Direct import: merge all exported symbols
                    merged.merge(&entry.symbols);
                }
            }
        }

        merged
    }

    /// Resolve an import path relative to a base path.
    pub fn resolve_import_path(base: &Path, import_path: &str) -> PathBuf {
        let trimmed = import_path.trim_matches('"');
        if let Some(parent) = base.parent() {
            parent.join(trimmed)
        } else {
            PathBuf::from(trimmed)
        }
    }
}
