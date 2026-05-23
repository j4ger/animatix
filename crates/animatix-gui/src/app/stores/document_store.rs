use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use crate::app::commands::{Command, UndoEntry};
use animatix::diagnostics::Diagnostic;
use animatix::diagnostics::diagnostics_phase_summary;

/// Owns the canonical document text (via EditorBuffer) and the compiled
/// timeline (via DocumentSession).  This is the single source of truth for
/// everything that can be saved to disk.
pub struct DocumentStore {
    pub document: DocumentSession,
    pub editor: EditorBuffer,
    pub render_diagnostics: Vec<Diagnostic>,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub undo_limit: usize,
}

impl DocumentStore {
    pub fn new(
        document: DocumentSession,
        editor: EditorBuffer,
    ) -> Self {
        Self {
            document,
            editor,
            render_diagnostics: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_limit: 100,
        }
    }

    pub fn combined_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.document.diagnostics.clone();
        diagnostics.extend(self.render_diagnostics.iter().cloned());
        diagnostics
    }

    /// Take a snapshot of the current source text for undo/redo.
    /// Call this BEFORE making a change to the source.
    pub fn snapshot(&mut self, command: Command) {
        self.undo_stack.push(UndoEntry {
            command,
            source_before: self.document.source_text.clone(),
        });
        self.redo_stack.clear();
        // Limit undo history
        if self.undo_stack.len() > self.undo_limit {
            self.undo_stack.remove(0);
        }
    }

    pub fn document_status(&self, base_status: String) -> String {
        if self.document.diagnostics.is_empty() {
            base_status
        } else {
            format!(
                "{base_status} • {}",
                diagnostics_phase_summary(&self.document.diagnostics)
            )
        }
    }
}