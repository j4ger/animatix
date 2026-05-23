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
    /// The 1-based line number.
    pub line: usize,
    /// The 1-based column number.
    pub col: usize,
}

/// A document symbol for outline view.
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    /// The display name of the symbol.
    pub name: String,
    /// The kind of symbol (actor, variable, etc.).
    pub kind: SymbolKind,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Optional detail text (e.g., type name or parameter list).
    pub detail: Option<String>,
}

/// The kind of document symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    /// An actor declaration (label with a type).
    Actor,
    /// A variable or let binding.
    Variable,
    /// A reusable component definition.
    Component,
    /// A code block (always, sequence, stagger, etc.).
    Block,
}
