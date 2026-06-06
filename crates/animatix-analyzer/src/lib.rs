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

mod symbol_table;
mod completer;
mod diagnostics;
mod workspace;
mod types;
mod hover;
mod definition;
mod references;
mod document_symbol;

pub use symbol_table::{
    SymbolTable, ImportInfo, LabelInfo, LabelKind, ComponentInfo, ParamInfo, SceneInfo,
};
pub use completer::{all_snippets, CompletionItem, CompletionKind, completions_at};
pub use diagnostics::{Diagnostic, DiagnosticSeverity, LintConfig, collect_diagnostics, collect_diagnostics_with_config};
pub use workspace::Workspace;
pub use types::{HoverInfo, Location, DocumentSymbol, SymbolKind};

use animatix_syntax::ast::{Span, Stmt};
use animatix_syntax::parser::{parse_source, ParseError};
// chumsky::Parser trait is not needed directly; parser functions are called via module API.
use std::path::{Path, PathBuf};
use tree_sitter::{Parser as TsParser, Tree};

/// The main entry point for language intelligence.
///
/// Holds parsed source, AST, tree-sitter tree, and extracted symbols.
/// Call `update()` when source changes; query methods are cheap.
pub struct Analyzer {
    source: String,
    path: Option<PathBuf>,
    ast: Option<Vec<Stmt>>,
    parse_errors: Vec<ParseError>,
    tree: Option<Tree>,
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
            tree: None,
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

        // Parse with tree-sitter (for position-based queries)
        let mut ts_parser = TsParser::new();
        ts_parser
            .set_language(&tree_sitter_animatix::language())
            .expect("Failed to set tree-sitter language");
        self.tree = ts_parser.parse(source, None);

        self.rebuild_symbols();
    }

    /// Rebuild the symbol table from the current source and tree.
    fn rebuild_symbols(&mut self) {
        let source = &self.source;

        // Parse with chumsky (source of truth for AST)
        let (ast, errors) = parse_source(source);
        self.ast = ast;
        self.parse_errors = errors;

        // Build symbol table from AST
        let table = if let Some(ref stmts) = self.ast {
            let mut table = SymbolTable::build_from_ast(stmts);
            table.collect_references(stmts);
            if let Some(ref tree) = self.tree {
                Self::enrich_positions(tree, source, &mut table);
            }
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
                syntax_diagnostics
                    .into_iter()
                    .map(Self::convert_type_diagnostic)
                    .collect()
            } else {
                Vec::new()
            };

        self.symbols = table;
    }

    /// Build a component registry from AST statements for the type checker.
    fn build_component_registry(
        stmts: &[Stmt],
    ) -> std::collections::HashMap<String, animatix_syntax::module::ComponentEntry> {
        use animatix_syntax::module::ComponentEntry;
        use std::collections::HashMap;

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
            }
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                diagnostics::DiagnosticSeverity::Warning
            }
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

    /// Walk the tree-sitter tree and populate symbol table entries with real line/col positions.
    fn enrich_positions(tree: &Tree, source: &str, table: &mut SymbolTable) {
        let root = tree.root_node();
        let mut cursor = root.walk();
        Self::walk_for_positions(&mut cursor, source, table);
    }

    fn walk_for_positions(cursor: &mut tree_sitter::TreeCursor, source: &str, table: &mut SymbolTable) {
        let node = cursor.node();
        let kind = node.kind();

        // Extract positions for declaration nodes
        match kind {
            "let_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = name_node.start_position();
                    let end = name_node.end_position();
                    if let Some(info) = table.labels.get_mut(&name) {
                        info.line = start.row + 1; // tree-sitter is 0-based
                        info.col = start.column + 1;
                        info.span = Some(Span {
                            start_line: start.row + 1,
                            start_col: start.column + 1,
                            end_line: end.row + 1,
                            end_col: end.column + 1,
                        });
                    }
                }
            }
            "actor_declaration" => {
                if let Some(label_node) = node.child_by_field_name("label") {
                    let name = label_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = label_node.start_position();
                    let end = label_node.end_position();
                    if let Some(info) = table.labels.get_mut(&name) {
                        info.line = start.row + 1;
                        info.col = start.column + 1;
                        info.span = Some(Span {
                            start_line: start.row + 1,
                            start_col: start.column + 1,
                            end_line: end.row + 1,
                            end_col: end.column + 1,
                        });
                    }
                }
            }
            "component_definition" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = name_node.start_position();
                    let end = name_node.end_position();
                    if let Some(info) = table.components.get_mut(&name) {
                        info.line = start.row + 1;
                        info.col = start.column + 1;
                        info.span = Some(Span {
                            start_line: start.row + 1,
                            start_col: start.column + 1,
                            end_line: end.row + 1,
                            end_col: end.column + 1,
                        });
                    }
                }
            }
            "import_statement" => {
                if let Some(path_node) = node.child_by_field_name("path") {
                    let path = path_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = node.start_position();
                    let end = node.end_position();
                    for info in &mut table.imports {
                        if info.path == path {
                            info.span = Some(Span {
                                start_line: start.row + 1,
                                start_col: start.column + 1,
                                end_line: end.row + 1,
                                end_col: end.column + 1,
                            });
                            break;
                        }
                    }
                }
            }
            "for_block" => {
                if let Some(var_node) = node.child_by_field_name("variable") {
                    let name = var_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = var_node.start_position();
                    let end = var_node.end_position();
                    if let Some(info) = table.labels.get_mut(&name) {
                        info.line = start.row + 1;
                        info.col = start.column + 1;
                        info.span = Some(Span {
                            start_line: start.row + 1,
                            start_col: start.column + 1,
                            end_line: end.row + 1,
                            end_col: end.column + 1,
                        });
                    }
                }
            }
            "use_statement" => {
                if let Some(path_node) = node.child_by_field_name("path") {
                    let path = path_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = node.start_position();
                    let end = node.end_position();
                    for info in &mut table.imports {
                        if info.path == path {
                            info.span = Some(Span {
                                start_line: start.row + 1,
                                start_col: start.column + 1,
                                end_line: end.row + 1,
                                end_col: end.column + 1,
                            });
                            break;
                        }
                    }
                }
            }
            "reactive_binding" => {
                if let Some(target_node) = node.child_by_field_name("target") {
                    let name = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = target_node.start_position();
                    let end = target_node.end_position();
                    if let Some(info) = table.labels.get_mut(&name) {
                        info.line = start.row + 1;
                        info.col = start.column + 1;
                        info.span = Some(Span {
                            start_line: start.row + 1,
                            start_col: start.column + 1,
                            end_line: end.row + 1,
                            end_col: end.column + 1,
                        });
                    }
                }
            }
            "scene_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let start = name_node.start_position();
                    if let Some(info) = table.scenes.get_mut(&name) {
                        info.line = start.row + 1;
                        info.col = start.column + 1;
                    }
                }
            }
            _ => {}
        }

        // Recurse into children
        if cursor.goto_first_child() {
            loop {
                Self::walk_for_positions(cursor, source, table);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
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

    /// Get the tree-sitter tree, if parsing succeeded.
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
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
        completer::completions_at(&self.symbols, self.tree.as_ref(), &self.source, line, col)
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
            self.tree.as_ref(),
            config,
        );
        diagnostics.extend_from_slice(&self.type_diagnostics);
        diagnostics
    }

    /// Hover information at cursor position.
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo> {
        hover::hover_at(&self.symbols, self.tree.as_ref(), &self.source, line, col)
    }

    /// Go-to-definition at cursor position.
    ///
    /// Pass `workspace` for cross-file definition lookup.
    pub fn definition_at(&self, workspace: Option<&Workspace>, line: usize, col: usize) -> Option<Location> {
        definition::definition_at(
            &self.symbols,
            self.tree.as_ref(),
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
        references::find_references(self.tree.as_ref(), &self.source, symbol_name)
    }

    /// Document symbols (outline view).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        document_symbol::document_symbols(&self.symbols)
    }

    /// Get the symbol name at a cursor position.
    /// Returns the identifier or type name at the given line/column,
    /// or None if the position is not on a symbol.
    pub fn symbol_at(&self, line: usize, col: usize) -> Option<String> {
        let tree = self.tree.as_ref()?;
        let point = tree_sitter::Point::new(line, col);
        let node = tree.root_node().descendant_for_point_range(point, point)?;

        match node.kind() {
            "identifier" => {
                Some(node.utf8_text(self.source.as_bytes()).unwrap_or("").to_string())
            }
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
        assert!(analyzer.tree().is_some());
        assert!(analyzer.parse_errors().is_empty());
    }

    #[test]
    fn analyzer_handles_parse_errors() {
        let source = "this is not valid @@@ syntax";
        let analyzer = Analyzer::new(source);
        assert!(!analyzer.parse_errors().is_empty());
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
        assert!(lib_symbols.labels.contains_key("shared_color"), "lib should have shared_color " );

        let analyzer = Analyzer::new_with_path(main_source, Some(PathBuf::from("/project/main.amx")));

        // Verify local symbols are available
        let symbols = analyzer.symbols();
        assert!(symbols.labels.contains_key("title"), "main should have title from local source");

        // Verify workspace resolves cross-file symbols
        let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
        assert!(resolved.labels.contains_key("shared_color"), "workspace should resolve shared_color from import");
    }

    #[test]
    fn workspace_resolves_aliased_imports_to_namespaces() {
        let mut workspace = Workspace::new();

        let lib_source = r#"
let accent = rgb(255, 0, 0)
btn: Button {
    text: "Click"
}
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
        assert!(!resolved.labels.contains_key("btn"));

        // Imported symbols should be in the "lib" namespace
        assert!(resolved.namespaces.contains_key("lib"));
        let lib_ns = &resolved.namespaces["lib"];
        assert!(lib_ns.labels.contains_key("accent"));
        assert!(lib_ns.labels.contains_key("btn"));

        // Namespace-qualified lookup should work
        assert!(resolved.resolve_namespaced_label("lib.accent").is_some());
        assert!(resolved.resolve_namespaced_label("lib.btn").is_some());
        assert!(resolved.resolve_namespaced_label("lib.unknown").is_none());
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
        assert!(loc.file.is_none(), "definition in same file " );
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
                symbols[i - 1].name, symbols[i - 1].line,
                symbols[i].name, symbols[i].line,
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
}
