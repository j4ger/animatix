use std::collections::VecDeque;

use crate::app::commands::{Command, UndoEntry};
use animatix_syntax::diagnostics::Diagnostic;

/// Owns undo/redo history and render diagnostics.
///
/// Separated from `SourceStore` so handlers can declare whether they need
/// history access (for snapshot guards) or only source access.
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

    /// Take a snapshot of the current source text for undo/redo.
    /// Call this BEFORE making a change to the source.
    pub fn snapshot(&mut self, command: Command, source_before: &str) {
        self.undo_stack.push_back(UndoEntry {
            command,
            source_before: source_before.to_string(),
        });
        self.redo_stack.clear();
        // Limit undo history
        if self.undo_stack.len() > self.undo_limit {
            self.undo_stack.pop_front();
        }
    }
}
