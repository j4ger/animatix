use std::collections::HashMap;

use kurbo::Rect;

use crate::app::document::source_change::SourceChange;
use crate::app::document::version::SourceEpoch;
use crate::document::DocumentSession;
use crate::editor::EditorBuffer;

/// Owns the canonical document text (via EditorBuffer) and the compiled
/// timeline (via DocumentSession). This is the single source of truth for
/// everything that can be saved to disk.
pub struct SourceStore {
    pub document: DocumentSession,
    pub editor: EditorBuffer,

    // ── Source version tracking ──
    pub source_epoch: SourceEpoch,

    // ── Cached hot-path allocations ──
    /// Cached per-actor world-space hit regions (actor_label, bounds rect).
    pub cached_hit_regions: Vec<(String, Rect)>,
    /// Cached per-actor world-space bounds, keyed by actor label.
    pub cached_actor_bounds: HashMap<String, Rect>,
    /// Flag; when false, cached fields must be recomputed.
    pub cache_valid: bool,
}

impl SourceStore {
    pub fn new(document: DocumentSession, editor: EditorBuffer) -> Self {
        Self {
            document,
            editor,
            source_epoch: SourceEpoch::initial(),
            cached_hit_regions: Vec::new(),
            cached_actor_bounds: HashMap::new(),
            cache_valid: false,
        }
    }

    /// Mark all cached hot-path data as stale.
    /// Call this whenever the timeline is rebuilt or the document text changes.
    pub fn invalidate_cache(&mut self) {
        self.cached_hit_regions.clear();
        self.cached_actor_bounds.clear();
        self.cache_valid = false;
    }

    /// Apply pre-computed source text and source index to the document.
    ///
    /// Centralizes the boilerplate that was previously duplicated across handlers.
    /// Callers compute `(stmts_to_source(stmts), SourceIndex::build(stmts))` while
    /// holding the `stmts` borrow, then call this after the borrow ends.
    pub fn commit_source(
        &mut self,
        new_source: String,
        source_index: animatix_syntax::source_index::SourceIndex,
    ) {
        self.source_epoch = self.source_epoch.next();
        self.document.source_text = new_source.clone();
        self.document.is_dirty = true;
        self.editor.replace_text(new_source);
        self.document.source_index = Some(source_index);
        self.invalidate_cache();
    }

    pub fn text(&self) -> &str {
        &self.document.source_text
    }

    pub fn file_path(&self) -> &std::path::Path {
        &self.document.file_path
    }

    pub fn epoch(&self) -> SourceEpoch {
        self.source_epoch
    }

    pub fn is_dirty(&self) -> bool {
        self.document.is_dirty
    }

    pub fn mark_saved(&mut self) {
        self.document.is_dirty = false;
    }

    /// Replace source text from an external change (editor, undo, reload).
    /// Returns a `SourceChange` with the new epoch.
    pub fn replace_text(&mut self, text: String) -> SourceChange {
        let before_epoch = self.source_epoch;
        self.source_epoch = self.source_epoch.next();
        self.document.source_text = text.clone();
        self.document.is_dirty = true;
        self.editor.replace_text(text.clone());
        self.invalidate_cache();
        SourceChange {
            before_epoch,
            after_epoch: self.source_epoch,
            source_len: self.document.source_text.len(),
        }
    }

    /// Sync canonical source text from the live editor without replacing its
    /// cell state. The editor is already the source of truth while typing, so
    /// re-parsing here would drop in-memory cells and reset focus mid-edit.
    pub fn sync_from_editor(&mut self) -> SourceChange {
        let before_epoch = self.source_epoch;
        let text = self.editor.text().to_string();
        self.source_epoch = self.source_epoch.next();
        self.document.source_text = text;
        self.document.is_dirty = true;
        self.invalidate_cache();
        SourceChange {
            before_epoch,
            after_epoch: self.source_epoch,
            source_len: self.document.source_text.len(),
        }
    }
}

/// Rebuild cached hit regions and actor bounds from the timeline. This is a free
/// function to avoid borrow conflicts when called from behavior.rs.
pub fn rebuild_cache(
    cached_hit_regions: &mut Vec<(String, Rect)>,
    cached_actor_bounds: &mut HashMap<String, Rect>,
    cache_valid: &mut bool,
    timeline: Option<&animatix::timeline::Timeline>,
) {
    // Populate hit_regions and actor_bounds from the timeline
    let hit_regions: Vec<(String, Rect)> = timeline.map(|tl| tl.hit_regions()).unwrap_or_default();
    let actor_bounds: HashMap<String, Rect> =
        hit_regions.iter().map(|(label, bounds)| (label.clone(), *bounds)).collect();

    *cached_hit_regions = hit_regions;
    *cached_actor_bounds = actor_bounds;

    *cache_valid = true;
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::document::DocumentSession;
    use crate::editor::EditorBuffer;

    #[test]
    fn sync_from_editor_preserves_live_cell_state() {
        let path = PathBuf::from("test.amx");
        // A leading blank line parses as an empty code cell followed by a keyframe.
        let source = "\n#0s\n";
        let mut document =
            DocumentSession::from_source(path.clone(), source.to_string()).expect("valid source");
        document.rebuild().expect("valid source should rebuild");
        let mut editor = EditorBuffer::new(&path, source.to_string());
        let parsed = crate::cell_editor::parse_cells(source);
        assert!(matches!(parsed.first(), Some(crate::cell_editor::Cell::Code { .. })));
        assert!(matches!(parsed.get(1), Some(crate::cell_editor::Cell::Keyframe { .. })));
        editor.set_focused_cell(Some(1));

        let mut store = SourceStore::new(document, editor);
        let change = store.sync_from_editor();

        assert_eq!(store.text(), source);
        assert_eq!(store.editor.focused_cell(), Some(1));
        assert!(store.document.is_dirty);
        assert_eq!(store.epoch(), change.after_epoch);
    }
}
