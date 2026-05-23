use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use crate::app::commands::{Command, CommandQueue, UndoEntry};
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

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn combined_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.document.diagnostics.clone();
        diagnostics.extend(self.render_diagnostics.iter().cloned());
        diagnostics
    }

    pub fn apply_source_edit(&mut self, new_source: String) {
        let old_source = self.document.source_text.clone();
        let edits = crate::text_diff::diff_text(&old_source, &new_source);
        self.document.source_text = new_source;
        self.editor.apply_edits(&edits);
        self.document.is_dirty = true;
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