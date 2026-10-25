//! Payload access tests.
//!
//! The keyframe introspection free functions that used to live here were
//! track-taking compatibility wrappers; their id-keyed home is
//! `world::fields` and the property-name boundary is `world::keyframes`.

#[test]
fn rect_track_carries_no_payload() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world.spawn("box", "Rect");
    assert!(timeline.world.payload(id).is_none());
}

#[test]
fn ensure_plot_payload_switches_variant() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world.spawn("curve", "PlotCurve");
    assert!(timeline.world.payload(id).is_none());

    timeline
        .world
        .ensure_plot_payload(id)
        .plot_param_tracks
        .insert("freq".to_string(), super::PropertyTrack::new(2.0));
    assert!(matches!(
        timeline.world.payload(id),
        Some(crate::primitives::PrimitivePayload::Plot(_))
    ));
    assert_eq!(timeline.world.plot_param_track(id, "freq").map(|t| t.default_value), Some(2.0));

    // A second `ensure` must hand back the same payload, not reset it.
    assert_eq!(timeline.world.ensure_plot_payload(id).plot_param_tracks.len(), 1);
    assert_eq!(timeline.world.plot_param_track(id, "freq").map(|t| t.default_value), Some(2.0));
}

#[test]
fn svg_and_image_payloads_are_distinct() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world.spawn("icon", "Svg");
    timeline
        .world
        .ensure_svg_payload(id)
        .svg_paths
        .push(crate::timeline::VelloPath::default());
    assert!(matches!(
        timeline.world.payload(id),
        Some(crate::primitives::PrimitivePayload::Svg(_))
    ));
    assert!(timeline.world.image_payload(id).is_none());
    assert!(timeline.world.image_track(id).is_none());
    assert_eq!(timeline.world.svg_paths(id).len(), 1);
}
