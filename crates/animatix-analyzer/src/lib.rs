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
pub use completer::{CompletionItem, CompletionKind, completions_at};
pub use diagnostics::{Diagnostic, DiagnosticSeverity, collect_diagnostics};
pub use workspace::Workspace;
pub use types::{HoverInfo, Location, DocumentSymbol, SymbolKind};

use animatix_syntax::ast::{Span, Stmt};
use animatix_syntax::parser::parser;
use chumsky::Parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Parser as TsParser, Tree};

/// The main entry point for language intelligence.
///
/// Holds parsed source, AST, tree-sitter tree, and extracted symbols.
/// Call `update()` when source changes; query methods are cheap.
#[derive(Clone)]
pub struct Analyzer {
    source: String,
    path: Option<PathBuf>,
    ast: Option<Vec<Stmt>>,
    parse_errors: Vec<String>,
    tree: Option<Tree>,
    symbols: SymbolTable,
    workspace: Option<std::sync::Arc<Workspace>>,
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
            workspace: None,
        };
        analyzer.update(source);
        analyzer
    }

    /// Attach a workspace for cross-file analysis.
    /// Triggers re-resolution of cross-file symbols.
    pub fn set_workspace(&mut self, workspace: std::sync::Arc<Workspace>) {
        self.workspace = Some(workspace);
        // Force re-build of symbols with workspace context
        self.rebuild_symbols();
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
    /// Shared logic between `update()` and `set_workspace()`.
    fn rebuild_symbols(&mut self) {
        let source = &self.source;

        // Parse with chumsky (source of truth for AST)
        let (ast, errors): (Option<Vec<Stmt>>, _) = parser().parse(source).into_output_errors();
        self.ast = ast;
        self.parse_errors = errors.iter().map(|e| format!("{:?}", e)).collect();

        // Build symbol table from AST
        let mut table = if let Some(ref stmts) = self.ast {
            let mut table = SymbolTable::build_from_ast(stmts);
            // Enrich with real positions from tree-sitter
            if let Some(ref tree) = self.tree {
                Self::enrich_positions(tree, source, &mut table);
            }
            table
        } else {
            SymbolTable::default()
        };

        // Resolve cross-file symbols if workspace is attached
        if let Some(ref workspace) = self.workspace {
            if let Some(ref path) = self.path {
                let resolved = workspace.resolve_symbols(path);
                table.merge(&resolved);
            }
        }

        self.symbols = table;
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
            "actor_declaration" | "text_shorthand" => {
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

    /// Get parse errors.
    pub fn parse_errors(&self) -> &[String] {
        &self.parse_errors
    }

    /// Completions at cursor position.
    pub fn completions_at(&self, line: usize, col: usize) -> Vec<CompletionItem> {
        completer::completions_at(&self.symbols, self.tree.as_ref(), &self.source, line, col)
    }

    /// All diagnostics (parse errors + semantic checks).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        diagnostics::collect_diagnostics(&self.source, &self.parse_errors, self.tree.as_ref(), &self.symbols, self.ast.as_deref())
    }

    /// Hover information at cursor position.
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo> {
        hover::hover_at(&self.symbols, self.tree.as_ref(), &self.source, line, col)
    }

    /// Go-to-definition at cursor position.
    pub fn definition_at(&self, line: usize, col: usize) -> Option<Location> {
        definition::definition_at(
            &self.symbols,
            self.tree.as_ref(),
            &self.source,
            self.workspace.as_deref(),
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
        // Should not panic, may have errors
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

        // Should include symbols from main file
        assert!(resolved.labels.contains_key("title"));
        // Should include symbols from imported lib file
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

        // Verify workspace has the file
        assert!(workspace.has_file(Path::new("/project/lib.amx")));
        
        // Verify import resolution
        let import_path = Workspace::resolve_import_path(Path::new("/project/main.amx"), "lib.amx");
        assert_eq!(import_path, PathBuf::from("/project/lib.amx"));
        
        // Verify lib symbols exist
        let lib_symbols = workspace.file_symbols(Path::new("/project/lib.amx")).unwrap();
        assert!(lib_symbols.labels.contains_key("shared_color"), "lib should have shared_color");

        let mut analyzer = Analyzer::new_with_path(main_source, Some(PathBuf::from("/project/main.amx")));
        analyzer.set_workspace(std::sync::Arc::new(workspace));

        let symbols = analyzer.symbols();

        // Should include symbols from imported file
        assert!(symbols.labels.contains_key("shared_color"), "main should have shared_color from import");
        assert!(symbols.labels.contains_key("title"));
    }

    #[test]
    fn workspace_import_path_resolution() {
        let base = Path::new("/project/src/main.amx");
        let resolved = Workspace::resolve_import_path(base, "../lib.amx");
        // Path may not be normalized (../ not resolved), just check components
        assert!(resolved.to_string_lossy().contains("lib.amx"));
        assert_eq!(
            Workspace::resolve_import_path(base, "utils.amx"),
            PathBuf::from("/project/src/utils.amx")
        );
    }
}
