use super::*;

// ────────────────────────────────────────────────────────
// 4.7: keyframe_times_s tests
// ────────────────────────────────────────────────────────

fn keyframe_times_s_timeline() -> Timeline {
    let mut timeline = Timeline::new();
    // Remove the default background_color keyframe at 0 so it doesn't pollute results
    timeline.background_color.keyframes_mut().clear();
    timeline
}

#[test]
fn test_keyframe_times_s_collects_all_fields() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::placeholder("test".to_string());

    // Add keyframes to various fields
    track.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
    track
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(2000, [100.0, 0.0], Easing::Linear);
    track.geometry.transform.ensure([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).add_keyframe(
        3000,
        [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        Easing::Linear,
    );

    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    // Times in seconds: 1.0 (opacity), 2.0 (position), 3.0 (transform)
    assert_eq!(times.len(), 3, "Got: {:?}", times);
    assert!(times.contains(&1.0));
    assert!(times.contains(&2.0));
    assert!(times.contains(&3.0));
}

#[test]
fn test_keyframe_times_s_includes_highlight_fields() {
    let mut timeline = keyframe_times_s_timeline();
    // Highlight fields apply to Equation/Fragment actors
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.set_identity("Equation");

    track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
        500,
        [1.0, 0.0, 0.0, 1.0],
        Easing::Linear,
    );
    track
        .highlight
        .highlight_opacity
        .ensure(0.0)
        .add_keyframe(2500, 0.8, Easing::Linear);

    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&0.5), "Got: {:?}", times);
    assert!(times.contains(&2.5), "Got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_returns_unique_times() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track_a = AnimationTrack::placeholder("a".to_string());
    track_a.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

    let mut track_b = AnimationTrack::placeholder("b".to_string());
    track_b.style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);

    timeline.tracks.insert("a".to_string(), track_a);
    timeline.tracks.insert("b".to_string(), track_b);
    let times = timeline.keyframe_times_s();
    // Both tracks have the same keyframe time (1000ms = 1.0s)
    assert_eq!(times.len(), 1, "Should have unique times, got: {:?}", times);
    assert!((times[0] - 1.0).abs() < 0.001);
}

#[test]
fn test_keyframe_times_s_returns_seconds_not_milliseconds() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(5000, 0.5, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(!times.contains(&5000.0), "Should be in seconds, not milliseconds");
    assert!(times.contains(&5.0), "5000ms should be 5.0s, got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_includes_background_color() {
    let mut timeline = keyframe_times_s_timeline();
    timeline
        .background_color
        .add_keyframe(3000, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&3.0), "Got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_includes_effect_params() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::placeholder("test".to_string());
    let mut stage = crate::timeline::effects::EffectStage::new(
        "soft".to_string(),
        crate::timeline::effects::EffectId::new("Blur"),
    );
    stage
        .param_track_mut("radius", crate::timeline::effects::EffectParamKind::F32)
        .add_keyframe(500, crate::timeline::property_engine::PropertyValue::F32(8.0));
    track.effects.stages.push(stage);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&0.5), "Got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_includes_plot_param_tracks() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::placeholder("test".to_string());
    track
        .plot_param_tracks
        .entry("freq".to_string())
        .or_insert_with(|| PropertyTrack::new(1.0))
        .add_keyframe(2000, 2.0, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&2.0));
}

#[test]
fn test_keyframe_times_s_empty_when_no_keyframes() {
    let timeline = keyframe_times_s_timeline();
    let times = timeline.keyframe_times_s();
    assert!(times.is_empty());
}

/// The engine once kept its own copy of the easing name table and fell behind:
/// `custom` parsed in the editor and then silently resolved to nothing at
/// build time. Both layers must answer for the same set of names.
#[test]
fn engine_easing_names_cover_the_syntax_registry() {
    for (id, label) in crate::easing::EASING_REGISTRY {
        assert!(
            crate::timeline::parse_easing_name(id).is_some(),
            "{label:?} is offered by the registry but rejected by the engine"
        );
    }
}
