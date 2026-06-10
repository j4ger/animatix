use std::sync::Arc;

use crate::app::commands::Command;
use crate::app::document::snapshot::{DocumentSnapshot, SnapshotStatus};
use crate::app::document::version::DocumentGeneration;
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
///
/// Snapshots are immutable derived state produced by rebuilds.
pub struct DocumentStore {
    pub source: SourceStore,
    pub history: HistoryStore,

    // ── Immutable snapshot management ──
    /// Current generation counter, incremented on each accepted rebuild.
    pub(crate) generation: DocumentGeneration,
    /// The latest completed snapshot (may be failed/stale).
    pub(crate) current: Option<Arc<DocumentSnapshot>>,
    /// The latest snapshot with a renderable target (for preview fallback).
    pub(crate) last_good: Option<Arc<DocumentSnapshot>>,
}

impl DocumentStore {
    pub fn new(document: DocumentSession, editor: EditorBuffer) -> Self {
        Self {
            source: SourceStore::new(document, editor),
            history: HistoryStore::new(),
            generation: DocumentGeneration::initial(),
            current: None,
            last_good: None,
        }
    }

    /// Combined document + render + runtime diagnostics for the diagnostics panel.
    pub fn combined_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.source.document.diagnostics.clone();
        diagnostics.extend(self.history.render_diagnostics.iter().cloned());
        diagnostics.extend(self.history.runtime_diagnostics.iter().cloned());
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
        let source_after = self.source.document.source_text.clone();
        let default_ui = crate::app::document::history::UiSnapshot::default_with_tool(
            crate::app::preview::ToolMode::Move
        );
        self.history.snapshot(
            command,
            &source_before,
            &source_after,
            default_ui.clone(),
            default_ui,
        );
    }

    /// Snapshot with UI state capture for richer undo/redo.
    pub fn snapshot_with_ui(
        &mut self,
        command: Command,
        ui_before: crate::app::document::history::UiSnapshot,
        ui_after: crate::app::document::history::UiSnapshot,
    ) {
        let source_before = self.source.document.source_text.clone();
        let source_after = self.source.document.source_text.clone();
        self.history.snapshot(command, &source_before, &source_after, ui_before, ui_after);
    }

    // ── Snapshot API ──

    /// The latest completed snapshot (may be failed or stale).
    pub fn current_snapshot(&self) -> Option<Arc<DocumentSnapshot>> {
        self.current.clone()
    }

    /// The latest snapshot with a renderable target (for preview fallback).
    pub fn last_good_snapshot(&self) -> Option<Arc<DocumentSnapshot>> {
        self.last_good.clone()
    }

    /// The current document generation.
    pub fn document_generation(&self) -> DocumentGeneration {
        self.generation
    }

    /// Publish a new snapshot, incrementing the generation.
    pub fn publish_snapshot(&mut self, mut snapshot: DocumentSnapshot) {
        self.generation = self.generation.next();
        snapshot.generation = self.generation;
        snapshot.source_epoch = self.source.epoch();

        let has_renderable = snapshot.has_renderable_target();
        let is_current = !matches!(snapshot.status, SnapshotStatus::Stale { .. });

        let snapshot = Arc::new(snapshot);

        if is_current {
            self.current = Some(snapshot.clone());
        }
        if has_renderable {
            self.last_good = Some(snapshot);
        }
    }

    /// Mark the current snapshot as stale due to a source change.
    pub fn mark_source_stale(&mut self, epoch: crate::app::document::version::SourceEpoch) {
        if let Some(ref current) = self.current {
            // Don't replace a failed snapshot's status, just add stale note
            if matches!(current.status, SnapshotStatus::Clean) {
                let mut updated = current.as_ref().clone();
                updated.status = SnapshotStatus::Stale {
                    current_source_epoch: epoch,
                };
                self.current = Some(Arc::new(updated));
            }
        }
    }

    /// Try to rebuild the current document and produce a snapshot.
    /// Returns true if a snapshot was published.
    pub fn try_rebuild_snapshot(&mut self) -> bool {
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            self.source.text().hash(&mut hasher);
            hasher.finish()
        };

        let result = self.source.document.rebuild();
        match result {
            Ok(()) => {
                let snapshot = snapshot_from_session(&self.source.document, hash);
                self.publish_snapshot(snapshot);
                true
            },
            Err(_) => {
                // Create snapshot with diagnostics from the failed state
                let snapshot = DocumentSnapshot {
                    generation: self.generation.next(),
                    source_epoch: self.source.epoch(),
                    source_hash: crate::app::document::version::SourceHash(hash),
                    status: SnapshotStatus::Failed {
                        error: "rebuild failed",
                    },
                    raw_statements: None,
                    expanded_statements: None,
                    namespaces: Default::default(),
                    components: Default::default(),
                    module_actions: Default::default(),
                    source_index: None,
                    target: crate::app::document::snapshot::BuildTargetSnapshot::Empty,
                    timeline_index: Default::default(),
                    keyframe_lines: Vec::new(),
                    diagnostics: Arc::new(self.source.document.diagnostics.clone()),
                    duration_s: self.source.document.duration_s,
                    scene_dimensions: self.source.document.scene_dimensions,
                };
                self.publish_snapshot(snapshot);
                false
            },
        }
    }
}

/// Build a DocumentSnapshot from the current DocumentSession state.
fn snapshot_from_session(doc: &DocumentSession, source_hash: u64) -> DocumentSnapshot {
    let target = if let Some(timeline) = doc.timeline.as_ref() {
        crate::app::document::snapshot::BuildTargetSnapshot::Timeline(std::sync::Arc::new(
            timeline.clone(),
        ))
    } else if let Some(composition) = doc.composition.as_ref() {
        crate::app::document::snapshot::BuildTargetSnapshot::Composition(std::sync::Arc::new(
            composition.clone(),
        ))
    } else {
        crate::app::document::snapshot::BuildTargetSnapshot::Empty
    };

    DocumentSnapshot {
        generation: DocumentGeneration::initial(), // will be set by publish_snapshot
        source_epoch: crate::app::document::version::SourceEpoch(0), // will be set
        source_hash: crate::app::document::version::SourceHash(source_hash),
        status: SnapshotStatus::Clean,
        raw_statements: doc.raw_statements.as_ref().map(|v| std::sync::Arc::new(v.clone())),
        expanded_statements: doc
            .expanded_statements
            .as_ref()
            .map(|v| std::sync::Arc::new(v.clone())),
        namespaces: std::sync::Arc::new(doc.namespaces.clone()),
        components: std::sync::Arc::new(doc.components.clone()),
        module_actions: std::sync::Arc::new(doc.module_actions.clone()),
        source_index: doc.source_index.clone(),
        target,
        timeline_index: doc.timeline_index.clone(),
        keyframe_lines: doc.keyframe_lines.clone(),
        diagnostics: std::sync::Arc::new(doc.diagnostics.clone()),
        duration_s: doc.duration_s,
        scene_dimensions: doc.scene_dimensions,
    }
}

// Re-export rebuild_cache so callers don't need to change.
pub use crate::app::stores::source_store::rebuild_cache;
