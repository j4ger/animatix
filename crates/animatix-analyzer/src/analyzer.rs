//! Analyzer lifecycle: construction, source updates, symbol rebuilds, and state access.

use std::path::{Path, PathBuf};

use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::ParseError;

use crate::Analyzer;
use crate::diagnostics;
use crate::extension_manifest::ExtensionManifest;
use crate::symbol_table::SymbolTable;

impl Analyzer {
    /// Create a new analyzer from source text.
    pub fn new(source: &str) -> Self {
        Self::new_with_path(source, None)
    }

    /// Create a new analyzer with an associated file path.
    pub fn new_with_path(source: &str, path: Option<PathBuf>) -> Self {
        Self::new_with_path_and_manifest(source, path, ExtensionManifest::default())
    }

    /// Create a new analyzer with a file path and extension metadata already
    /// installed, so construction performs exactly one symbol-table rebuild.
    pub fn new_with_path_and_manifest(
        source: &str,
        path: Option<PathBuf>,
        manifest: ExtensionManifest,
    ) -> Self {
        let mut analyzer = Self {
            source: String::new(),
            path,
            ast: None,
            parse_errors: Vec::new(),
            parse_warnings: Vec::new(),
            tokens: Vec::new(),
            token_issues: Vec::new(),
            occurrences: Vec::new(),
            symbols: SymbolTable::default(),
            import_symbols: None,
            type_diagnostics: Vec::new(),
            lint_config: diagnostics::LintConfig::default(),
            extension_manifest: manifest,
            diagnostics_cache: std::sync::Mutex::new(None),
            rebuild_count: 0,
        };
        analyzer.update(source);
        analyzer
    }

    /// Set extension metadata used by completions and diagnostics.
    pub fn with_extension_manifest(mut self, manifest: ExtensionManifest) -> Self {
        self.set_extension_manifest(manifest);
        self
    }

    /// Replace extension metadata without re-parsing source.
    ///
    /// Applying an identical manifest is a no-op: callers such as the LSP
    /// keystroke path may hold a freshly merged (but equal) manifest and must
    /// not pay for a second rebuild.
    pub fn set_extension_manifest(&mut self, manifest: ExtensionManifest) {
        if self.extension_manifest == manifest {
            return;
        }
        self.extension_manifest = manifest;
        self.rebuild_symbols();
    }

    /// Replace the source text and extension metadata together, rebuilding at
    /// most once.
    ///
    /// This is the per-keystroke entry point: when both the text and the
    /// merged manifest changed, parsing happens once and the symbol table is
    /// built once against the new manifest. When neither changed, it is a
    /// no-op.
    pub fn update_with_manifest(&mut self, source: &str, manifest: ExtensionManifest) {
        let source_changed = self.source != source;
        let manifest_changed = self.extension_manifest != manifest;
        if !source_changed && !manifest_changed {
            return;
        }
        if source_changed {
            self.source = source.to_string();
            self.parse_source(source);
        }
        self.extension_manifest = manifest;
        self.rebuild_symbols();
    }

    /// Merge symbols resolved from this file's imports into the symbol table.
    ///
    /// The per-file table only knows the file's own definitions, so semantic
    /// diagnostics (e.g. `unknown-type`) would flag instances of imported
    /// `pub component` types. This loads the module graph for the analyzer's
    /// file path, caches the resolved table, and merges it (local definitions
    /// win). Later [`Analyzer::update`] / `set_extension_manifest` rebuilds
    /// reuse the cache, so editors can call this once per opened file without
    /// per-keystroke disk reads. Best effort: silently does nothing without a
    /// path or when imports fail to resolve. Call
    /// [`Analyzer::clear_import_symbols`] when known imports change on disk.
    pub fn merge_import_symbols(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let mut graph = animatix_syntax::module::ModuleGraph::new();
        if graph.load_program(&path).is_err() {
            return;
        }
        self.import_symbols = Some(graph.resolve_symbols(&path));
        self.rebuild_symbols();
    }

    /// Drop the cached import symbols (e.g. when an imported file changed on
    /// disk) and rebuild without them.
    pub fn clear_import_symbols(&mut self) {
        self.import_symbols = None;
        self.rebuild_symbols();
    }

    /// Update the source text. Re-parses and rebuilds only if it changed.
    pub fn update(&mut self, source: &str) {
        let manifest = self.extension_manifest.clone();
        self.update_with_manifest(source, manifest);
    }

    /// Parse source once and keep the token stream it produced.
    ///
    /// Tokenizing again in `rebuild_symbols` would lex the same source a
    /// second time (the lexer is ~4% of a keystroke rebuild, and the tokens
    /// are needed for hover/completion/highlighting queries anyway).
    fn parse_source(&mut self, source: &str) {
        let parsed = animatix_syntax::parser::parse_source_complete(source);
        self.ast = parsed.ast;
        self.parse_errors = parsed.errors;
        self.parse_warnings = parsed.warnings;
        self.occurrences = parsed.occurrences;
        self.tokens = parsed.tokens;
        self.token_issues = parsed.token_issues;
    }

    /// Rebuild the symbol table from the current source and occurrence stream.
    ///
    /// **Traversal budget: three AST walks per edit.** This is the per-keystroke
    /// hot path, so keep it that way:
    ///
    /// 1. `SymbolTable::build` declaration pass (labels, components, scenes).
    /// 2. `SymbolTable::build` reference pass (`referenced_labels`).
    /// 3. `TypeEnv::check_statements` (type diagnostics; its alias registration
    ///    is top-level only).
    ///
    /// The component registry is read from the definitions collected in pass 1
    /// instead of re-walking the AST, and `enrich_positions` reads occurrences
    /// and tokens only. Adding another walk here needs a measurement.
    fn rebuild_symbols(&mut self) {
        let source = &self.source;
        let occurrences = &self.occurrences;
        // One line index serves every position conversion in this rebuild:
        // declaration enrichment, syntax diagnostics, and later queries that
        // receive the resulting spans. Building it per declaration made the
        // per-keystroke path O(n²) on declaration-heavy files.
        let line_index = animatix_syntax::token::LineIndex::new(source);

        // Build the symbol table: one declaration pass, one reference pass.
        let table = if let Some(ref stmts) = self.ast {
            let mut table = SymbolTable::build(stmts);
            Self::enrich_positions(occurrences, &self.tokens, &line_index, &mut table);
            table
        } else {
            SymbolTable::default()
        };

        // Cache lint config from source comments
        self.lint_config = diagnostics::LintConfig::from_source(source);

        // Run type checker
        self.type_diagnostics = if let Some(ref stmts) = self.ast {
            let components = Self::build_component_registry(&table);
            let module_fns = std::collections::HashMap::new();
            let mut env = animatix_syntax::typecheck::TypeEnv::new(&components, &module_fns);
            let syntax_diagnostics = env.check_statements(stmts);
            syntax_diagnostics
                .into_iter()
                .map(|d| diagnostics::convert_syntax_diagnostic_with_index(source, &line_index, d))
                .collect()
        } else {
            Vec::new()
        };

        let mut table = table;
        // Re-merge cached import symbols so rebuilds triggered by edits or
        // manifest updates keep the cross-file view.
        if let Some(resolved) = &self.import_symbols {
            table.merge(resolved);
        }
        self.extension_manifest.apply_to(&mut table);

        self.symbols = table;

        // Every caller of this function changes something the diagnostics
        // depend on (source, manifest, or imported symbols), so invalidating
        // here covers the whole state-change surface: `update`,
        // `update_with_manifest`, `merge_import_symbols`,
        // `clear_import_symbols`, and `set_extension_manifest` all route
        // through it.
        *self.diagnostics_cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.rebuild_count += 1;
    }

    /// Number of symbol-table rebuilds performed since construction.
    ///
    /// Small test hook used to guarantee one rebuild per source/manifest
    /// change; callers outside tests should not depend on the exact count.
    #[doc(hidden)]
    pub fn rebuild_count(&self) -> u64 {
        self.rebuild_count
    }

    /// Build the type checker's component registry from the declarations the
    /// symbol-table pass already collected.
    ///
    /// Reading [`SymbolTable::component_defs`] instead of walking the AST keeps
    /// the component registry free: the definitions were cloned once during
    /// declaration collection.
    fn build_component_registry(
        table: &SymbolTable,
    ) -> std::collections::HashMap<String, animatix_syntax::module::ComponentEntry> {
        use std::collections::HashMap;

        use animatix_syntax::module::ComponentEntry;

        let mut registry = HashMap::new();
        for (name, def) in &table.component_defs {
            let actions = animatix_syntax::module::discovery::collect_component_actions(def);
            registry.insert(
                name.clone(),
                ComponentEntry {
                    definition: def.clone(),
                    source_path: std::path::PathBuf::new(),
                    actions,
                },
            );
        }
        registry
    }

    /// Get the file path, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Get the current source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the parsed AST, if parsing succeeded.
    pub fn ast(&self) -> Option<&[Stmt]> {
        self.ast.as_deref()
    }

    /// Get the extracted symbol table.
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// Get the parser-recorded identifier occurrences.
    pub fn occurrences(&self) -> &[animatix_syntax::occurrence::Occurrence] {
        &self.occurrences
    }

    /// Get structured parse errors with position information.
    pub fn parse_errors(&self) -> &[ParseError] {
        &self.parse_errors
    }
}
