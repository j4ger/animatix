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

mod completer;
mod definition;
mod diagnostics;
mod document_symbol;
mod hover;
mod references;
mod symbol_table;
mod types;
mod workspace;

// chumsky::Parser trait is not needed directly; parser functions are called via module API.
use std::path::{Path, PathBuf};

use animatix_syntax::ast::{Span, Stmt};
use animatix_syntax::parser::ParseError;
use animatix_syntax::token::{Token, TokenKind};
pub use completer::{CompletionItem, CompletionKind, all_snippets, completions_at};
pub use diagnostics::{
    Diagnostic, DiagnosticSeverity, LintConfig, collect_diagnostics,
    collect_diagnostics_with_config,
};
pub use symbol_table::{
    ComponentInfo, ImportInfo, LabelInfo, LabelKind, ParamInfo, SceneInfo, SymbolTable,
};
pub use types::{DocumentSymbol, HoverInfo, Location, SymbolKind};
pub use workspace::Workspace;

/// The main entry point for language intelligence.
///
/// Holds parsed source, AST, tree-sitter tree, and extracted symbols.
/// Call `update()` when source changes; query methods are cheap.
pub struct Analyzer {
    source: String,
    path: Option<PathBuf>,
    ast: Option<Vec<Stmt>>,
    parse_errors: Vec<ParseError>,
    tokens: Vec<Token>,
    symbols: SymbolTable,
    type_diagnostics: Vec<diagnostics::Diagnostic>,
    lint_config: diagnostics::LintConfig,
}

impl Analyzer {
    /// Create a new analyzer from source text.
    pub fn new(source: &str) -> Self {
        Self::new_with_path(source, None)
    }

    /// Create a new analyzer with an associated file path.
    pub fn new_with_path(source: &str, path: Option<PathBuf>) -> Self {
        let mut analyzer = Self {
            source: String::new(),
            path,
            ast: None,
            parse_errors: Vec::new(),
            tokens: Vec::new(),
            symbols: SymbolTable::default(),
            type_diagnostics: Vec::new(),
            lint_config: diagnostics::LintConfig::default(),
        };
        analyzer.update(source);
        analyzer
    }

    /// Update the source text. Re-parses if changed.
    pub fn update(&mut self, source: &str) {
        if self.source == source {
            return;
        }

        self.source = source.to_string();

        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        self.ast = ast;
        self.parse_errors = parse_errors;
        self.tokens = animatix_syntax::token::tokenize(source);

        self.rebuild_symbols();
    }

    /// Rebuild the symbol table from the current source and token stream.
    fn rebuild_symbols(&mut self) {
        let source = &self.source;
        let tokens = &self.tokens;

        // Build symbol table from AST
        let table = if let Some(ref stmts) = self.ast {
            let mut table = SymbolTable::build_from_ast(stmts);
            table.collect_references(stmts);
            Self::enrich_positions(tokens, source, &mut table);
            table
        } else {
            SymbolTable::default()
        };

        // Cache lint config from source comments
        self.lint_config = diagnostics::LintConfig::from_source(source);

        // Run type checker
        self.type_diagnostics = if let Some(ref stmts) = self.ast {
            let components = Self::build_component_registry(stmts);
            let module_actions = std::collections::HashMap::new();
            let mut env = animatix_syntax::typecheck::TypeEnv::new(&components, &module_actions);
            let syntax_diagnostics = env.check_statements(stmts);
            syntax_diagnostics.into_iter().map(Self::convert_type_diagnostic).collect()
        } else {
            Vec::new()
        };

        self.symbols = table;
    }

    /// Build a component registry from AST statements for the type checker.
    fn build_component_registry(
        stmts: &[Stmt],
    ) -> std::collections::HashMap<String, animatix_syntax::module::ComponentEntry> {
        use std::collections::HashMap;

        use animatix_syntax::module::ComponentEntry;

        let mut registry = HashMap::new();
        for def in animatix_syntax::module::discovery::collect_component_defs(stmts) {
            let actions = animatix_syntax::module::discovery::collect_component_actions(&def);
            registry.insert(
                def.name.clone(),
                ComponentEntry {
                    definition: def,
                    source_path: std::path::PathBuf::new(),
                    actions,
                },
            );
        }
        registry
    }

    /// Convert a syntax-level diagnostic to an analyzer diagnostic.
    fn convert_type_diagnostic(
        d: animatix_syntax::diagnostics::Diagnostic,
    ) -> diagnostics::Diagnostic {
        let severity = match d.severity {
            animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                diagnostics::DiagnosticSeverity::Error
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                diagnostics::DiagnosticSeverity::Warning
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Info => {
                diagnostics::DiagnosticSeverity::Info
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                diagnostics::DiagnosticSeverity::Hint
            },
        };
        let line = d.location.line.unwrap_or(1).saturating_sub(1);
        let col = d.location.column.unwrap_or(1).saturating_sub(1);
        diagnostics::Diagnostic {
            severity,
            line,
            col,
            end_line: line,
            end_col: col + 1,
            message: d.message,
            code: Some(d.code.to_string()),
        }
    }

    /// Populate symbol table entries with real line/col positions from the
    /// lossless token stream. The first matching identifier token is treated as
    /// the declaration position, matching the previous tree-sitter walk.
    fn enrich_positions(tokens: &[Token], source: &str, table: &mut SymbolTable) {
        for token in tokens {
            if let TokenKind::Ident(name) = &token.kind {
                let span = Span::from_byte_span(source, token.span);
                if let Some(info) = table.labels.get_mut(name) {
                    if info.line == 0 && info.col == 0 {
                        info.line = span.start_line;
                        info.col = span.start_col;
                        info.span = Some(span);
                    }
                }
                if let Some(info) = table.components.get_mut(name) {
                    if info.line == 0 && info.col == 0 {
                        info.line = span.start_line;
                        info.col = span.start_col;
                        info.span = Some(span);
                    }
                }
                if let Some(info) = table.scenes.get_mut(name) {
                    if info.line == 0 && info.col == 0 {
                        info.line = span.start_line;
                        info.col = span.start_col;
                        info.span = Some(span);
                    }
                }
            }

            if let TokenKind::Str(path) = &token.kind {
                let span = Span::from_byte_span(source, token.span);
                for info in &mut table.imports {
                    if &info.path == path && info.span.is_none() {
                        info.span = Some(span);
                        break;
                    }
                }
            }
        }
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

    /// Get structured parse errors with position information.
    pub fn parse_errors(&self) -> &[ParseError] {
        &self.parse_errors
    }

    /// Completions at cursor position.
    pub fn completions_at(&self, line: usize, col: usize) -> Vec<CompletionItem> {
        completer::completions_at(&self.symbols, &self.tokens, &self.source, line, col)
    }

    /// All diagnostics (parse errors + semantic checks).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics_with_config(&self.lint_config)
    }

    /// All diagnostics with explicit lint configuration.
    pub fn diagnostics_with_config(&self, config: &diagnostics::LintConfig) -> Vec<Diagnostic> {
        let mut diagnostics = diagnostics::collect_diagnostics_with_config(
            &self.source,
            &self.parse_errors,
            &self.symbols,
            self.ast.as_deref(),
            &self.tokens,
            config,
        );
        diagnostics.extend_from_slice(&self.type_diagnostics);
        diagnostics
    }

    /// Hover information at cursor position.
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo> {
        hover::hover_at(&self.symbols, &self.tokens, &self.source, line, col)
    }

    /// Go-to-definition at cursor position.
    ///
    /// Pass `workspace` for cross-file definition lookup.
    pub fn definition_at(
        &self,
        workspace: Option<&Workspace>,
        line: usize,
        col: usize,
    ) -> Option<Location> {
        definition::definition_at(
            &self.symbols,
            &self.tokens,
            &self.source,
            workspace,
            self.path.as_deref(),
            line,
            col,
        )
    }

    /// Find all references to a symbol name in this file.
    /// Returns a list of (start_line, start_col, end_line, end_col) ranges.
    pub fn find_references(&self, symbol_name: &str) -> Vec<(usize, usize, usize, usize)> {
        references::find_references(&self.tokens, &self.source, symbol_name)
    }

    /// Document symbols (outline view).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        document_symbol::document_symbols(&self.symbols)
    }

    /// Get the symbol name at a cursor position.
    /// Returns the identifier or type name at the given line/column,
    /// or None if the position is not on a symbol.
    pub fn symbol_at(&self, line: usize, col: usize) -> Option<String> {
        let byte = animatix_syntax::token::line_col_to_byte(&self.source, line, col);
        let token = animatix_syntax::token::token_at_byte(&self.tokens, byte)?;
        match &token.kind {
            TokenKind::Ident(name) => Some(name.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzer_parses_valid_source() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
    position: (400, 300),
}
"#;
        let analyzer = Analyzer::new(source);
        assert!(analyzer.ast().is_some());
        assert!(analyzer.parse_errors().is_empty());
    }

    #[test]
    fn analyzer_handles_parse_errors() {
        let source = "this is not valid @@@ syntax";
        let analyzer = Analyzer::new(source);
        assert!(!analyzer.parse_errors().is_empty());
    }

    #[test]
    fn transform_is_known_actor_property() {
        let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
a: Rect, size: (100, 100), transform: (1, 0.5, 0, 1, 0, 0), color: accent.primary, at: (200, 150)
"#;
        let analyzer = Analyzer::new(source);

        let unknown_properties: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unknown-property"))
            .collect();
        assert!(
            unknown_properties.is_empty(),
            "transform should be a known actor property: {unknown_properties:?}"
        );
    }

    #[test]
    fn analyzer_extracts_symbols() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
btn: Button {
    text: "Click",
}
"#;
        let analyzer = Analyzer::new(source);
        let symbols = analyzer.symbols();

        assert!(symbols.labels.contains_key("title"));
        assert!(symbols.labels.contains_key("btn"));
        assert_eq!(symbols.labels["title"].ty.as_deref(), Some("Text"));
        assert_eq!(symbols.labels["btn"].ty.as_deref(), Some("Button"));
    }

    #[test]
    fn analyzer_update_is_noop_on_same_source() {
        let source = "title: Text {}";
        let mut analyzer = Analyzer::new(source);
        let symbols_before = analyzer.symbols().labels.len();

        analyzer.update(source);
        assert_eq!(analyzer.symbols().labels.len(), symbols_before);
    }

    #[test]
    fn workspace_resolves_cross_file_symbols() {
        let mut workspace = Workspace::new();

        let lib_source = r#"
let accent = rgb(255, 0, 0)
btn: Button {
    text: "Click"
}
"#;
        let main_source = r#"
import "lib.amx"

title: Text {
    content: "Hello"
}
"#;

        workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
        workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));

        assert!(resolved.labels.contains_key("title"));
        assert!(resolved.labels.contains_key("accent"));
        assert!(resolved.labels.contains_key("btn"));
    }

    #[test]
    fn analyzer_with_workspace_provides_cross_file_completions() {
        let mut workspace = Workspace::new();

        let lib_source = r#"
let shared_color = rgb(0, 255, 0)
"#;
        let main_source = r#"
import "lib.amx"

title: Text {
    content: "Hello"
}
"#;

        workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
        workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

        assert!(workspace.has_file(Path::new("/project/lib.amx")));

        let import_path = Workspace::resolve_import_path(Path::new("/project/main.amx"), "lib.amx");
        assert_eq!(import_path, PathBuf::from("/project/lib.amx"));

        let lib_symbols = workspace.file_symbols(Path::new("/project/lib.amx")).unwrap();
        assert!(lib_symbols.labels.contains_key("shared_color"), "lib should have shared_color ");

        let analyzer =
            Analyzer::new_with_path(main_source, Some(PathBuf::from("/project/main.amx")));

        // Verify local symbols are available
        let symbols = analyzer.symbols();
        assert!(symbols.labels.contains_key("title"), "main should have title from local source");

        // Verify workspace resolves cross-file symbols
        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
        assert!(
            resolved.labels.contains_key("shared_color"),
            "workspace should resolve shared_color from import"
        );
    }

    #[test]
    fn workspace_resolves_aliased_imports_to_namespaces() {
        let mut workspace = Workspace::new();

        let lib_source = r#"
pub let accent = rgb(255, 0, 0)
pub component Button(text: "Click") {}
"#;
        let main_source = r#"
import "lib.amx" as lib

title: Text {
    content: "Hello"
}
"#;

        workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
        workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));

        // Local symbols should be in the global namespace
        assert!(resolved.labels.contains_key("title"));

        // Imported symbols should NOT be in the global namespace (aliased)
        assert!(!resolved.labels.contains_key("accent"));

        // Aliased imports expose only pub lets, pub components, pub types, scenes.
        let lib_ns = &resolved.namespaces["lib"];
        assert!(lib_ns.labels.contains_key("accent"));
        assert!(lib_ns.components.contains_key("Button"));
        assert!(!lib_ns.labels.contains_key("btn"));

        // Namespace-qualified lookup should work for exports only.
        assert!(resolved.resolve_namespaced_label("lib.accent").is_some());
        assert!(resolved.resolve_namespaced_component("lib.Button").is_some());
        assert!(resolved.resolve_namespaced_label("lib.btn").is_none());
        assert!(resolved.resolve_namespaced_label("lib.unknown").is_none());
    }

    #[test]
    fn workspace_aliased_namespace_only_exposes_pub_symbols() {
        let mut workspace = Workspace::new();

        let lib_source = r#"
let private_value = 1
pub let shared_value = 2
component InternalCard {}
pub component ExternalCard {}
"#;
        let main_source = r#"
import "lib.amx" as lib

title: Text {
    content: "Hello"
}
"#;

        workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
        workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
        let lib_ns = &resolved.namespaces["lib"];

        assert!(lib_ns.labels.contains_key("shared_value"));
        assert!(!lib_ns.labels.contains_key("private_value"));
        assert!(lib_ns.components.contains_key("ExternalCard"));
        assert!(!lib_ns.components.contains_key("InternalCard"));
    }

    #[test]
    fn workspace_resolves_nested_namespace_depth() {
        let mut workspace = Workspace::new();

        let inner_source = r#"
pub let accent = rgb(255, 0, 0)
pub component InnerButton {
    frame: Rect
}
"#;
        let lib_source = r#"
import "inner.amx" as inner
"#;
        let main_source = r#"
import "lib.amx" as lib
"#;

        workspace.add_file(PathBuf::from("/project/inner.amx"), inner_source);
        workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
        workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
        assert!(resolved.resolve_namespaced_label("lib.inner.accent").is_some());
        assert!(resolved.resolve_namespaced_component("lib.inner.InnerButton").is_some());
        assert!(resolved.resolve_namespaced_label("lib.inner.missing").is_none());
        let mut labels = resolved.namespace_labels("lib.inner");
        labels.sort();
        assert_eq!(labels, vec!["accent"], "component-internal labels should not be exported");
        assert_eq!(resolved.namespace_components("lib.inner"), vec!["InnerButton"]);
    }

    #[test]
    fn workspace_import_path_resolution() {
        let base = Path::new("/project/src/main.amx");
        let resolved = Workspace::resolve_import_path(base, "../lib.amx");
        assert!(resolved.to_string_lossy().contains("lib.amx"));
        assert_eq!(
            Workspace::resolve_import_path(base, "utils.amx"),
            PathBuf::from("/project/src/utils.amx")
        );
    }

    // ── Hover tests ──

    #[test]
    fn hover_on_actor_label_returns_label_info() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // "title" at line 2, col 0 (0-based, after leading newline)
        let info = analyzer.hover_at(2, 0);
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(info.contents.contains("**Actor**"));
        assert!(info.contents.contains("title"));
        assert!(info.contents.contains("Text"));
        assert!(info.range.is_some());
    }

    #[test]
    fn hover_on_type_returns_type_documentation() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // "Text" at line 2, col 7
        let info = analyzer.hover_at(2, 7);
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(info.contents.contains("**Type**"));
        assert!(info.contents.contains("Text"));
        assert!(info.contents.contains("element"));
    }

    #[test]
    fn hover_builtin_actions_in_symbol_table() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);
        let symbols = analyzer.symbols();
        assert!(symbols.actions.contains("fade-in"));
        assert!(symbols.actions.contains("move"));
        assert!(symbols.actions.contains("rotate"));
    }

    // ── Go-to-definition tests ──

    #[test]
    fn definition_at_returns_location_for_label() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
always {
    title.content = "World"
}
"#;
        let analyzer = Analyzer::new(source);

        // Click on "title" in always block (line 6, col 4)
        let loc = analyzer.definition_at(None, 6, 4);
        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert!(loc.file.is_none(), "definition in same file ");
        // Declaration "title:" on line 2 (0-based): enrich_positions sets line to 2+1=3
        assert_eq!(loc.line, 3);
        assert_eq!(loc.col, 1);
    }

    #[test]
    fn definition_at_on_colon_returns_none() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // Position on colon ':' at line 2, col 5
        let loc = analyzer.definition_at(None, 2, 5);
        assert!(loc.is_none());
    }

    #[test]
    fn definition_at_out_of_bounds_returns_none() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        let loc = analyzer.definition_at(None, 999, 999);
        assert!(loc.is_none());
    }

    // ── Find references tests ──

    #[test]
    fn find_references_finds_all_occurrences() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
always {
    title.content = "World"
}
"#;
        let analyzer = Analyzer::new(source);

        let refs = analyzer.find_references("title");
        // At least 2: declaration (line 2) + reference (line 6)
        assert!(refs.len() >= 2, "expected >= 2 references for title, got {}", refs.len());

        for (sl, _sc, el, _ec) in &refs {
            assert!(*sl <= *el);
        }
    }

    #[test]
    fn find_references_empty_for_unknown() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        let refs = analyzer.find_references("nonexistent");
        assert!(refs.is_empty());
    }

    #[test]
    fn find_references_empty_for_empty_source() {
        let analyzer = Analyzer::new("");
        let refs = analyzer.find_references("anything");
        assert!(refs.is_empty());
    }

    // ── Document symbols tests ──

    #[test]
    fn document_symbols_returns_outline() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
btn: Button {
    text: "Click",
}
"#;
        let analyzer = Analyzer::new(source);
        let symbols = analyzer.document_symbols();

        assert!(symbols.iter().any(|s| s.name == "title"));
        assert!(symbols.iter().any(|s| s.name == "btn"));

        let title_sym = symbols.iter().find(|s| s.name == "title").unwrap();
        assert_eq!(title_sym.kind, SymbolKind::Actor);
        assert_eq!(title_sym.detail.as_deref(), Some("Text"));

        assert!(title_sym.line > 0);
        assert!(title_sym.col > 0);
    }

    #[test]
    fn document_symbols_includes_labels() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
# 1s
btn: Button {
    text: "Click",
}
"#;
        let analyzer = Analyzer::new(source);
        let symbols = analyzer.document_symbols();

        // Should include labels
        assert!(symbols.iter().any(|s| s.name == "title"));
        assert!(symbols.iter().any(|s| s.name == "btn"));

        let title_sym = symbols.iter().find(|s| s.name == "title").unwrap();
        assert_eq!(title_sym.kind, SymbolKind::Actor);
    }

    #[test]
    fn document_symbols_sorted_by_line() {
        let source = r#"
# 0s
z_actor: Text { content: "Z", }
a_actor: Text { content: "A", }
"#;
        let analyzer = Analyzer::new(source);
        let symbols = analyzer.document_symbols();

        for i in 1..symbols.len() {
            assert!(
                symbols[i - 1].line <= symbols[i].line,
                "sort order: {} (L{}) before {} (L{})",
                symbols[i - 1].name,
                symbols[i - 1].line,
                symbols[i].name,
                symbols[i].line,
            );
        }
    }

    // ── Symbol at position tests ──

    #[test]
    fn symbol_at_returns_label_name() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // "title" at line 2, col 0
        let sym = analyzer.symbol_at(2, 0);
        assert_eq!(sym.as_deref(), Some("title"));
    }

    #[test]
    fn symbol_at_returns_type_name() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // "Text" at line 2, col 7
        let sym = analyzer.symbol_at(2, 7);
        assert_eq!(sym.as_deref(), Some("Text"));
    }

    #[test]
    fn symbol_at_on_colon_returns_none() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // Position on colon at line 2, col 5
        let sym = analyzer.symbol_at(2, 5);
        assert!(sym.is_none());
    }

    #[test]
    fn symbol_at_on_whitespace_returns_none() {
        let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
        let analyzer = Analyzer::new(source);

        // Position on space between colon and Text at line 2, col 6
        let sym = analyzer.symbol_at(2, 6);
        assert!(sym.is_none());
    }

    #[test]
    fn symbol_at_on_empty_source_returns_none() {
        let analyzer = Analyzer::new("");
        let sym = analyzer.symbol_at(0, 0);
        assert!(sym.is_none());
    }

    // ── Resolver equivalence with ModuleGraph ───────────────────────────

    #[test]
    fn workspace_and_module_graph_agree_on_aliased_exports() {
        let mut workspace = Workspace::new();
        let mut graph = animatix_syntax::module::ModuleGraph::new();

        let colors = "/project/colors.amx";
        let theme = "/project/theme.amx";
        let main = "/project/main.amx";

        for (path, source) in [
            (
                colors,
                r#"
pub let primary = (0.38, 0.78, 1.0, 1.0)
let hidden = 1
pub type Swatch = Bool | Str
pub component ColorCard(color: Color) { frame: Rect, color: color }
# SceneFromColors
#0s
box: Rect, size: (10, 10)
"#,
            ),
            (
                theme,
                r#"
import "./colors.amx" as colors
pub let accent = colors.primary
"#,
            ),
            (
                main,
                r#"
import "./theme.amx" as theme
title: Text { content: "Hello" }
"#,
            ),
        ] {
            workspace.add_file(std::path::PathBuf::from(path), source);
            graph.add_source(std::path::PathBuf::from(path), source.to_string());
        }

        let resolved = workspace.resolve_symbols(std::path::Path::new(main));
        let program = graph.load_program(std::path::Path::new(main)).expect("program loads");

        let ws_ns = resolved.namespaces.get("theme").expect("workspace theme namespace");
        let graph_ns = program.namespaces.get("theme").expect("graph theme namespace");

        let mut ws_values: Vec<&str> = ws_ns
            .labels
            .iter()
            .filter(|(_, info)| info.is_pub)
            .map(|(name, _)| name.as_str())
            .collect();
        ws_values.sort();
        let mut graph_values: Vec<&str> = graph_ns.exports.keys().map(String::as_str).collect();
        graph_values.sort();
        assert_eq!(ws_values, graph_values, "pub let export sets should agree");

        let mut ws_aliases: Vec<&str> = ws_ns.type_aliases.keys().map(String::as_str).collect();
        ws_aliases.sort();
        let mut graph_aliases: Vec<&str> =
            graph_ns.type_exports.keys().map(String::as_str).collect();
        graph_aliases.sort();
        assert_eq!(ws_aliases, graph_aliases, "pub type export sets should agree");

        let mut ws_components: Vec<&str> = ws_ns.components.keys().map(String::as_str).collect();
        ws_components.sort();
        let mut graph_components: Vec<&str> =
            graph_ns.component_exports.keys().map(String::as_str).collect();
        graph_components.sort();
        assert_eq!(ws_components, graph_components, "pub component export sets should agree");

        let mut ws_scenes: Vec<&str> = ws_ns.scenes.keys().map(String::as_str).collect();
        ws_scenes.sort();
        let mut graph_scenes: Vec<&str> = graph_ns.scenes.keys().map(String::as_str).collect();
        graph_scenes.sort();
        assert_eq!(ws_scenes, graph_scenes, "scene export sets should agree");

        let mut ws_nested: Vec<&str> = ws_ns.namespaces.keys().map(String::as_str).collect();
        ws_nested.sort();
        let mut graph_nested: Vec<&str> = graph_ns.namespaces.keys().map(String::as_str).collect();
        graph_nested.sort();
        assert_eq!(ws_nested, graph_nested, "nested namespace sets should agree");
    }

    #[test]
    fn workspace_and_module_graph_agree_on_direct_import_exports() {
        let mut workspace = Workspace::new();
        let mut graph = animatix_syntax::module::ModuleGraph::new();

        let lib = "/project/lib.amx";
        let main = "/project/main.amx";

        for (path, source) in [
            (
                lib,
                r#"
pub component PublicCard {}
component PrivateCard {}
# SceneInLib
#0s
box: Rect, size: (10, 10)
"#,
            ),
            (
                main,
                r#"
import "./lib.amx"
title: Text { content: "Hello" }
"#,
            ),
        ] {
            workspace.add_file(std::path::PathBuf::from(path), source);
            graph.add_source(std::path::PathBuf::from(path), source.to_string());
        }

        let resolved = workspace.resolve_symbols(std::path::Path::new(main));
        let program = graph.load_program(std::path::Path::new(main)).expect("program loads");

        let mut ws_components: Vec<&str> = resolved.components.keys().map(String::as_str).collect();
        ws_components.sort();
        let mut graph_components: Vec<&str> =
            program.components.keys().map(String::as_str).collect();
        graph_components.sort();
        assert_eq!(
            ws_components, graph_components,
            "direct imports should expose the same component set (pub-only)"
        );

        let mut ws_scenes: Vec<&str> = resolved.scenes.keys().map(String::as_str).collect();
        ws_scenes.sort();
        let mut graph_scenes: Vec<&str> = program
            .namespaces
            .values()
            .flat_map(|ns| ns.scenes.keys().map(String::as_str).collect::<Vec<_>>())
            .collect();
        graph_scenes.sort();
        // The direct import is flattened, so its scene is part of the loaded
        // statements rather than a namespace export. Compare by checking the
        // workspace scene set contains it and ModuleGraph still carries it.
        assert!(
            ws_scenes.contains(&"SceneInLib"),
            "workspace should include the direct-import scene"
        );
        assert_eq!(graph_scenes, Vec::<&str>::new(), "direct scene is not a namespace export");
    }
}
