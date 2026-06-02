use crate::app::commands::Command;
use crate::app::stores::{HistoryStore, SourceStore};
use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::diagnostics::diagnostics_phase_summary;

/// Facade that combines `SourceStore` (document + editor + caches) and
/// `HistoryStore` (undo/redo + render diagnostics).
///
/// All source fields (document, editor, caches) are accessed via `document_store.source.*`.
/// History fields are accessed via `document_store.history.*`.
pub struct DocumentStore {
    pub source: SourceStore,
    pub history: HistoryStore,
}

impl DocumentStore {
    pub fn new(document: DocumentSession, editor: EditorBuffer) -> Self {
        Self {
            source: SourceStore::new(document, editor),
            history: HistoryStore::new(),
        }
    }

    /// Combined document + render diagnostics for the diagnostics panel.
    pub fn combined_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.source.document.diagnostics.clone();
        diagnostics.extend(self.history.render_diagnostics.iter().cloned());
        diagnostics
    }

    /// Convenience: build a status string that includes diagnostic summary.
    pub fn document_status(&self, base_status: String) -> String {
        if self.source.document.diagnostics.is_empty() {
            base_status
        } else {
            format!(
                "{base_status} • {}",
                diagnostics_phase_summary(&self.source.document.diagnostics)
            )
        }
    }

    /// Convenience: snapshot current source text for undo/redo.
    pub fn snapshot(&mut self, command: Command) {
        let source_before = self.source.document.source_text.clone();
        self.history.snapshot(command, &source_before);
    }
}

// Re-export rebuild_cache so callers don't need to change.
pub use crate::app::stores::source_store::rebuild_cache;
