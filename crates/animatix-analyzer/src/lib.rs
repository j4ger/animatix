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

pub use symbol_table::*;
pub use completer::*;
pub use diagnostics::*;

use animatix::ast::Stmt;
use animatix::parser::parser;
use chumsky::Parser;
use tree_sitter::{Parser as TsParser, Tree};

/// The main entry point for language intelligence.
///
/// Holds parsed source, AST, tree-sitter tree, and extracted symbols.
/// Call `update()` when source changes; query methods are cheap.
#[derive(Clone)]
pub struct Analyzer {
    source: String,
    ast: Option<Vec<Stmt>>,
    parse_errors: Vec<String>,
    tree: Option<Tree>,
    symbols: SymbolTable,
}

impl Analyzer {
    /// Create a new analyzer from source text.
    pub fn new(source: &str) -> Self {
        let mut analyzer = Self {
            source: String::new(),
            ast: None,
            parse_errors: Vec::new(),
            tree: None,
            symbols: SymbolTable::default(),
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
        self.symbols = if let Some(ref stmts) = self.ast {
            SymbolTable::build_from_ast(stmts)
        } else {
            SymbolTable::default()
        };
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

        None
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
        "Circle" => "Circle shape with fill and stroke.",
        "Dot" => "Small dot marker.",
        "Rect" => "Rectangle shape.",
        "Square" => "Square shape.",
        "Line" => "Line segment between two points.",
        "Arrow" => "Arrow with head.",
        "Ellipse" => "Ellipse shape.",
        "Arc" => "Arc segment.",
        "Polygon" => "Polygon shape.",
        "RegularPolygon" => "Regular polygon (triangle, pentagon, etc.).",
        "Path" => "SVG path element.",
        "Graph" => "Function graph.",
        "CartesianPlot" => "Cartesian coordinate plot.",
        "PolarPlot" => "Polar coordinate plot.",
        "ParametricPlot" => "Parametric curve plot.",
        "ImplicitPlot" => "Implicit equation plot.",
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
}
