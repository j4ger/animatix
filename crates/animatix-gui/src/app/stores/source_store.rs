use crate::app::document::source_change::SourceChange;
use crate::app::document::version::SourceEpoch;
use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use kurbo::Rect;
use std::collections::HashMap;

/// Owns the canonical document text (via EditorBuffer) and the compiled
/// timeline (via DocumentSession). This is the single source of truth for
/// everything that can be saved to disk.
pub struct SourceStore {
    pub document: DocumentSession,
    pub editor: EditorBuffer,

    // ── Source version tracking ──
    pub source_epoch: SourceEpoch,

    // ── Cached hot-path allocations ──
    /// Cached actor labels from the timeline, to avoid re-collecting every frame.
    pub cached_actor_labels: Vec<String>,
    /// Cached per-actor keyframe property lists (actor_label, keyframes).
    pub cached_actor_keyframes: Vec<(String, Vec<(u64, &'static str)>)>,
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
            cached_actor_labels: Vec::new(),
            cached_actor_keyframes: Vec::new(),
            cached_hit_regions: Vec::new(),
            cached_actor_bounds: HashMap::new(),
            cache_valid: false,
        }
    }

    /// Mark all cached hot-path data as stale.
    /// Call this whenever the timeline is rebuilt or the document text changes.
    pub fn invalidate_cache(&mut self) {
        self.cached_actor_labels.clear();
        self.cached_actor_keyframes.clear();
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
    }

    pub fn text(&self) -> &str {
        &self.document.source_text
    }

    pub fn file_path(&self) -> &std::path::Path {
        &self.document.file_path
    }

    #[allow(dead_code)] // Accessors for epoch and dirty state used by future background rebuild integration.
    /// Accessors for epoch and dirty state used by future background rebuild integration.
    pub fn epoch(&self) -> SourceEpoch {
        self.source_epoch
    }

    #[allow(dead_code)] // Accessors for epoch and dirty state used by future background rebuild integration.
    /// Accessors for epoch and dirty state used by future background rebuild integration.
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
}

/// Rebuild cached actor labels, per-actor keyframe lists, hit regions, and actor
/// bounds from the timeline.  This is a free function to
/// avoid borrow conflicts when called from behavior.rs.
pub fn rebuild_cache(
    cached_actor_labels: &mut Vec<String>,
    cached_actor_keyframes: &mut Vec<(String, Vec<(u64, &'static str)>)>,
    cached_hit_regions: &mut Vec<(String, Rect)>,
    cached_actor_bounds: &mut HashMap<String, Rect>,
    cache_valid: &mut bool,
    timeline: Option<&animatix::timeline::Timeline>,
) {
    let labels: Vec<String> =
        timeline.map(|tl| tl.root_actor_labels().to_vec()).unwrap_or_default();
    let keyframes: Vec<(String, Vec<(u64, &'static str)>)> = labels
        .iter()
        .map(|label| {
            let props = timeline
                .and_then(|tl| tl.get_track(label))
                .map(|track| {
                    let mut result = Vec::new();
                    push_kf_props(track, &mut result);
                    result
                })
                .unwrap_or_default();
            (label.clone(), props)
        })
        .collect();

    // Populate hit_regions and actor_bounds from the timeline
    let hit_regions: Vec<(String, Rect)> = timeline.map(|tl| tl.hit_regions()).unwrap_or_default();
    let actor_bounds: HashMap<String, Rect> =
        hit_regions.iter().map(|(label, bounds)| (label.clone(), *bounds)).collect();

    *cached_actor_labels = labels;
    *cached_actor_keyframes = keyframes;
    *cached_hit_regions = hit_regions;
    *cached_actor_bounds = actor_bounds;

    *cache_valid = true;
}

// ── Helper: push keyframe times for all animated properties on a track ──
fn push_kf_props(track: &animatix::timeline::AnimationTrack, result: &mut Vec<(u64, &'static str)>) {
    let indices = animatix::timeline::allowed_property_indices(track.kind);
    for idx in indices {
        let schema = &animatix::timeline::PROPERTY_REGISTRY[idx];
        for ms in animatix::timeline::property_keyframe_times(track, schema.field) {
            result.push((ms, schema.name));
        }
    }
    result.sort_by_key(|(ms, _)| *ms);
    result.dedup_by(|a, b| a.0 == b.0);
}
