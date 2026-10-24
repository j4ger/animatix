use super::*;
use crate::easing::Easing;
use crate::timeline::PlacementMode;
use crate::timeline::animation_track::CalloutPlace;
use crate::timeline::dispatch::read_property_value_or_default;
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
    timeline.world_mut().rebuild_property_plan(id);
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
    let mut timeline = crate::timeline::Timeline::new();
    let track = AnimationTrack::placeholder("test".to_string());
    let id = timeline.world_mut().spawn_with_track("test", track);
    timeline.world_mut().rebuild_property_plan(id);
    let position = crate::timeline::property_id("position").expect("position is registered");

    assert!(write_property_plan_slot(
        timeline.world_mut().property_plan_mut(id),
        position,
        crate::timeline::PropertyKind::Vec2,
        PropertyValue::Vec2([10.0, 20.0]),
        0,
        0,
        Easing::Linear,
    ));
    assert_eq!(
        read_property_plan_slot(timeline.world().property_plan(id), position, 0),
        Some(PropertyValue::Vec2([10.0, 20.0]))
    );

    assert!(write_property_plan_slot(
        timeline.world_mut().property_plan_mut(id),
        position,
        crate::timeline::PropertyKind::Vec2,
        PropertyValue::Vec2([110.0, 20.0]),
        0,
        1000,
        Easing::Linear,
    ));
    let slot = timeline.world().property_plan(id).get(position).expect("position slot");
    assert_eq!(slot.track.sample(500), Some(PropertyValue::Vec2([60.0, 20.0])));
}

/// Write and read one property through `World`, which owns the value tiers.
///
/// The track alone cannot reach component-backed fields now that the geometry,
/// style, shape, text and highlight tiers live in `World`, so round-trips go
/// through it.
fn world_write_read_roundtrip(
    timeline: &mut crate::timeline::Timeline,
    id: crate::timeline::EntityId,
    field: ActorField,
    value: PropertyValue,
    time_ms: u64,
) -> Option<PropertyValue> {
    timeline.world_mut().write_property_field(
        id,
        field,
        value,
        time_ms,
        time_ms,
        Easing::Linear,
        &mut vec![],
    );
    timeline.world().read_property_value(id, field, time_ms)
}

// ────────────────────────────────────────────────
// 4.2: write/read round-trip tests
// ────────────────────────────────────────────────

#[test]
fn test_write_read_roundtrip_f32() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::Opacity,
        PropertyValue::F32(0.75),
        500,
    );
    assert_eq!(result, Some(PropertyValue::F32(0.75)));
}

#[test]
fn test_write_read_roundtrip_vec2() {
    // position is geometry-tier storage owned by `World`.
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::Position,
        PropertyValue::Vec2([100.0, 200.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Vec2([100.0, 200.0])));
}

#[test]
fn test_write_read_roundtrip_color() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::Color,
        PropertyValue::Color([1.0, 0.0, 0.0, 1.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Color([1.0, 0.0, 0.0, 1.0])));
}

#[test]
fn test_write_read_roundtrip_vec4() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::StrokeColor,
        PropertyValue::Vec4([0.0, 1.0, 0.0, 0.5]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Color([0.0, 1.0, 0.0, 0.5])));
}

#[test]
fn test_write_read_roundtrip_transform() {
    // transform is geometry-tier storage owned by `World`.
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::Transform,
        PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0]),
        500,
    );
    assert_eq!(result, Some(PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0])));
}

#[test]
fn test_write_read_roundtrip_string() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Text");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
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
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::MinWidth,
        PropertyValue::F32(200.0),
        500,
    );
    assert_eq!(result, Some(PropertyValue::F32(200.0)));
}

#[test]
fn test_write_read_roundtrip_max_height() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::MaxHeight,
        PropertyValue::F32(800.0),
        500,
    );
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
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::MinHeight,
        PropertyValue::F32(300.0),
        500,
    );
    assert_eq!(result, Some(PropertyValue::F32(300.0)));
}

#[test]
fn test_write_read_roundtrip_letter_spacing() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Text");
    let result = world_write_read_roundtrip(
        &mut timeline,
        id,
        ActorField::LetterSpacing,
        PropertyValue::F32(2.5),
        500,
    );
    assert_eq!(result, Some(PropertyValue::F32(2.5)));
}

#[test]
fn test_write_read_roundtrip_with_duration_uses_linear_easing_at_start() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let mut diag = vec![];
    timeline.world_mut().write_property_field(
        id,
        ActorField::Opacity,
        PropertyValue::F32(0.5),
        0,    // t_start
        1000, // t_end
        Easing::EaseOut,
        &mut diag,
    );
    // Should have keyframes at 0 (start snapshot) and 1000 (end value)
    let world = timeline.world();
    assert_eq!(world.property_keyframe_count(id, ActorField::Opacity), 2);
    assert_eq!(world.property_keyframe_easing(id, ActorField::Opacity, 0), Some(Easing::Linear));
    assert_eq!(
        world.property_keyframe_easing(id, ActorField::Opacity, 1000),
        Some(Easing::EaseOut)
    );
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
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    timeline
        .world_mut()
        .style_mut(id)
        .opacity
        .ensure(1.0)
        .add_keyframe(500, 0.5, Easing::Linear);
    assert!(timeline.world().property_has_keyframes(id, ActorField::Opacity));
    assert!(!timeline.world().property_has_keyframes(id, ActorField::Rotation));
}

#[test]
fn test_property_has_keyframe_at() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    timeline
        .world_mut()
        .style_mut(id)
        .opacity
        .ensure(1.0)
        .add_keyframe(500, 0.5, Easing::Linear);
    assert!(timeline.world().property_has_keyframe_at(id, ActorField::Opacity, 500));
    assert!(!timeline.world().property_has_keyframe_at(id, ActorField::Opacity, 0));
}

#[test]
fn test_property_keyframe_times_sorted() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let style = timeline.world_mut().style_mut(id);
    style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
    style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    let times = timeline.world().property_keyframe_times(id, ActorField::Opacity);
    assert_eq!(times, vec![0, 1000]);
}

#[test]
fn test_property_keyframe_count() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    let style = timeline.world_mut().style_mut(id);
    style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    assert_eq!(timeline.world().property_keyframe_count(id, ActorField::Opacity), 2);
    assert_eq!(timeline.world().property_keyframe_count(id, ActorField::Rotation), 0);
}

#[test]
fn test_property_keyframe_easing() {
    let mut timeline = crate::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("test", "Rect");
    timeline
        .world_mut()
        .style_mut(id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::EaseInOut);
    let easing = timeline.world().property_keyframe_easing(id, ActorField::Opacity, 0);
    assert_eq!(easing, Some(Easing::EaseInOut));
}
