//! Change summary between two timeline builds.
//!
//! The GUI rebuilds the whole document on edits, but should preserve what the
//! user is looking at when the new build is structurally compatible. This module
//! summarizes the compiled-target change so handlers can keep time, active
//! scene, and selection when they still exist and report what was removed.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use animatix::timeline::ActorField;

use crate::document::DocumentSession;

/// Stable identity of a keyframe selection.
///
/// `scene` is `None` for a single-scene document and `Some(name)` for a named
/// scene in a composition. Time is scene-local milliseconds, which is stable
/// against composition reordering and scene duration changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyframeId {
    pub scene: Option<String>,
    pub actor: String,
    pub property: String,
    pub time_ms: u64,
}

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
) -> Vec<(String, Vec<u64>)> {
    let mut result = Vec::new();
    use animatix::timeline::{Interpolate, PropertyTrack};
    fn push<T: Interpolate>(
        result: &mut Vec<(String, Vec<u64>)>,
        opt: &Option<PropertyTrack<T>>,
        name: &'static str,
    ) {
        if let Some(pt) = opt {
            if !pt.keyframes().is_empty() {
                result.push((name.to_string(), pt.keyframes().keys().copied().collect()));
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
    // Effect stages: one lane per declared parameter plus the implicit `enabled`.
    for stage in &track.effects.stages {
        let Some(effect) = animatix::timeline::effects::effect(&stage.kind) else {
            continue;
        };
        for spec in effect.params() {
            let times = stage
                .params
                .get(spec.name.as_ref())
                .map(|param| param.keyframe_times())
                .unwrap_or_default();
            if !times.is_empty() {
                result.push((format!("{}.{}", stage.label, spec.name), times));
            }
        }
        let enabled_times = stage.enabled.keyframe_times();
        if !enabled_times.is_empty() {
            result.push((format!("{}.enabled", stage.label), enabled_times));
        }
    }
    result
}

/// Canonical lane name and storage field for every keyframe-addressable
/// property, in the order expanded property lanes render.
///
/// Names match [`collect_per_property_keyframes`] exactly so a lane identity
/// lines up with keyframe selection and diff identities. Each `ActorField`
/// appears once: registry aliases such as `background_color -> ActorField::Color`
/// collapse onto the storage field and never create a duplicate lane.
const PROPERTY_LANES: &[(&str, ActorField)] = &[
    // Transform
    ("position", ActorField::Position),
    ("motion_offset", ActorField::MotionOffset),
    ("rotation", ActorField::Rotation),
    ("scale", ActorField::Scale),
    ("size", ActorField::Size),
    ("layout_size", ActorField::LayoutSize),
    // Style
    ("color", ActorField::Color),
    ("opacity", ActorField::Opacity),
    ("stroke_width", ActorField::StrokeWidth),
    ("stroke_color", ActorField::StrokeColor),
    ("stroke_progress", ActorField::StrokeProgress),
    ("fill_opacity", ActorField::FillOpacity),
    ("line_cap", ActorField::LineCap),
    ("line_join", ActorField::LineJoin),
    // Shape
    ("shape_type", ActorField::ShapeType),
    ("line_from", ActorField::LineFrom),
    ("line_to", ActorField::LineTo),
    ("arc_angles", ActorField::ArcAngles),
    ("points", ActorField::Points),
    ("commands", ActorField::Commands),
    ("vector_paths", ActorField::VectorPaths),
    ("head_size", ActorField::HeadSize),
    // Text
    ("text_content", ActorField::TextContent),
    ("font_family", ActorField::FontFamily),
    ("font_size", ActorField::FontSize),
];

/// Collect every animatable property lane for an actor kind, including lanes
/// whose property has never been keyframed (empty time list).
///
/// A lane is included when the registry marks its storage field applicable to
/// the actor kind, or when the property already carries keyframes. Names are
/// unique and match [`collect_per_property_keyframes`], which remains the
/// authority for actual keyframe times.
pub(crate) fn collect_property_lanes(
    track: &animatix::timeline::AnimationTrack,
) -> Vec<(String, Vec<u64>)> {
    use animatix::timeline::{
        PROPERTY_REGISTRY, allowed_property_indices, property_keyframe_times,
    };

    // Storage fields the registry allows for this actor kind. Keying by field
    // dedupes aliases that share one storage location.
    let allowed_fields: Vec<ActorField> = allowed_property_indices(&track.caps, &track.actor_type)
        .into_iter()
        .map(|idx| PROPERTY_REGISTRY[idx].field)
        .collect();
    // Fields with real keyframes keep a lane even when the registry does not
    // expose their storage field directly (e.g. `arc_angles`, `vector_paths`).
    let keyframed: Vec<String> = collect_per_property_keyframes(track)
        .into_iter()
        .map(|(property, _)| property)
        .collect();

    let mut lanes: Vec<(String, Vec<u64>)> = PROPERTY_LANES
        .iter()
        .filter(|(property, field)| {
            allowed_fields.contains(field) || keyframed.iter().any(|name| name == property)
        })
        .map(|&(property, field)| (property.to_string(), property_keyframe_times(track, field)))
        .collect();

    // Effect stage lanes exist whenever the scope declares the stage.
    for stage in &track.effects.stages {
        let Some(effect) = animatix::timeline::effects::effect(&stage.kind) else {
            continue;
        };
        for spec in effect.params() {
            let times = stage
                .params
                .get(spec.name.as_ref())
                .map(|param| param.keyframe_times())
                .unwrap_or_default();
            lanes.push((format!("{}.{}", stage.label, spec.name), times));
        }
        lanes.push((format!("{}.enabled", stage.label), stage.enabled.keyframe_times()));
    }

    lanes
}

/// Resolve the writable registry schema that owns a property lane.
///
/// Lane names are typed-identity names (e.g. `motion_offset`, `filter_blur`)
/// that do not always equal the source-text property name (`shift`, `blur`).
/// This maps a lane back to the applicable schema so callers can key it with
/// the canonical source name while reading the current value from the same
/// storage field.
pub(crate) fn lane_schema(
    caps: &animatix::timeline::ActorCaps,
    actor_type: &str,
    lane: &str,
) -> Option<&'static animatix::timeline::PropertySchema> {
    use animatix::timeline::{PROPERTY_REGISTRY, allowed_property_indices};

    let field = PROPERTY_LANES.iter().find(|(name, _)| *name == lane).map(|(_, field)| *field)?;
    let allowed = || {
        allowed_property_indices(caps, actor_type)
            .into_iter()
            .map(|idx| &PROPERTY_REGISTRY[idx])
    };
    // Prefer a schema whose canonical name equals the lane name. This avoids
    // matching derived component schemas first (e.g. `height`/`width` share
    // `ActorField::Size` with `size`).
    allowed()
        .find(|schema| schema.name == lane)
        .or_else(|| allowed().find(|schema| schema.field == field))
}

/// Collect one flattened keyframe time per actor property, sorted by time.
///
/// When multiple properties share a time, only one entry is retained because
/// timeline UI renders one diamond per actor/time.
pub(crate) fn collect_actor_keyframes(
    track: &animatix::timeline::AnimationTrack,
) -> Vec<(u64, String)> {
    let mut result = Vec::new();
    for (property, times) in collect_per_property_keyframes(track) {
        result.extend(times.into_iter().map(|time_ms| (time_ms, property.clone())));
    }
    result.sort_by_key(|(time_ms, _)| *time_ms);
    result.dedup_by(|a, b| a.0 == b.0);
    result
}

/// Stable view of the compiled target used as a diff baseline.
///
/// This is captured before a rebuild because applying a rebuild replaces the
/// previous `DocumentSession` data in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimelineFingerprint {
    /// Union across all scenes, used for structural change reporting.
    actors: BTreeSet<String>,
    /// Actor labels per scene. `None` is the single-scene document.
    actors_by_scene: BTreeMap<Option<String>, BTreeSet<String>>,
    scenes: BTreeSet<String>,
    duration_ms: i64,
    keyframes: BTreeSet<KeyframeId>,
}

impl TimelineFingerprint {
    /// Capture the current compiled actor/scene/keyframe identity.
    pub fn from_document(document: &DocumentSession) -> Self {
        let mut actors = BTreeSet::new();
        let mut actors_by_scene: BTreeMap<Option<String>, BTreeSet<String>> = BTreeMap::new();
        let mut keyframes = BTreeSet::new();

        if let Some(timeline) = &document.timeline {
            actors.extend(timeline.tracks().keys().cloned());
            actors_by_scene.insert(None, timeline.tracks().keys().cloned().collect());
            for track in timeline.tracks().values() {
                for (property, times) in collect_per_property_keyframes(track) {
                    for time_ms in times {
                        keyframes.insert(KeyframeId {
                            scene: None,
                            actor: track.label.clone(),
                            property: property.to_string(),
                            time_ms,
                        });
                    }
                }
            }
        } else if let Some(composition) = &document.composition {
            for scene_name in &composition.declaration_order {
                let Some(scene) = composition.scenes.get(scene_name) else {
                    continue;
                };
                let scene_actors: BTreeSet<String> =
                    scene.timeline.tracks().keys().cloned().collect();
                actors.extend(scene_actors.iter().cloned());
                actors_by_scene.insert(Some(scene_name.clone()), scene_actors);
                for track in scene.timeline.tracks().values() {
                    for (property, times) in collect_per_property_keyframes(track) {
                        for time_ms in times {
                            keyframes.insert(KeyframeId {
                                scene: Some(scene_name.clone()),
                                actor: track.label.clone(),
                                property: property.to_string(),
                                time_ms,
                            });
                        }
                    }
                }
            }
        }

        let scenes = document
            .composition
            .as_ref()
            .map(|composition| composition.declaration_order.iter().cloned().collect())
            .unwrap_or_default();

        Self {
            actors,
            actors_by_scene,
            scenes,
            duration_ms: (document.duration_s * 1000.0).round() as i64,
            keyframes,
        }
    }

    /// Actor labels from `scene` that survived into the current build.
    pub fn surviving_actors(
        &self,
        scene: Option<&str>,
        labels: impl IntoIterator<Item = String>,
    ) -> HashSet<String> {
        let Some(scene_actors) = self.actors_by_scene.get(&scene.map(ToOwned::to_owned)) else {
            return HashSet::new();
        };
        labels.into_iter().filter(|label| scene_actors.contains(label)).collect()
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
/// end of the new duration if no keyframes remain. Composition keyframes are
/// compared in global time.
pub fn preserved_time_s(previous_time_s: f64, document: &DocumentSession) -> f64 {
    let duration_s = document.duration_s.max(0.1);
    if previous_time_s <= duration_s {
        return previous_time_s;
    }

    let times = if document.is_composition() {
        crate::document::timeline_keyframe_times_s(
            None,
            document.composition.as_ref(),
            document.active_scene.as_deref(),
        )
    } else {
        document
            .active_timeline()
            .map(|timeline| timeline.keyframe_times_s())
            .unwrap_or_default()
    };
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

    fn id(scene: Option<&str>, actor: &str, property: &str, time_ms: u64) -> KeyframeId {
        KeyframeId {
            scene: scene.map(ToOwned::to_owned),
            actor: actor.to_string(),
            property: property.to_string(),
            time_ms,
        }
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
            id(None, "box", "size", 0),
            id(None, "box", "color", 2000),
            id(None, "box", "position", 2000),
            id(None, "gone", "color", 2000),
        ]);

        assert_eq!(kept, vec![id(None, "box", "size", 0), id(None, "box", "color", 2000)]);
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
        assert!(diff.removed_keyframes.iter().any(|keyframe| {
            keyframe.scene.is_none()
                && keyframe.actor == "box"
                && keyframe.property == "color"
                && keyframe.time_ms == 2000
        }));
        assert!(diff.added_keyframes.iter().any(|keyframe| {
            keyframe.scene.is_none()
                && keyframe.actor == "box"
                && keyframe.property == "position"
                && keyframe.time_ms == 2000
        }));
    }

    #[test]
    fn fingerprint_does_not_emit_registry_alias_keyframes() {
        let current = TimelineFingerprint::from_document(&load_session(
            "#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));

        assert!(
            current
                .surviving_keyframes(vec![id(None, "box", "background_color", 2000)])
                .is_empty(),
            "background_color aliases ActorField::Color and must not create a separate identity"
        );
        assert_eq!(
            current.surviving_keyframes(vec![id(None, "box", "color", 2000)]),
            vec![id(None, "box", "color", 2000)]
        );
    }

    #[test]
    fn composition_fingerprint_qualifies_keyframes_by_scene() {
        let fingerprint = TimelineFingerprint::from_document(&load_session(
            "# A\n#0s\nbox: Rect, size: (100, 100)\n# B\n#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));

        let kept = fingerprint.surviving_keyframes(vec![
            id(Some("A"), "box", "size", 0),
            id(Some("B"), "box", "size", 0),
            id(Some("B"), "box", "color", 2000),
            id(None, "box", "color", 2000),
        ]);

        assert_eq!(
            kept,
            vec![
                id(Some("A"), "box", "size", 0),
                id(Some("B"), "box", "size", 0),
                id(Some("B"), "box", "color", 2000),
            ]
        );
    }

    #[test]
    fn composition_diff_drops_only_removed_scene_keyframes() {
        let previous = TimelineFingerprint::from_document(&load_session(
            "# A\n#0s\nbox: Rect, size: (100, 100)\n# B\n#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        ));
        let current = TimelineFingerprint::from_document(&load_session(
            "# A\n#0s\nbox: Rect, size: (100, 100)\n",
        ));

        let diff = TimelineDiff::between(&previous, &current);
        assert_eq!(diff.removed_scenes, vec!["B"]);
        assert!(diff.removed_keyframes.iter().all(|id| id.scene.as_deref() == Some("B")));
        assert!(
            diff.removed_keyframes
                .iter()
                .any(|id| { id.actor == "box" && id.property == "color" && id.time_ms == 2000 })
        );
        assert!(diff.removed_actors.iter().all(|actor| actor != "box"));
    }

    #[test]
    fn property_lanes_are_unique_and_cover_unkeyframed_properties() {
        let document = load_session("#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n");
        let timeline = document.active_timeline().expect("single-scene timeline");
        let track = timeline.get_track("box").expect("box track");

        let lanes = collect_property_lanes(track);

        // Lane names are unique (no duplicate rows from registry aliases).
        let mut names: Vec<String> = lanes.iter().map(|(name, _)| name.clone()).collect();
        let lane_count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), lane_count, "lane names must be unique");

        // Every keyframed property has a lane whose times match the typed
        // collector exactly.
        for (property, times) in collect_per_property_keyframes(track) {
            let lane = lanes
                .iter()
                .find(|(name, _)| *name == property)
                .unwrap_or_else(|| panic!("missing lane for keyframed property '{property}'"));
            assert_eq!(lane.1, times, "lane '{property}' times must match typed collector");
        }

        // An unkeyframed but animatable property still gets an empty lane.
        let rotation = lanes
            .iter()
            .find(|(name, _)| *name == "rotation")
            .expect("rotation lane must exist for a Rect");
        assert!(rotation.1.is_empty(), "unkeyframed rotation lane must be empty");
    }

    #[test]
    fn property_lanes_dedupe_registry_alias_fields() {
        let document = load_session("#0s\nbox: Rect\n");
        let timeline = document.active_timeline().expect("single-scene timeline");
        let track = timeline.get_track("box").expect("box track");

        // `background_color` aliases `ActorField::Color`, so the alias must not
        // surface as its own lane.
        assert!(
            !collect_property_lanes(track)
                .iter()
                .any(|(name, _)| *name == "background_color"),
            "registry aliases must collapse onto the storage field's lane"
        );
    }

    #[test]
    fn lane_schema_maps_typed_lanes_to_writable_source_names() {
        let rect_caps = animatix::timeline::caps_for_type("Rect").expect("Rect is a built-in");

        // Internal lane names that differ from the source property name must
        // resolve to the writable schema, not to an ambiguous sibling.
        assert_eq!(
            lane_schema(&rect_caps, "Rect", "motion_offset").map(|schema| schema.name),
            Some("shift")
        );
        // `size` shares `ActorField::Size` with `height`/`width`; exact-name
        // precedence must win.
        assert_eq!(lane_schema(&rect_caps, "Rect", "size").map(|schema| schema.name), Some("size"));
        // Unknown lane names resolve to nothing.
        assert!(lane_schema(&rect_caps, "Rect", "nope").is_none());
    }

    #[test]
    fn preserved_time_uses_global_composition_keyframes() {
        let document =
            load_session("# A\n#0s\nbox: Rect\n# B\n#0s\nbox: Rect\n#2s\nbox.color = red\n");

        // Global keyframe positions are A=0s and B=start+2s where B's start is
        // scene A's duration. Scene A has only a `#0s` declaration, so its
        // inferred duration is floored to one frame (1/60 s —
        // `composition::build` keeps zero-duration scenes alive long enough for
        // incoming transitions), and the engine also pre-seeds the instant `2s`
        // color assignment 1 ms early. The real global keyframe therefore sits
        // at ≈2.0167 s. A playhead beyond the composition duration must land on
        // that real global keyframe, not on active scene-local time.
        let preserved = preserved_time_s(5.0, &document);
        assert!(preserved > 2.0, "expected a global keyframe after 2s, got {preserved}");
        assert!(
            preserved <= 2.0 + 1.0 / 60.0 + 0.01,
            "expected the ~2.0167s global keyframe, got {preserved}"
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
