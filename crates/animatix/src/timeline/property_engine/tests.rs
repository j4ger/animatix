use super::*;
use crate::easing::Easing;
use crate::timeline::PlacementMode;
use crate::timeline::animation_track::CalloutPlace;
use crate::timeline::dispatch::{
    property_has_keyframe_at, property_has_keyframes, property_keyframe_count,
    property_keyframe_easing, property_keyframe_times, read_property_value,
    read_property_value_or_default,
};
use crate::timeline::property_registry::ActorField;
use crate::timeline::{AnimationTrack, Interpolate, ShapeType, TrackAccessor};

#[test]
fn enum_boundary_shape_type_roundtrips() {
    let value = ShapeType::Arrow;
    assert_eq!(value.name(), "Arrow");
    assert_eq!(ShapeType::from_name(value.name()), Some(value));
    assert_eq!(value.to_property_value(), PropertyValue::Enum("Arrow".to_string()));
}

#[test]
fn enum_boundary_placement_mode_roundtrips() {
    let value = PlacementMode::Manual;
    assert_eq!(value.name(), "manual");
    assert_eq!(PlacementMode::from_name(value.name()), Some(value));
    assert_eq!(value.to_property_value(), PropertyValue::Enum("manual".to_string()));
}

#[test]
fn enum_boundary_callout_place_roundtrips() {
    let value = CalloutPlace::Left;
    assert_eq!(value.name(), "left");
    assert_eq!(CalloutPlace::from_name(value.name()), Some(value));
    assert_eq!(value.to_property_value(), PropertyValue::Enum("left".to_string()));
}

#[test]
fn tagged_property_routes_through_actor_plan() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    timeline
        .world_mut()
        .tracks
        .get_by_id_mut(id)
        .expect("track")
        .rebuild_property_plan();
    let field = ActorField::Tagged("legend_title");
    timeline.world_mut().write_property_field(
        id,
        field,
        PropertyValue::String("Revenue".to_string()),
        0,
        0,
        Easing::Linear,
        &mut vec![],
    );
    assert_eq!(
        timeline.world().read_property_value(id, field, 0),
        Some(PropertyValue::String("Revenue".to_string()))
    );

    timeline.world_mut().write_property_field(
        id,
        field,
        PropertyValue::String("Costs".to_string()),
        0,
        1000,
        Easing::EaseInOut,
        &mut vec![],
    );
    assert!(timeline.world().property_has_keyframes(id, field));
    assert!(timeline.world().property_has_keyframe_at(id, field, 1000));
    assert_eq!(timeline.world().property_keyframe_times(id, field), vec![0, 1000]);
    assert_eq!(
        timeline.world().property_keyframe_easing(id, field, 1000),
        Some(Easing::EaseInOut)
    );
}
#[test]
fn property_plan_slot_writes_and_reads_without_string_lookup() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.rebuild_property_plan();
    let position = crate::timeline::property_id("position").expect("position is registered");

    assert!(write_property_plan_slot(
        &mut track,
        position,
        crate::timeline::PropertyKind::Vec2,
        PropertyValue::Vec2([10.0, 20.0]),
        0,
        0,
        Easing::Linear,
    ));
    assert_eq!(
        read_property_plan_slot(&track, position, 0),
        Some(PropertyValue::Vec2([10.0, 20.0]))
    );

    assert!(write_property_plan_slot(
        &mut track,
        position,
        crate::timeline::PropertyKind::Vec2,
        PropertyValue::Vec2([110.0, 20.0]),
        0,
        1000,
        Easing::Linear,
    ));
    let slot = track.property_plan.get(position).expect("position slot");
    assert_eq!(slot.track.sample(500), Some(PropertyValue::Vec2([60.0, 20.0])));
}

// Helper: write a keyframe and read it back
fn write_read_roundtrip(
    track: &mut AnimationTrack,
    field: ActorField,
    value: PropertyValue,
    time_ms: u64,
) -> Option<PropertyValue> {
    write_property_field(track, field, value, time_ms, time_ms, Easing::Linear, &mut vec![]);
    read_property_value(track, field, time_ms)
}

// ────────────────────────────────────────────────
// 4.2: write/read round-trip tests
// ────────────────────────────────────────────────

#[test]
fn test_write_read_roundtrip_f32() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result =
        write_read_roundtrip(&mut track, ActorField::Opacity, PropertyValue::F32(0.75), 500);
    assert_eq!(result, Some(PropertyValue::F32(0.75)));
}

#[test]
fn test_write_read_roundtrip_vec2() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result = write_read_roundtrip(
        &mut track,
        ActorField::Position,
        PropertyValue::Vec2([100.0, 200.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Vec2([100.0, 200.0])));
}

#[test]
fn test_write_read_roundtrip_color() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result = write_read_roundtrip(
        &mut track,
        ActorField::Color,
        PropertyValue::Color([1.0, 0.0, 0.0, 1.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Color([1.0, 0.0, 0.0, 1.0])));
}

#[test]
fn test_write_read_roundtrip_vec4() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result = write_read_roundtrip(
        &mut track,
        ActorField::StrokeColor,
        PropertyValue::Vec4([0.0, 1.0, 0.0, 0.5]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Color([0.0, 1.0, 0.0, 0.5])));
}

#[test]
fn test_write_read_roundtrip_transform() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result = write_read_roundtrip(
        &mut track,
        ActorField::Transform,
        PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0])));
}

#[test]
fn test_write_read_roundtrip_string() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result = write_read_roundtrip(
        &mut track,
        ActorField::FontFamily,
        PropertyValue::String("Arial".to_string()),
        500,
    );
    assert_eq!(result, Some(PropertyValue::String("Arial".to_string())));
}

#[test]
fn test_write_read_roundtrip_tagged_bool() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    timeline.world_mut().write_property_field(
        id,
        ActorField::Tagged("legend"),
        PropertyValue::Bool(false),
        500,
        500,
        Easing::Linear,
        &mut vec![],
    );
    assert_eq!(
        timeline.world().read_property_value(id, ActorField::Tagged("legend"), 500),
        Some(PropertyValue::Bool(false))
    );
}

#[test]
fn test_property_value_interpolation_rules() {
    let a = PropertyValue::F32(0.0);
    let b = PropertyValue::F32(100.0);
    assert_eq!(a.interpolate(&b, 0.5), PropertyValue::F32(50.0));

    let hidden = PropertyValue::Bool(false);
    let shown = PropertyValue::Bool(true);
    assert_eq!(hidden.interpolate(&shown, 0.25), PropertyValue::Bool(false));
    assert_eq!(hidden.interpolate(&shown, 0.75), PropertyValue::Bool(true));

    let left = PropertyValue::String("left".to_string());
    let right = PropertyValue::String("right".to_string());
    assert_eq!(left.interpolate(&right, 0.25), left);
    assert_eq!(left.interpolate(&right, 0.75), right);

    // Cross-variant transitions snap at the midpoint.
    assert_eq!(hidden.interpolate(&right, 0.25), hidden);
    assert_eq!(hidden.interpolate(&right, 0.75), right);
}

#[test]
fn test_tagged_auto_transition_snaps_cross_variant() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let mut diag = Vec::new();
    timeline.world_mut().write_property_field(
        id,
        ActorField::Tagged("legend"),
        PropertyValue::Bool(false),
        0,
        1000,
        Easing::Linear,
        &mut diag,
    );
    timeline.world_mut().write_property_field(
        id,
        ActorField::Tagged("legend"),
        PropertyValue::String("Revenue".to_string()),
        1000,
        2000,
        Easing::Linear,
        &mut diag,
    );

    assert_eq!(
        timeline.world().read_property_value(id, ActorField::Tagged("legend"), 1250),
        Some(PropertyValue::Bool(false))
    );
    assert_eq!(
        timeline.world().read_property_value(id, ActorField::Tagged("legend"), 1750),
        Some(PropertyValue::String("Revenue".to_string()))
    );
}

#[test]
fn test_write_read_roundtrip_min_width() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result =
        write_read_roundtrip(&mut track, ActorField::MinWidth, PropertyValue::F32(200.0), 500);
    assert_eq!(result, Some(PropertyValue::F32(200.0)));
}

#[test]
fn test_write_read_roundtrip_max_height() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result =
        write_read_roundtrip(&mut track, ActorField::MaxHeight, PropertyValue::F32(800.0), 500);
    assert_eq!(result, Some(PropertyValue::F32(800.0)));
}

#[test]
fn test_write_read_roundtrip_highlight_color() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Equation");
    timeline.world_mut().write_property_field(
        id,
        ActorField::HighlightColor,
        PropertyValue::Vec4([1.0, 0.0, 0.0, 1.0]),
        500,
        500,
        Easing::Linear,
        &mut vec![],
    );
    assert_eq!(
        timeline.world().read_property_value(id, ActorField::HighlightColor, 500),
        Some(PropertyValue::Color([1.0, 0.0, 0.0, 1.0]))
    );
}

#[test]
fn test_write_read_roundtrip_highlight_opacity() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Equation");
    timeline.world_mut().write_property_field(
        id,
        ActorField::HighlightOpacity,
        PropertyValue::F32(0.5),
        500,
        500,
        Easing::Linear,
        &mut vec![],
    );
    assert_eq!(
        timeline.world().read_property_value(id, ActorField::HighlightOpacity, 500),
        Some(PropertyValue::F32(0.5))
    );
}

#[test]
fn test_write_read_roundtrip_min_height() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result =
        write_read_roundtrip(&mut track, ActorField::MinHeight, PropertyValue::F32(300.0), 500);
    assert_eq!(result, Some(PropertyValue::F32(300.0)));
}

#[test]
fn test_write_read_roundtrip_letter_spacing() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let result =
        write_read_roundtrip(&mut track, ActorField::LetterSpacing, PropertyValue::F32(2.5), 500);
    assert_eq!(result, Some(PropertyValue::F32(2.5)));
}

#[test]
fn test_write_read_roundtrip_with_duration_uses_linear_easing_at_start() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let mut diag = vec![];
    write_property_field(
        &mut track,
        ActorField::Opacity,
        PropertyValue::F32(0.5),
        0,    // t_start
        1000, // t_end
        Easing::EaseOut,
        &mut diag,
    );
    // Should have keyframes at 0 (start snapshot) and 1000 (end value)
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    assert_eq!(rf.keyframe_count(), 2);
    assert_eq!(rf.keyframe_easing(0), Some(Easing::Linear));
    assert_eq!(rf.keyframe_easing(1000), Some(Easing::EaseOut));
}

#[test]
fn test_read_property_value_or_default_falls_back() {
    let track = AnimationTrack::placeholder("test".to_string());
    let schema = crate::timeline::property_registry::lookup_property("opacity").unwrap();
    let val = read_property_value_or_default(&track, schema, 0);
    assert_eq!(val, PropertyValue::F32(1.0));
}

#[test]
fn test_property_has_keyframes() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    assert!(property_has_keyframes(&track, ActorField::Opacity));
    assert!(!property_has_keyframes(&track, ActorField::Rotation));
}

#[test]
fn test_property_has_keyframe_at() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    assert!(property_has_keyframe_at(&track, ActorField::Opacity, 500));
    assert!(!property_has_keyframe_at(&track, ActorField::Opacity, 0));
}

#[test]
fn test_property_keyframe_times_sorted() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    let times = property_keyframe_times(&track, ActorField::Opacity);
    assert_eq!(times, vec![0, 1000]);
}

#[test]
fn test_property_keyframe_count() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    assert_eq!(property_keyframe_count(&track, ActorField::Opacity), 2);
    assert_eq!(property_keyframe_count(&track, ActorField::Rotation), 0);
}

#[test]
fn test_property_keyframe_easing() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::EaseInOut);
    let easing = property_keyframe_easing(&track, ActorField::Opacity, 0);
    assert_eq!(easing, Some(Easing::EaseInOut));
}
