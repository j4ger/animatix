//! Workspace management for cross-file analysis.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::symbol_table::SymbolTable;

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
        use animatix_syntax::parser::parse_source;

        let (ast, _) = parse_source(source);
        let mut symbols =
            ast.as_ref().map(|stmts| SymbolTable::build_from_ast(stmts)).unwrap_or_default();
        if let Some(ref stmts) = ast {
            symbols.collect_references(stmts);
        }
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
        let mut visited = HashSet::new();
        self.resolve_symbols_inner(path, &mut visited)
    }

    fn resolve_symbols_inner(&self, path: &Path, visited: &mut HashSet<PathBuf>) -> SymbolTable {
        let mut merged = self.files.get(path).map(|e| e.symbols.clone()).unwrap_or_default();
        if !visited.insert(path.to_path_buf()) {
            return merged;
        }

        for import in merged.imports.clone() {
            // Try to find the imported file by path
            let import_path = Self::resolve_import_path(path, &import.path);
            if !self.files.contains_key(&import_path) {
                continue;
            }
            let resolved = self.resolve_symbols_inner(&import_path, visited);
            if let Some(ref alias) = import.alias {
                // Aliased import: store symbols under namespace for qualified access.
                merged.namespaces.insert(alias.clone(), resolved);
            } else {
                // Direct import: merge all exported symbols
                merged.merge(&resolved);
            }
        }

        visited.remove(path);
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
