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

pub use symbol_table::{
    SymbolTable, ImportInfo, LabelInfo, LabelKind, ComponentInfo, ParamInfo, SceneInfo,
};
pub use completer::{CompletionItem, CompletionKind, completions_at};
pub use diagnostics::{Diagnostic, DiagnosticSeverity, collect_diagnostics};

use animatix_syntax::ast::{Span, Stmt};
use animatix_syntax::parser::parser;
use chumsky::Parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Parser as TsParser, Tree};

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
    #[allow(dead_code)]
    source: String,
    #[allow(dead_code)]
    ast: Option<Vec<Stmt>>,
    symbols: SymbolTable,
}

impl Workspace {
    /// Create an empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a file in the workspace.
    pub fn add_file(&mut self, path: PathBuf, source: &str) {
        let (ast, _) = parser().parse(source).into_output_errors();
        let symbols = ast
            .as_ref()
            .map(|stmts| SymbolTable::build_from_ast(stmts))
            .unwrap_or_default();
        self.files.insert(
            path,
            FileEntry {
                source: source.to_string(),
                ast,
                symbols,
            },
        );
    }

    /// Remove a file from the workspace.
    pub fn remove_file(&mut self, path: &Path) {
        self.files.remove(path);
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

    /// Check if a file exists in the workspace.
    pub fn has_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    /// Get the symbol table for a specific file.
    pub fn file_symbols(&self, path: &Path) -> Option<SymbolTable> {
        self.files.get(path).map(|e| e.symbols.clone())
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
        self.force_rebuild_symbols();
    }

    /// Force re-parsing and symbol resolution without source change.
    fn force_rebuild_symbols(&mut self) {
        // Parse with chumsky (source of truth for AST)
        let (ast, errors): (Option<Vec<Stmt>>, _) = parser().parse(&self.source).into_output_errors();
        self.ast = ast;
        self.parse_errors = errors.iter().map(|e| format!("{:?}", e)).collect();

        // Build symbol table from AST
        let mut table = if let Some(ref stmts) = self.ast {
            let mut table = SymbolTable::build_from_ast(stmts);
            // Enrich with real positions from tree-sitter
            if let Some(ref tree) = self.tree {
                Self::enrich_positions(tree, &self.source, &mut table);
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

    /// Get the file path, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Update the source text. Re-parses if changed.
    pub fn update(&mut self, source: &str) {
        if self.source == source {
            return;
        }

        self.source = source.to_string();

        // Parse with chumsky (source of truth for AST)
        let (ast, errors): (Option<Vec<Stmt>>, _) = parser().parse(source).into_output_errors();
        self.ast = ast;
        self.parse_errors = errors.iter().map(|e| format!("{:?}", e)).collect();

        // Parse with tree-sitter (for position-based queries)
        let mut ts_parser = TsParser::new();
        ts_parser
            .set_language(&tree_sitter_animatix::language())
            .expect("Failed to set tree-sitter language");
        self.tree = ts_parser.parse(source, None);

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
                // Merge imported symbols into local table
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
        let tree = self.tree.as_ref()?;
        let point = tree_sitter::Point::new(line, col);
        let node = tree.root_node().descendant_for_point_range(point, point)?;

        let text = &self.source[node.byte_range()];

        // Check what kind of node we're hovering over
        match node.kind() {
            "identifier" => {
                // Check if it's a label
                if let Some(info) = self.symbols.labels.get(text) {
                    let kind = match info.kind {
                        LabelKind::Actor => "Actor",
                        LabelKind::Let => "Variable",
                        LabelKind::For => "Loop variable",
                        LabelKind::Always => "Always block",
                        LabelKind::Component => "Component",
                    };
                    let ty = info.ty.as_deref().unwrap_or("unknown");
                    Some(HoverInfo {
                        contents: format!("**{}** `{}`\n\nType: {}", kind, text, ty),
                        range: Some((node.start_position().row, node.start_position().column,
                                     node.end_position().row, node.end_position().column)),
                    })
                }
                // Check if it's a type
                else if self.symbols.types.contains(text) {
                    let doc = type_documentation(text);
                    Some(HoverInfo {
                        contents: format!("**Type** `{}`\n\n{}", text, doc),
                        range: Some((node.start_position().row, node.start_position().column,
                                     node.end_position().row, node.end_position().column)),
                    })
                }
                // Check if it's an action
                else if self.symbols.actions.contains(text) {
                    let doc = action_documentation(text);
                    Some(HoverInfo {
                        contents: format!("**Action** `{}`\n\n{}", text, doc),
                        range: Some((node.start_position().row, node.start_position().column,
                                     node.end_position().row, node.end_position().column)),
                    })
                }
                // Check if it's a keyword
                else if self.symbols.keywords.contains(text) {
                    let doc = keyword_documentation(text);
                    Some(HoverInfo {
                        contents: format!("**Keyword** `{}`\n\n{}", text, doc),
                        range: Some((node.start_position().row, node.start_position().column,
                                     node.end_position().row, node.end_position().column)),
                    })
                }
                else {
                    None
                }
            }
            "type_identifier" => {
                let doc = type_documentation(text);
                Some(HoverInfo {
                    contents: format!("**Type** `{}`\n\n{}", text, doc),
                    range: Some((node.start_position().row, node.start_position().column,
                                 node.end_position().row, node.end_position().column)),
                })
            }
            "string" => {
                Some(HoverInfo {
                    contents: format!("**String** `{}`", text),
                    range: Some((node.start_position().row, node.start_position().column,
                                 node.end_position().row, node.end_position().column)),
                })
            }
            "number" | "duration_literal" | "percentage" => {
                Some(HoverInfo {
                    contents: format!("**Number** `{}`", text),
                    range: Some((node.start_position().row, node.start_position().column,
                                 node.end_position().row, node.end_position().column)),
                })
            }
            "comment" => {
                Some(HoverInfo {
                    contents: format!("*Comment*\n\n{}", text),
                    range: Some((node.start_position().row, node.start_position().column,
                                 node.end_position().row, node.end_position().column)),
                })
            }
            _ => None,
        }
    }

    /// Go-to-definition at cursor position.
    pub fn definition_at(&self, line: usize, col: usize) -> Option<Location> {
        let tree = self.tree.as_ref()?;
        let point = tree_sitter::Point::new(line, col);
        let node = tree.root_node().descendant_for_point_range(point, point)?;

        let text = &self.source[node.byte_range()];

        // Only handle identifiers
        if node.kind() != "identifier" && node.kind() != "type_identifier" {
            return None;
        }

        // Check if it's a label defined in this file
        if let Some(info) = self.symbols.labels.get(text) {
            return Some(Location {
                file: None, // Same file
                line: info.line,
                col: info.col,
            });
        }

        // Check if it's a component defined in this file
        if let Some(info) = self.symbols.components.get(text) {
            return Some(Location {
                file: None,
                line: info.line,
                col: info.col,
            });
        }

        // Check imported files for cross-file definitions
        if let Some(ref workspace) = self.workspace {
            if let Some(ref path) = self.path {
                for import in &self.symbols.imports {
                    let import_path = Workspace::resolve_import_path(path, &import.path);
                    if let Some(symbols) = (**workspace).file_symbols(&import_path) {
                        // Check labels in imported file
                        if let Some(info) = symbols.labels.get(text) {
                            return Some(Location {
                                file: Some(import_path.display().to_string()),
                                line: info.line,
                                col: info.col,
                            });
                        }
                        // Check components in imported file
                        if let Some(info) = symbols.components.get(text) {
                            return Some(Location {
                                file: Some(import_path.display().to_string()),
                                line: info.line,
                                col: info.col,
                            });
                        }
                    }
                }
            }
        }

        None
    }

    /// Find all references to a symbol name in this file.
    /// Returns a list of (start_line, start_col, end_line, end_col) ranges.
    pub fn find_references(&self, symbol_name: &str) -> Vec<(usize, usize, usize, usize)> {
        let mut refs = Vec::new();

        if let Some(tree) = self.tree.as_ref() {
            let mut cursor = tree.walk();
            Self::collect_references(&mut cursor, &self.source, symbol_name, &mut refs);
        }

        refs
    }

    fn collect_references(
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        symbol_name: &str,
        refs: &mut Vec<(usize, usize, usize, usize)>,
    ) {
        let node = cursor.node();

        if (node.kind() == "identifier" || node.kind() == "type_identifier")
            && node.utf8_text(source.as_bytes()).unwrap_or("") == symbol_name
        {
            refs.push((
                node.start_position().row,
                node.start_position().column,
                node.end_position().row,
                node.end_position().column,
            ));
        }

        if cursor.goto_first_child() {
            loop {
                Self::collect_references(cursor, source, symbol_name, refs);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    /// Document symbols (outline view).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();

        for (name, info) in &self.symbols.labels {
            let kind = match info.kind {
                LabelKind::Actor => SymbolKind::Actor,
                LabelKind::Let => SymbolKind::Variable,
                LabelKind::For => SymbolKind::Variable,
                LabelKind::Always => SymbolKind::Block,
                LabelKind::Component => SymbolKind::Component,
            };
            symbols.push(DocumentSymbol {
                name: name.clone(),
                kind,
                line: info.line,
                col: info.col,
                detail: info.ty.clone(),
            });
        }

        for (name, info) in &self.symbols.components {
            symbols.push(DocumentSymbol {
                name: name.clone(),
                kind: SymbolKind::Component,
                line: info.line,
                col: info.col,
                detail: Some(format!("({} params)", info.params.len())),
            });
        }

        symbols.sort_by(|a, b| a.line.cmp(&b.line));
        symbols
    }
}

/// Hover information.
#[derive(Debug, Clone)]
pub struct HoverInfo {
    /// Markdown content to display.
    pub contents: String,
    /// Range of the hovered element (start_line, start_col, end_line, end_col).
    pub range: Option<(usize, usize, usize, usize)>,
}

/// A location in a file.
#[derive(Debug, Clone)]
pub struct Location {
    /// File path (None = same file).
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

/// A document symbol for outline view.
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub col: usize,
    pub detail: Option<String>,
}

/// The kind of document symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Actor,
    Variable,
    Component,
    Block,
}

/// Documentation for a type.
fn type_documentation(name: &str) -> &str {
    match name {
        "Text" => "Text element with content and styling properties.",
        "Math" => "Mathematical expression renderer.",
        "Code" => "Code block with syntax highlighting.",
        "Svg" => "SVG image element.",
        "Image" => "Raster image element.",
        "Rect" => "Rectangle shape with fill and stroke.",
        "Ellipse" => "Ellipse, circle, arc, or dot shape.",
        "Line" => "Line segment or arrow with optional head.",
        "Polygon" => "Polygon or regular polygon shape.",
        "Path" => "SVG path element.",
        "Graph" => "Function graph.",
        "PlotCurve" => "Plot curve with configurable sampling kind.",
        "Button" => "Interactive button element.",
        _ => "Unknown type.",
    }
}

/// Documentation for an action.
fn action_documentation(name: &str) -> &str {
    match name {
        "fade-in" => "Fade in from transparent.",
        "draw-in" => "Draw in (like handwriting).",
        "wipe-in" => "Wipe in from edge.",
        "fade-out" => "Fade out to transparent.",
        "wipe-out" => "Wipe out to edge.",
        "reveal-out" => "Reveal out (reverse draw).",
        "draw-out" => "Draw out (reverse handwriting).",
        "move" => "Move to position: `move target to (x, y)`",
        "shift" => "Shift by offset: `shift target by (dx, dy)`",
        "rotate" => "Rotate: `rotate target by 90`",
        "scale" => "Scale: `scale target to 2`",
        _ => "Unknown action.",
    }
}

/// Documentation for a keyword.
fn keyword_documentation(name: &str) -> &str {
    match name {
        "let" => "Declare a variable: `let name = value`",
        "import" => "Import another file: `import \"path\"`",
        "always" => "Reactive block that runs continuously.",
        "if" => "Conditional: `if condition { ... }`",
        "else" => "Else branch: `if ... { } else { }`",
        "for" => "Loop: `for item in collection { ... }`",
        "in" => "Used in for loops.",
        "pub" => "Make visible to other files.",
        "component" => "Define a reusable component.",
        "sequence" => "Run actions in sequence.",
        "stagger" => "Stagger actions with delay.",
        _ => "Keyword.",
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
