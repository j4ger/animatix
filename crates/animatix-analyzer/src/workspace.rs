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
    /// pipeline; the tokenizer is used for positions.
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

    /// True when `importer` imports `target` (directly), by resolved path.
    ///
    /// Used by cross-file rename to tell an occurrence that came from an import
    /// apart from one that merely shares the name: only files that actually
    /// import the target's module should be rewritten.
    ///
    /// Reads the importer's own source rather than the module graph's cached
    /// import list: that list is captured when a file is added, so it misses
    /// imports whose target was registered later (and vice versa), which made
    /// this answer depend on the order files happened to be opened.
    pub fn imports(&self, importer: &Path, target: &Path) -> bool {
        let Some(source) = self.source(importer) else {
            return false;
        };
        let target = animatix_syntax::module::source_map::normalize_path(target);
        import_paths(source).iter().any(|import_path| {
            let resolved = resolve_import(importer, import_path);
            animatix_syntax::module::source_map::normalize_path(&resolved) == target
        })
    }
}

/// Extract the import path strings from an `.amx` source.
///
/// A light scan for `import "…"` rather than a full parse: this runs on the
/// query path and only needs the string literals.
fn import_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("import") else {
            continue;
        };
        // `import "path"` / `import "path" as alias`
        let rest = rest.trim_start();
        if !rest.starts_with('"') {
            continue;
        }
        if let Some(end) = rest[1..].find('"') {
            paths.push(rest[1..1 + end].to_string());
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn imports_is_independent_of_add_order() {
        let main = PathBuf::from("/p/main.amx");
        let lib = PathBuf::from("/p/lib.amx");
        let main_src = "import \"lib.amx\"\nbox: Rect, size: widget\n";
        let lib_src = "pub let widget = rgb(1, 0, 0)\n";

        // Importer added first (the order that used to report false).
        let mut first = Workspace::new();
        first.add_file(main.clone(), main_src);
        first.add_file(lib.clone(), lib_src);
        assert!(first.imports(&main, &lib), "importer-first must still resolve");

        // Imported file added first.
        let mut second = Workspace::new();
        second.add_file(lib.clone(), lib_src);
        second.add_file(main.clone(), main_src);
        assert!(second.imports(&main, &lib), "target-first must resolve");
    }

    #[test]
    fn imports_rejects_unrelated_and_reverse_edges() {
        let main = PathBuf::from("/p/main.amx");
        let lib = PathBuf::from("/p/lib.amx");
        let other = PathBuf::from("/p/other.amx");
        let mut w = Workspace::new();
        w.add_file(main.clone(), "import \"lib.amx\"\nbox: Rect, size: widget\n");
        w.add_file(lib.clone(), "pub let widget = rgb(1, 0, 0)\n");
        w.add_file(other.clone(), "box2: Rect, size: widget\n");

        assert!(!w.imports(&other, &lib), "a file with no import edge is unrelated");
        assert!(!w.imports(&lib, &main), "import edges are directed");
    }

    #[test]
    fn imports_handles_aliased_and_relative_paths() {
        let main = PathBuf::from("/p/sub/main.amx");
        let lib = PathBuf::from("/p/lib.amx");
        let mut w = Workspace::new();
        w.add_file(main.clone(), "import \"../lib.amx\" as l\nuse = l.widget\n");
        w.add_file(lib.clone(), "pub let widget = rgb(1, 0, 0)\n");
        assert!(w.imports(&main, &lib), "relative alias import resolves");
    }
}
