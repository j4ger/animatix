//! Change summary between two timeline builds.
//!
//! The GUI rebuilds the whole document on edits, but should preserve what the
//! user is looking at when the new build is structurally compatible. This module
//! summarizes the compiled-target change so handlers can keep time, active
//! scene, and selection when they still exist and report what was removed.

use std::collections::{BTreeSet, HashSet};

use crate::document::DocumentSession;

/// Stable identity of a keyframe selection: (actor, property, time_ms).
pub type KeyframeId = (String, String, u64);

/// Collect per-property keyframe times for the canonical property set used by
/// timeline selection.
///
/// This is the single source for keyframe identities shared by the timeline
/// panel and rebuild diff. It deliberately enumerates typed track fields rather
/// than `PROPERTY_REGISTRY`, because registry aliases such as
/// `background_color -> ActorField::Color` would otherwise create duplicate
/// phantom identities for the same keyframes.
pub(crate) fn collect_per_property_keyframes(
    track: &animatix::timeline::AnimationTrack,
) -> Vec<(&'static str, Vec<u64>)> {
    let mut result = Vec::new();
    use animatix::timeline::{Interpolate, PropertyTrack};
    fn push<T: Interpolate>(
        result: &mut Vec<(&'static str, Vec<u64>)>,
        opt: &Option<PropertyTrack<T>>,
        name: &'static str,
    ) {
        if let Some(pt) = opt {
            if !pt.keyframes().is_empty() {
                result.push((name, pt.keyframes().keys().copied().collect()));
            }
        }
    }
    // Geometry
    push(&mut result, &track.geometry.position, "position");
    push(&mut result, &track.geometry.motion_offset, "motion_offset");
    push(&mut result, &track.geometry.rotation, "rotation");
    push(&mut result, &track.geometry.scale, "scale");
    push(&mut result, &track.geometry.size, "size");
    push(&mut result, &track.geometry.layout_size, "layout_size");
    // Style
    push(&mut result, &track.style.color, "color");
    push(&mut result, &track.style.opacity, "opacity");
    push(&mut result, &track.style.stroke_width, "stroke_width");
    push(&mut result, &track.style.stroke_color, "stroke_color");
    push(&mut result, &track.style.stroke_progress, "stroke_progress");
    push(&mut result, &track.style.fill_opacity, "fill_opacity");
    push(&mut result, &track.style.line_cap, "line_cap");
    push(&mut result, &track.style.line_join, "line_join");
    // Text
    push(&mut result, &track.text.text_content, "text_content");
    push(&mut result, &track.text.font_family, "font_family");
    push(&mut result, &track.text.font_size, "font_size");
    // Shape
    push(&mut result, &track.shape.shape_type, "shape_type");
    push(&mut result, &track.shape.line_from, "line_from");
    push(&mut result, &track.shape.line_to, "line_to");
    push(&mut result, &track.shape.arc_angles, "arc_angles");
    push(&mut result, &track.shape.points, "points");
    push(&mut result, &track.shape.commands, "commands");
    push(&mut result, &track.shape.vector_paths, "vector_paths");
    push(&mut result, &track.shape.head_size, "head_size");
    // Filter
    push(&mut result, &track.filter.filter_blur, "filter_blur");
    push(&mut result, &track.filter.filter_brightness, "filter_brightness");
    push(&mut result, &track.filter.filter_contrast, "filter_contrast");
    push(&mut result, &track.filter.filter_saturate, "filter_saturate");
    push(&mut result, &track.filter.filter_hue_rotate, "filter_hue_rotate");
    push(&mut result, &track.filter.filter_sepia, "filter_sepia");
    result
}

/// Stable view of the compiled target used as a diff baseline.
///
/// This is captured before a rebuild because applying a rebuild replaces the
/// previous `DocumentSession` data in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineFingerprint {
    actors: BTreeSet<String>,
    scenes: BTreeSet<String>,
    duration_ms: i64,
    keyframes: BTreeSet<KeyframeId>,
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
        let keyframes = document
            .active_timeline()
            .map(|timeline| {
                let mut ids = BTreeSet::new();
                for track in timeline.tracks().values() {
                    for (property, times) in collect_per_property_keyframes(track) {
                        for time_ms in times {
                            ids.insert((track.label.clone(), property.to_string(), time_ms));
                        }
                    }
                }
                ids
            })
            .unwrap_or_default();

        Self {
            actors,
            scenes,
            duration_ms: (document.duration_s * 1000.0).round() as i64,
            keyframes,
        }
    }

    /// Actor labels that survived into the current build.
    pub fn surviving_actors(&self, labels: impl IntoIterator<Item = String>) -> HashSet<String> {
        labels.into_iter().filter(|label| self.actors.contains(label)).collect()
    }

    /// Keyframes that still exist in the current build.
    pub fn surviving_keyframes(
        &self,
        keyframes: impl IntoIterator<Item = KeyframeId>,
    ) -> Vec<KeyframeId> {
        keyframes
            .into_iter()
            .filter(|keyframe| self.keyframes.contains(keyframe))
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
    /// Keyframe identities present in the new build but not the previous one.
    pub added_keyframes: Vec<KeyframeId>,
    /// Keyframe identities present in the previous build but not the new one.
    pub removed_keyframes: Vec<KeyframeId>,
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
            added_keyframes: current.keyframes.difference(&previous.keyframes).cloned().collect(),
            removed_keyframes: previous.keyframes.difference(&current.keyframes).cloned().collect(),
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
    fn surviving_keyframes_filter_removed_actors_and_properties() {
        let current = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));

        let kept = current.surviving_keyframes(vec![
            ("box".to_string(), "size".to_string(), 0),
            ("box".to_string(), "color".to_string(), 2000),
            ("box".to_string(), "position".to_string(), 2000),
            ("gone".to_string(), "color".to_string(), 2000),
        ]);

        assert_eq!(
            kept,
            vec![
                ("box".to_string(), "size".to_string(), 0),
                ("box".to_string(), "color".to_string(), 2000)
            ]
        );
    }

    #[test]
    fn diff_reports_removed_property_keyframe_identity() {
        let previous = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));
        let current = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.position = (200, 0)\n",
        ));

        let diff = TimelineDiff::between(&previous, &current);
        assert!(diff.removed_keyframes.iter().any(|(actor, property, time_ms)| {
            actor == "box" && property == "color" && *time_ms == 2000
        }));
        assert!(diff.added_keyframes.iter().any(|(actor, property, time_ms)| {
            actor == "box" && property == "position" && *time_ms == 2000
        }));
    }

    #[test]
    fn fingerprint_does_not_emit_registry_alias_keyframes() {
        let current = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));

        assert!(
            current
                .surviving_keyframes(vec![(
                    "box".to_string(),
                    "background_color".to_string(),
                    2000
                )])
                .is_empty(),
            "background_color aliases ActorField::Color and must not create a separate identity"
        );
        assert_eq!(
            current.surviving_keyframes(vec![("box".to_string(), "color".to_string(), 2000)]),
            vec![("box".to_string(), "color".to_string(), 2000)]
        );
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
