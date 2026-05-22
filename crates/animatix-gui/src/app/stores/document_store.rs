use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use crate::app::commands::UndoEntry;

/// Owns the canonical document text (via EditorBuffer) and the compiled
/// timeline (via DocumentSession).  This is the single source of truth for
/// everything that can be saved to disk.
pub struct DocumentStore {
    pub document: DocumentSession,
    pub editor: EditorBuffer,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub undo_limit: usize,
    pub drag_snapshot_taken: bool,
}
