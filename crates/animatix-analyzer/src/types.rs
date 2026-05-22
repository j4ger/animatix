//! Public types for language intelligence queries.

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
