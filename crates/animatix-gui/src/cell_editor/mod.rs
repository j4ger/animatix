mod cell;
mod parser;
mod render;

pub use cell::Cell;
pub use parser::{cells_to_source, parse_cells};
pub use render::render_cell_editor;

/// Persistent state for the cell editor (scroll position, focused cell, etc.).
#[derive(Debug, Clone)]
pub struct CellEditorState {
    pub focused_cell: Option<usize>,
    pub scroll_to_cell: Option<usize>,
    pub highlighted_cell: Option<usize>,
    /// Optional action requests that the caller can apply to the backing cell list.
    pub pending_delete_cell: Option<usize>,
    /// Optional duplicate request that the caller can apply to the backing cell list.
    pub pending_duplicate_cell: Option<usize>,
}

impl Default for CellEditorState {
    fn default() -> Self {
        Self {
            focused_cell: None,
            scroll_to_cell: None,
            highlighted_cell: None,
            pending_delete_cell: None,
            pending_duplicate_cell: None,
        }
    }
}
