//! Change summary between two timeline builds.
//!
//! The GUI rebuilds the whole document on edits, but should preserve what the
//! user is looking at when the new build is structurally compatible. This module
//! summarizes the compiled-target change so handlers can keep time, active
//! scene, and selection when they still exist and report what was removed.

use std::collections::{BTreeSet, HashSet};

use crate::document::DocumentSession;

/// Stable view of the compiled target used as a diff baseline.
///
/// This is captured before a rebuild because applying a rebuild replaces the
/// previous `DocumentSession` data in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineFingerprint {
    actors: BTreeSet<String>,
    scenes: BTreeSet<String>,
    duration_ms: i64,
    keyframe_times: BTreeSet<u64>,
}

impl TimelineFingerprint {
    /// Capture the current compiled actor/scene/keyframe identity.
    pub fn from_document(document: &DocumentSession) -> Self {
        let mut actors = BTreeSet::new();
        if let Some(timeline) = &document.timeline {
            actors.extend(timeline.tracks().keys().cloned());
        } else if let Some(composition) = &document.composition {
            for scene in composition.scenes.values() {
                actors.extend(scene.timeline.tracks().keys().cloned());
            }
        }

        let scenes = document
            .composition
            .as_ref()
            .map(|composition| composition.declaration_order.iter().cloned().collect())
            .unwrap_or_default();
        let keyframe_times =
            document.timeline_index.keyframes.iter().map(|(time_ms, _)| *time_ms).collect();

        Self {
            actors,
            scenes,
            duration_ms: (document.duration_s * 1000.0).round() as i64,
            keyframe_times,
        }
    }

    /// Actor labels that survived into the current build.
    pub fn surviving_actors(&self, labels: impl IntoIterator<Item = String>) -> HashSet<String> {
        labels.into_iter().filter(|label| self.actors.contains(label)).collect()
    }

    /// Keyframes that still exist in the current build.
    pub fn surviving_keyframes(
        &self,
        keyframes: impl IntoIterator<Item = (String, String, u64)>,
    ) -> Vec<(String, String, u64)> {
        keyframes
            .into_iter()
            .filter(|(actor, _, time_ms)| {
                self.actors.contains(actor) && self.keyframe_times.contains(time_ms)
            })
            .collect()
    }
}

/// Structural difference between two timeline builds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineDiff {
    /// Actors present in the new build but not the previous one.
    pub added_actors: Vec<String>,
    /// Actors present in the previous build but not the new one.
    pub removed_actors: Vec<String>,
    /// Scenes present in the new composition but not the previous one.
    pub added_scenes: Vec<String>,
    /// Scenes present in the previous composition but not the new one.
    pub removed_scenes: Vec<String>,
    /// New duration minus previous duration, in milliseconds.
    pub duration_ms_delta: i64,
    /// Keyframe times present in the new build but not the previous one.
    pub added_keyframe_times: Vec<u64>,
    /// Keyframe times present in the previous build but not the new one.
    pub removed_keyframe_times: Vec<u64>,
}

impl TimelineDiff {
    /// Compute the difference between two fingerprints.
    pub fn between(previous: &TimelineFingerprint, current: &TimelineFingerprint) -> Self {
        Self {
            added_actors: current.actors.difference(&previous.actors).cloned().collect(),
            removed_actors: previous.actors.difference(&current.actors).cloned().collect(),
            added_scenes: current.scenes.difference(&previous.scenes).cloned().collect(),
            removed_scenes: previous.scenes.difference(&current.scenes).cloned().collect(),
            duration_ms_delta: current.duration_ms - previous.duration_ms,
            added_keyframe_times: current
                .keyframe_times
                .difference(&previous.keyframe_times)
                .copied()
                .collect(),
            removed_keyframe_times: previous
                .keyframe_times
                .difference(&current.keyframe_times)
                .copied()
                .collect(),
        }
    }
}

/// Pick the time to keep after a rebuild.
///
/// Keep the old playhead when the new duration still covers it. When the
/// timeline shrank, jump to the nearest surviving keyframe; fall back to the
/// end of the new duration if no keyframes remain.
pub fn preserved_time_s(previous_time_s: f64, document: &DocumentSession) -> f64 {
    let duration_s = document.duration_s.max(0.1);
    if previous_time_s <= duration_s {
        return previous_time_s;
    }

    let times = document
        .active_timeline()
        .map(|timeline| timeline.keyframe_times_s())
        .unwrap_or_default();
    times
        .into_iter()
        .filter(|time_s| *time_s <= duration_s)
        .min_by(|a, b| (a - previous_time_s).abs().total_cmp(&(b - previous_time_s).abs()))
        .unwrap_or(duration_s)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::document::DocumentSession;

    fn load_session(source: &str) -> DocumentSession {
        let mut session =
            DocumentSession::from_source(PathBuf::from("test.amx"), source.to_string()).unwrap();
        session.rebuild().expect("valid source should rebuild");
        session
    }

    #[test]
    fn diff_reports_added_and_removed_actors() {
        let previous =
            TimelineFingerprint::from_document(&load_session("#0s\nbox: Rect, size: (100, 100)\n"));
        let current = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\ncircle: Ellipse, radius: 20\n",
        ));

        let diff = TimelineDiff::between(&previous, &current);
        assert_eq!(diff.added_actors, vec!["circle"]);
        assert!(diff.removed_actors.is_empty());
    }

    #[test]
    fn diff_reports_removed_actors_and_duration_delta() {
        let previous = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));
        let current = TimelineFingerprint::from_document(&load_session("#0s\n"));

        let diff = TimelineDiff::between(&previous, &current);
        assert_eq!(diff.removed_actors, vec!["box"]);
        assert_eq!(diff.duration_ms_delta, -1900);
    }

    #[test]
    fn diff_reports_scene_changes() {
        let previous = TimelineFingerprint::from_document(&load_session(
            "# Intro\n#0s\ntitle: Text, text: \"Hi\"\n# Diagram\n#0s\ngraph: Rect\n",
        ));
        let current = TimelineFingerprint::from_document(&load_session(
            "# Intro\n#0s\ntitle: Text, text: \"Hi\"\n",
        ));

        let diff = TimelineDiff::between(&previous, &current);
        assert_eq!(diff.removed_scenes, vec!["Diagram"]);
        assert!(diff.removed_actors.iter().any(|label| label == "graph"));
    }

    #[test]
    fn no_op_build_has_empty_diff() {
        let source = "#0s\nbox: Rect, size: (100, 100)\n";
        let previous = TimelineFingerprint::from_document(&load_session(source));
        let current = TimelineFingerprint::from_document(&load_session(source));
        let diff = TimelineDiff::between(&previous, &current);

        assert_eq!(diff, TimelineDiff::default());
    }

    #[test]
    fn surviving_keyframes_filter_removed_actors_and_times() {
        let current =
            TimelineFingerprint::from_document(&load_session("#0s\nbox: Rect, size: (100, 100)\n"));

        let kept = current.surviving_keyframes(vec![
            ("box".to_string(), "size".to_string(), 0),
            ("box".to_string(), "color".to_string(), 2000),
            ("gone".to_string(), "color".to_string(), 0),
        ]);

        assert_eq!(kept, vec![("box".to_string(), "size".to_string(), 0)]);
    }

    #[test]
    fn preserved_time_keeps_in_bounds_playhead() {
        let document = load_session("#0s\nbox: Rect\n#2s\nbox.color = red\n");
        assert_eq!(preserved_time_s(1.5, &document), 1.5);
    }

    #[test]
    fn preserved_time_uses_nearest_keyframe_after_shrink() {
        let document = load_session("#0s\nbox: Rect\n#2s\nbox.color = red\n");
        assert_eq!(preserved_time_s(3.5, &document), 2.0);
    }

    #[test]
    fn preserved_time_uses_default_keyframe_when_no_authored_keyframes() {
        let document = load_session("");
        assert_eq!(preserved_time_s(5.0, &document), 0.0);
    }
}
