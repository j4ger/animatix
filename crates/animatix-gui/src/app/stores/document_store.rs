use crate::document::DocumentSession;
use crate::editor::EditorBuffer;
use crate::app::commands::{Command, UndoEntry};
use animatix::diagnostics::Diagnostic;
use animatix::diagnostics::diagnostics_phase_summary;
use std::collections::HashMap;
use kurbo::Rect;

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
            cached_actor_labels: Vec::new(),
            cached_actor_keyframes: Vec::new(),
            cached_hit_regions: Vec::new(),
            cached_actor_bounds: HashMap::new(),
            cache_valid: false,
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

    /// Mark all cached hot-path data as stale.
    /// Call this whenever the timeline is rebuilt or the document text changes.
    pub fn invalidate_cache(&mut self) {
        self.cached_actor_labels.clear();
        self.cached_actor_keyframes.clear();
        self.cached_hit_regions.clear();
        self.cached_actor_bounds.clear();
        self.cache_valid = false;
    }
}

/// Rebuild cached actor labels, per-actor keyframe lists, hit regions, and actor
/// bounds from the timeline.  This is a free function to avoid borrow conflicts
/// when called from behavior.rs.
pub fn rebuild_cache(
    cached_actor_labels: &mut Vec<String>,
    cached_actor_keyframes: &mut Vec<(String, Vec<(u64, &'static str)>)>,
    cached_hit_regions: &mut Vec<(String, Rect)>,
    cached_actor_bounds: &mut HashMap<String, Rect>,
    cache_valid: &mut bool,
    timeline: Option<&animatix::timeline::Timeline>,
) {
    let labels: Vec<String> = timeline.map(|tl| tl.root_actor_labels().to_vec()).unwrap_or_default();
    let keyframes: Vec<(String, Vec<(u64, &'static str)>)> = labels
        .iter()
        .map(|label| {
            let props = timeline
                .and_then(|tl| tl.get_track(label))
                .map(|track| {
                    let mut result = Vec::new();
                    push_kf_props(&mut result, &track.position, "position");
                    push_kf_props(&mut result, &track.motion_offset, "motion_offset");
                    push_kf_props(&mut result, &track.rotation, "rotation");
                    push_kf_props(&mut result, &track.scale, "scale");
                    push_kf_props(&mut result, &track.size, "size");
                    push_kf_props(&mut result, &track.color, "color");
                    push_kf_props(&mut result, &track.opacity, "opacity");
                    push_kf_props(&mut result, &track.stroke_width, "stroke_width");
                    push_kf_props(&mut result, &track.stroke_color, "stroke_color");
                    push_kf_props(&mut result, &track.stroke_progress, "stroke_progress");
                    push_kf_props(&mut result, &track.fill_opacity, "fill_opacity");
                    push_kf_props(&mut result, &track.text_content, "text_content");
                    push_kf_props(&mut result, &track.font_family, "font_family");
                    push_kf_props(&mut result, &track.font_size, "font_size");
                    push_kf_props(&mut result, &track.shape_type, "shape_type");
                    push_kf_props(&mut result, &track.line_from, "line_from");
                    push_kf_props(&mut result, &track.line_to, "line_to");
                    push_kf_props(&mut result, &track.arc_angles, "arc_angles");
                    push_kf_props(&mut result, &track.points, "points");
                    push_kf_props(&mut result, &track.commands, "commands");
                    push_kf_props(&mut result, &track.layout_size, "layout_size");
                    push_kf_props(&mut result, &track.vector_paths, "vector_paths");
                    result.sort_by_key(|(ms, _)| *ms);
                    result.dedup_by(|a, b| a.0 == b.0);
                    result
                })
                .unwrap_or_default();
            (label.clone(), props)
        })
        .collect();

    // Populate hit_regions and actor_bounds from the timeline
    let hit_regions: Vec<(String, Rect)> = timeline
        .map(|tl| tl.hit_regions())
        .unwrap_or_default();
    let actor_bounds: HashMap<String, Rect> = hit_regions
        .iter()
        .map(|(label, bounds)| (label.clone(), *bounds))
        .collect();

    *cached_actor_labels = labels;
    *cached_actor_keyframes = keyframes;
    *cached_hit_regions = hit_regions;
    *cached_actor_bounds = actor_bounds;
    *cache_valid = true;
}

// ── Helper: push keyframe times for a property track ──
fn push_kf_props(result: &mut Vec<(u64, &'static str)>, opt: &Option<impl KeyframeSource>, name: &'static str) {
    if let Some(pt) = opt {
        result.extend(pt.keyframe_times().into_iter().map(|ms| (ms, name)));
    }
}

trait KeyframeSource {
    fn keyframe_times(&self) -> Vec<u64>;
}

impl<T> KeyframeSource for animatix::timeline::PropertyTrack<T> {
    fn keyframe_times(&self) -> Vec<u64> {
        self.keyframes.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::Command;
    use std::path::PathBuf;

    /// Helper to create a minimal DocumentStore for testing.
    fn make_store() -> DocumentStore {
        let document = DocumentSession::from_error(PathBuf::from("test.amx"));
        let editor = EditorBuffer::new(&PathBuf::from("test.amx"), document.source_text.clone());
        DocumentStore::new(document, editor)
    }

    #[test]
    fn snapshot_pushes_onto_undo_stack_and_clears_redo() {
        let mut store = make_store();

        store.snapshot(Command::Rebuild);
        store.snapshot(Command::TogglePlayback);
        store.snapshot(Command::Save);

        assert_eq!(store.undo_stack.len(), 3);
        assert!(store.redo_stack.is_empty());
    }

    #[test]
    fn snapshot_removes_oldest_when_exceeding_limit() {
        let mut store = make_store();
        store.undo_limit = 2;

        store.snapshot(Command::Rebuild);
        store.snapshot(Command::Save);
        store.snapshot(Command::TogglePlayback);

        assert_eq!(store.undo_stack.len(), 2);
        // The oldest entry (Rebuild) should have been removed
        assert!(matches!(store.undo_stack[0].command, Command::Save));
        assert!(matches!(store.undo_stack[1].command, Command::TogglePlayback));
    }

    #[test]
    fn snapshot_clears_redo_stack() {
        let mut store = make_store();

        store.snapshot(Command::Rebuild);
        // Simulate an undo by moving one entry from undo to redo
        let entry = store.undo_stack.pop().unwrap();
        store.redo_stack.push(entry);

        assert_eq!(store.undo_stack.len(), 0);
        assert_eq!(store.redo_stack.len(), 1);

        // Pushing a new snapshot should clear redo
        store.snapshot(Command::Save);
        assert!(store.redo_stack.is_empty());
        assert_eq!(store.undo_stack.len(), 1);
    }

    #[test]
    fn invalidate_cache_sets_cache_valid_to_false() {
        let mut store = make_store();
        store.cache_valid = true;

        store.invalidate_cache();

        assert!(!store.cache_valid);
    }

    #[test]
    fn cached_actor_labels_can_be_set_and_read() {
        let mut store = make_store();
        assert!(store.cached_actor_labels.is_empty());

        let labels = vec!["box".to_string(), "circle".to_string()];
        store.cached_actor_labels = labels.clone();

        assert_eq!(store.cached_actor_labels, labels);
    }

    #[test]
    fn cached_hit_regions_can_be_set_and_read() {
        let mut store = make_store();
        assert!(store.cached_hit_regions.is_empty());

        store.cached_hit_regions.push(("actor1".to_string(), Rect::new(0.0, 0.0, 100.0, 100.0)));
        assert_eq!(store.cached_hit_regions.len(), 1);
        assert_eq!(store.cached_hit_regions[0].0, "actor1");
    }

    #[test]
    fn cached_actor_bounds_can_be_set_and_read() {
        let mut store = make_store();
        assert!(store.cached_actor_bounds.is_empty());

        store.cached_actor_bounds.insert("actor1".to_string(), Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(store.cached_actor_bounds.len(), 1);
        assert_eq!(store.cached_actor_bounds["actor1"], Rect::new(0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn invalidate_cache_clears_all_cached_fields() {
        let mut store = make_store();
        store.cached_actor_labels.push("actor1".to_string());
        store.cached_actor_keyframes.push(("actor1".to_string(), vec![(0, "position")]));
        store.cached_hit_regions.push(("actor1".to_string(), Rect::ZERO));
        store.cached_actor_bounds.insert("actor1".to_string(), Rect::ZERO);
        store.cache_valid = true;

        store.invalidate_cache();

        assert!(store.cached_actor_labels.is_empty());
        assert!(store.cached_actor_keyframes.is_empty());
        assert!(store.cached_hit_regions.is_empty());
        assert!(store.cached_actor_bounds.is_empty());
        assert!(!store.cache_valid);
    }
}