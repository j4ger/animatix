use std::sync::Arc;

use animatix_syntax::diagnostics::{Diagnostic, diagnostics_phase_summary};

use crate::app::commands::Command;
use crate::app::document::DocumentSession;
use crate::app::document::history::UiSnapshot;
use crate::app::document::snapshot::{DocumentSnapshot, SnapshotStatus};
use crate::app::document::version::DocumentGeneration;
use crate::app::stores::{HistoryStore, SourceStore};
use crate::editor::EditorBuffer;

/// Pending undo snapshot state, captured before a mutation.
struct PendingSnapshot {
    command: Command,
    source_before: String,
    ui_before: UiSnapshot,
}

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

    /// Pending undo snapshot (captured before mutation, finalized after).
    pending_snapshot: Option<PendingSnapshot>,
}

// Snapshot types contain Rc-based interior mutability and are not Send+Sync;
// Arc is used here intentionally for cheap cloning within the single-threaded GUI.
#[allow(clippy::arc_with_non_send_sync)]
impl DocumentStore {
    pub fn new(document: DocumentSession, editor: EditorBuffer) -> Self {
        Self {
            source: SourceStore::new(document, editor),
            history: HistoryStore::new(),
            generation: DocumentGeneration::initial(),
            current: None,
            last_good: None,
            pending_snapshot: None,
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

    /// Begin an undo snapshot — captures source text before mutation.
    /// The snapshot is finalized when `commit_source()` or `replace_text()` is called.
    pub fn snapshot(&mut self, command: Command, ui_before: UiSnapshot) {
        self.pending_snapshot = Some(PendingSnapshot {
            command,
            source_before: self.source.text().to_string(),
            ui_before,
        });
    }

    /// Finalize a pending snapshot by capturing source-after and committing to history.
    fn finalize_snapshot(&mut self, ui_after: UiSnapshot) {
        if let Some(pending) = self.pending_snapshot.take() {
            let source_after = self.source.text().to_string();
            self.history.snapshot(
                pending.command,
                &pending.source_before,
                &source_after,
                pending.ui_before,
                ui_after,
            );
        }
    }

    /// Drop a pending undo snapshot after a mutation attempt did not change source.
    pub fn abort_snapshot(&mut self) {
        self.pending_snapshot = None;
    }

    /// Run a source mutation with a scoped undo snapshot.
    ///
    /// The snapshot is finalized when the mutation commits source text and
    /// drops the pending snapshot, or when the source text changed without an
    /// explicit commit. It is aborted when the mutation returns an error or
    /// leaves the source unchanged. This removes the need for every handler to
    /// remember to call `abort_snapshot` on each failure branch.
    pub fn with_mutation<T, E>(
        &mut self,
        command: Command,
        ui_before: UiSnapshot,
        ui_after: UiSnapshot,
        mutate: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        self.snapshot(command, ui_before);
        let result = mutate(self);
        match result {
            Ok(value) => {
                let changed = self
                    .pending_snapshot
                    .as_ref()
                    .is_some_and(|pending| pending.source_before != self.source.text());
                if changed {
                    self.finalize_snapshot(ui_after);
                } else {
                    self.abort_snapshot();
                }
                Ok(value)
            },
            Err(err) => {
                self.abort_snapshot();
                Err(err)
            },
        }
    }

    /// Returns true when a mutation snapshot is waiting to be finalized.
    #[cfg(test)]
    pub fn pending_snapshot_is_none(&self) -> bool {
        self.pending_snapshot.is_none()
    }

    // ── Snapshot API ──

    /// The latest snapshot with a renderable target (for preview fallback).
    pub fn last_good_snapshot(&self) -> Option<Arc<DocumentSnapshot>> {
        self.last_good.clone()
    }

    /// Publish a new snapshot, incrementing the generation.
    pub fn publish_snapshot(&mut self, mut snapshot: DocumentSnapshot) {
        self.generation = self.generation.next();
        snapshot.generation = self.generation;
        snapshot.source_epoch = self.source.epoch();

        let has_renderable = snapshot.has_renderable_target();
        let is_current = !matches!(snapshot.status, SnapshotStatus::Stale { .. });

        let snapshot = Arc::new(snapshot); // Arc used for cheap cloning across snapshot consumers in the GUI

        if is_current {
            self.current = Some(snapshot.clone());
        }
        if has_renderable {
            self.last_good = Some(snapshot);
        }
    }

    /// Publish a snapshot from the current session state without re-running rebuild.
    /// `success` indicates whether the most recent rebuild completed successfully.
    pub fn publish_rebuild_result(&mut self, success: bool) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.source.text().hash(&mut hasher);
        let hash = hasher.finish();

        let success = success
            && self.source.document.last_rebuild_error.is_none()
            && !self.source.document.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error
            });
        if success {
            // Skip re-publishing if current is already Clean with the same hash
            if let Some(ref current) = self.current
                && current.source_hash == crate::app::document::version::SourceHash(hash)
                && matches!(current.status, crate::app::document::snapshot::SnapshotStatus::Clean)
            {
                return;
            }
            let snapshot = snapshot_from_session(&self.source.document, hash);
            self.publish_snapshot(snapshot);
        } else {
            // Failed build — publish a Failed snapshot
            let snapshot = crate::app::document::snapshot::DocumentSnapshot {
                generation: self.generation.next(),
                source_epoch: self.source.epoch(),
                source_hash: crate::app::document::version::SourceHash(hash),
                status: crate::app::document::snapshot::SnapshotStatus::Failed {
                    error: "rebuild failed",
                },
                target: crate::app::document::snapshot::BuildTargetSnapshot::Empty,
                duration_s: self.source.document.duration_s,
                scene_dimensions: self.source.document.scene_dimensions,
            };
            self.publish_snapshot(snapshot);
        }
    }

    /// Clear all snapshots (used when opening a different file).
    pub fn clear_snapshots(&mut self) {
        self.generation = crate::app::document::version::DocumentGeneration::initial();
        self.current = None;
        self.last_good = None;
    }

    /// Returns true if the current snapshot is stale (source changed since last rebuild).
    pub fn snapshot_is_stale(&self) -> bool {
        self.current.as_ref().is_some_and(|c| {
            if matches!(c.status, crate::app::document::snapshot::SnapshotStatus::Stale { .. }) {
                return true;
            }
            c.source_epoch != self.source.epoch()
        })
    }

    /// Returns true if the current build failed and we're falling back to last_good.
    pub fn showing_last_good(&self) -> bool {
        let current_failed = self.current.as_ref().is_some_and(|c| {
            matches!(c.status, crate::app::document::snapshot::SnapshotStatus::Failed { .. })
                || !c.has_renderable_target()
        });
        current_failed && self.last_good.is_some()
    }

    /// Commit source text through DocumentStore, marking the current snapshot as stale.
    pub fn commit_source(
        &mut self,
        new_source: String,
        source_index: animatix_syntax::source_index::SourceIndex,
        ui_after: UiSnapshot,
    ) {
        self.source.commit_source(new_source, source_index);
        self.mark_source_stale(self.source.epoch());
        self.finalize_snapshot(ui_after);
    }

    /// Replace text through DocumentStore, marking the current snapshot as stale.
    pub fn replace_text(
        &mut self,
        text: String,
    ) -> crate::app::document::source_change::SourceChange {
        self.replace_text_with_ui(
            text,
            UiSnapshot::default_with_tool(crate::app::preview::ToolMode::Move),
        )
    }

    /// Replace text and record the UI state that accompanies the resulting source change.
    pub fn replace_text_with_ui(
        &mut self,
        text: String,
        ui_after: UiSnapshot,
    ) -> crate::app::document::source_change::SourceChange {
        let change = self.source.replace_text(text);
        self.mark_source_stale(change.after_epoch);
        self.finalize_snapshot(ui_after);
        change
    }

    /// Sync canonical source text from the live editor without resetting editor
    /// focus or re-parsing its in-memory cells.
    pub fn sync_from_editor(&mut self) -> crate::app::document::source_change::SourceChange {
        let change = self.source.sync_from_editor();
        self.mark_source_stale(change.after_epoch);
        self.finalize_snapshot(UiSnapshot::default_with_tool(crate::app::preview::ToolMode::Move));
        change
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
                self.current = Some(Arc::new(updated)); // Arc used for cheap cloning across snapshot consumers
            }
        }
    }
}

/// Build a DocumentSnapshot from the current DocumentSession state.
#[allow(clippy::arc_with_non_send_sync)] // Arc chosen for future async rebuild path compatibility
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
        target,
        duration_s: doc.duration_s,
        scene_dimensions: doc.scene_dimensions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::Command;
    use crate::app::document::history::UiSnapshot;
    use crate::app::document::snapshot::{BuildTargetSnapshot, SnapshotStatus};
    use crate::app::document::version::{DocumentGeneration, SourceEpoch, SourceHash};
    use crate::app::preview::ToolMode;
    use crate::app::test_support::document_store_empty;

    fn snapshot_with_timeline(
        g: DocumentGeneration,
        epoch: SourceEpoch,
        hash: u64,
    ) -> DocumentSnapshot {
        DocumentSnapshot {
            generation: g,
            source_epoch: epoch,
            source_hash: SourceHash(hash),
            status: SnapshotStatus::Clean,
            target: BuildTargetSnapshot::Timeline(Default::default()),
            duration_s: 10.0,
            scene_dimensions: animatix::timeline::SceneDimensions {
                width: 1920,
                height: 1080,
            },
        }
    }

    fn snapshot_failed(g: DocumentGeneration, epoch: SourceEpoch, hash: u64) -> DocumentSnapshot {
        DocumentSnapshot {
            generation: g,
            source_epoch: epoch,
            source_hash: SourceHash(hash),
            status: SnapshotStatus::Failed {
                error: "test failure",
            },
            target: BuildTargetSnapshot::Empty,
            duration_s: 0.1,
            scene_dimensions: animatix::timeline::SceneDimensions {
                width: 1920,
                height: 1080,
            },
        }
    }

    #[test]
    fn test_snapshot_finalizes_after_commit_source() {
        let mut store = document_store_empty();
        let before = store.source.text().to_string();
        let ui = UiSnapshot {
            active_scene: None,
            selected_actors: Default::default(),
            selected_keyframes: Vec::new(),
            playhead_time_s: 1.25,
            loop_start_s: Some(0.5),
            loop_end_s: Some(2.0),
            timeline_scroll_offset: 4.0,
            tool_mode: ToolMode::Move,
        };
        store.snapshot(Command::FindReplaceAll, ui.clone());

        let after = "rect(100, 100) at 0,0\n".to_string();
        let source_index = animatix_syntax::source_index::SourceIndex::build(&[]);
        store.commit_source(after.clone(), source_index, ui.clone());

        assert_eq!(store.history.undo_stack.len(), 1);
        let entry = store.history.undo_stack.back().unwrap();
        assert_eq!(entry.source_before, before);
        assert_eq!(entry.source_after, after);
        assert_eq!(entry.ui_before.playhead_time_s, 1.25);
        assert_eq!(entry.ui_after.timeline_scroll_offset, 4.0);
    }

    #[test]
    fn test_commit_source_invalidates_hit_region_cache() {
        let mut store = document_store_empty();
        store.source.cache_valid = true;
        store.source.cached_hit_regions.push(("box".to_string(), kurbo::Rect::ZERO));

        store.commit_source(
            "box: Rect, size: (100, 100)\n".to_string(),
            animatix_syntax::source_index::SourceIndex::build(&[]),
            UiSnapshot::default_with_tool(ToolMode::Move),
        );

        assert!(!store.source.cache_valid, "commit_source should invalidate cached hit regions");
        assert!(store.source.cached_hit_regions.is_empty());
    }

    #[test]
    fn test_abort_snapshot_does_not_push_history() {
        let mut store = document_store_empty();
        store.snapshot(Command::FindReplaceAll, UiSnapshot::default_with_tool(ToolMode::Move));
        store.abort_snapshot();
        store.replace_text("changed\n".to_string());
        assert!(store.history.undo_stack.is_empty());
    }

    #[test]
    fn test_with_mutation_finalizes_on_changed_source() {
        let mut store = document_store_empty();
        let result = store.with_mutation(
            Command::FindReplaceAll,
            UiSnapshot::default_with_tool(ToolMode::Move),
            UiSnapshot::default_with_tool(ToolMode::Select),
            |store| {
                store.replace_text("changed\n".to_string());
                Ok::<_, String>(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(store.history.undo_stack.len(), 1);
        assert!(store.pending_snapshot_is_none());
    }

    #[test]
    fn test_with_mutation_aborts_on_error() {
        let mut store = document_store_empty();
        let result = store.with_mutation(
            Command::FindReplaceAll,
            UiSnapshot::default_with_tool(ToolMode::Move),
            UiSnapshot::default_with_tool(ToolMode::Select),
            |_| Err::<(), _>("rejected"),
        );

        assert!(result.is_err());
        assert!(store.history.undo_stack.is_empty());
        assert!(store.pending_snapshot_is_none());
    }

    #[test]
    fn test_with_mutation_aborts_without_source_change() {
        let mut store = document_store_empty();
        let result = store.with_mutation(
            Command::FindReplaceAll,
            UiSnapshot::default_with_tool(ToolMode::Move),
            UiSnapshot::default_with_tool(ToolMode::Select),
            |_| Ok::<_, String>(()),
        );

        assert!(result.is_ok());
        assert!(store.history.undo_stack.is_empty());
        assert!(store.pending_snapshot_is_none());
    }

    #[test]
    fn test_replace_text_finalizes_pending_snapshot() {
        let mut store = document_store_empty();
        let before = store.source.text().to_string();
        store.snapshot(Command::FindReplaceAll, UiSnapshot::default_with_tool(ToolMode::Move));
        let after = "changed\n".to_string();
        store.replace_text(after.clone());

        assert_eq!(store.history.undo_stack.len(), 1);
        let entry = store.history.undo_stack.back().unwrap();
        assert_eq!(entry.source_before, before);
        assert_eq!(entry.source_after, after);
    }

    #[test]
    fn test_failed_publish_keeps_last_good() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Manually set a good snapshot so last_good is populated
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(store.last_good.is_some(), "should have last_good after successful publish");
        let good_gen = store.last_good.as_ref().unwrap().generation;

        // Publish failed snapshot (via the real API)
        store.publish_rebuild_result(false);

        // last_good should still be from the first publish (unchanged generation)
        assert!(store.last_good.is_some());
        assert_eq!(
            store.last_good.as_ref().unwrap().generation,
            good_gen,
            "last_good should be unchanged after failure"
        );

        // current should be failed
        assert!(store.current.is_some(), "current should exist after failed publish");
        assert!(
            matches!(store.current.as_ref().unwrap().status, SnapshotStatus::Failed { .. }),
            "current should be Failed"
        );

        assert!(
            store.showing_last_good(),
            "showing_last_good should be true when current failed and last_good exists"
        );
    }

    #[test]
    fn test_replace_text_marks_stale() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Publish a clean snapshot
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(store.current.is_some());
        assert!(!store.snapshot_is_stale(), "should not be stale initially");

        // Replace text
        store.replace_text("rect(100, 100) at 0,0\n".to_string());
        assert!(store.snapshot_is_stale(), "should be stale after replace_text");
    }

    #[test]
    fn test_successful_publish_restores_clean() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Publish Clean
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(matches!(store.current.as_ref().unwrap().status, SnapshotStatus::Clean));

        // Replace text -> Stale
        store.replace_text("rect(100, 100) at 0,0\n".to_string());
        assert!(matches!(store.current.as_ref().unwrap().status, SnapshotStatus::Stale { .. }));

        // Publish successful again -> restores Clean
        store.publish_rebuild_result(true);
        assert!(matches!(store.current.as_ref().unwrap().status, SnapshotStatus::Clean));
    }

    #[test]
    fn test_clear_snapshots() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Publish a snapshot with renderable target so both slots are set
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(store.current.is_some(), "current should exist");
        assert!(store.last_good.is_some(), "last_good should exist");

        // Clear
        store.clear_snapshots();
        assert!(store.current.is_none(), "current should be None after clear");
        assert!(store.last_good.is_none(), "last_good should be None after clear");
    }

    #[test]
    fn test_snapshot_is_stale_after_mark_stale() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Publish a clean snapshot
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(!store.snapshot_is_stale(), "should not be stale initially");

        // Increment epoch and mark stale
        let new_epoch = store.source.source_epoch.next();
        store.mark_source_stale(new_epoch);
        assert!(store.snapshot_is_stale(), "should be stale after mark_source_stale");
    }

    #[test]
    fn test_showing_last_good() {
        let mut store = document_store_empty();
        let e0 = store.source.epoch();

        // Initially, no snapshots
        assert!(!store.showing_last_good(), "should not show last good with no snapshots");

        // Publish a good snapshot
        let good = snapshot_with_timeline(DocumentGeneration::initial(), e0, 1234);
        store.publish_snapshot(good);
        assert!(!store.showing_last_good(), "should not show last good when current is clean");

        // Publish failed snapshot -> current is Failed, last_good still exists
        let failed = snapshot_failed(store.generation.next(), e0, 5678);
        store.publish_snapshot(failed);
        assert!(
            store.showing_last_good(),
            "should show last good when current failed and last_good exists"
        );
    }
}
