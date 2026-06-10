use std::collections::VecDeque;

use crate::app::commands::{Command, UndoEntry};
use crate::app::document::history::UiSnapshot;
use animatix_syntax::diagnostics::Diagnostic;

/// Owns undo/redo history with UI state snapshots.
pub struct HistoryStore {
    pub undo_stack: VecDeque<UndoEntry>,
    pub redo_stack: VecDeque<UndoEntry>,
    pub undo_limit: usize,
    pub render_diagnostics: Vec<Diagnostic>,
    pub runtime_diagnostics: Vec<Diagnostic>,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            undo_limit: 100,
            render_diagnostics: Vec::new(),
            runtime_diagnostics: Vec::new(),
        }
    }

    /// Take a snapshot for undo/redo. Call BEFORE making a change.
    /// Captures source text and UI state.
    pub fn snapshot(
        &mut self,
        command: Command,
        source_before: &str,
        source_after: &str,
        ui_before: UiSnapshot,
        ui_after: UiSnapshot,
    ) {
        self.undo_stack.push_back(UndoEntry {
            command,
            source_before: source_before.to_string(),
            source_after: source_after.to_string(),
            ui_before,
            ui_after,
        });
        self.redo_stack.clear();
        if self.undo_stack.len() > self.undo_limit {
            self.undo_stack.pop_front();
        }
    }

    /// Undo the last operation. Returns the undo entry if available.
    pub fn undo(&mut self) -> Option<UndoEntry> {
        self.undo_stack.pop_back()
    }

    /// Redo the last undone operation. Returns the redo entry if available.
    pub fn redo(&mut self) -> Option<UndoEntry> {
        self.redo_stack.pop_back()
    }
}
