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
    let mut track = AnimationTrack::new("test".to_string());

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
    let mut track = AnimationTrack::new("test".to_string());
    track.kind = ActorKindId::Equation;

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
    let mut track_a = AnimationTrack::new("a".to_string());
    track_a.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

    let mut track_b = AnimationTrack::new("b".to_string());
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
    let mut track = AnimationTrack::new("test".to_string());
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
fn test_keyframe_times_s_includes_filter_fields() {
    let mut timeline = keyframe_times_s_timeline();
    // Filter fields apply to Filter actor kind
    let mut track = AnimationTrack::new("test".to_string());
    track.kind = ActorKindId::Filter;
    track
        .filter
        .filter_brightness
        .ensure(1.0)
        .add_keyframe(500, 2.0, Easing::Linear);
    track.filter.filter_contrast.ensure(1.0).add_keyframe(1200, 1.5, Easing::Linear);
    track.filter.filter_saturate.ensure(1.0).add_keyframe(800, 0.0, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&0.5));
    assert!(times.contains(&0.8));
    assert!(times.contains(&1.2));
}

#[test]
fn test_keyframe_times_s_includes_plot_param_tracks() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::new("test".to_string());
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
