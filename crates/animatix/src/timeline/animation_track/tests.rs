use super::*;
use crate::easing::Easing;
use crate::primitives::primitive_catalog;
use crate::timeline::actor_caps::ShapeKind;
use crate::timeline::property_registry::ActorField;
use crate::timeline::property_track::{Interpolate, PropertyTrack, TrackAccessor};
use crate::timeline::shapes::ShapeType;

/// Every built-in primitive must appear in the metadata registry and
/// derive a complete capability projection.
#[test]
fn primitive_catalog_is_complete() {
    use crate::primitives::find_primitive;

    for meta in primitive_catalog() {
        assert!(
            find_primitive(meta.type_name.as_ref()).is_some(),
            "registry metadata for {:?} has no primitive",
            meta.type_name
        );
        let caps = animatix_std::caps_for_type(meta.type_name.as_ref())
            .unwrap_or_else(|| panic!("caps missing for {:?}", meta.type_name));
        let _ = caps; // derivation must not panic; contents are per-primitive
    }

    // Spot-check the derived projections for representative kinds.
    let rect = animatix_std::caps_for_type("Rect").unwrap();
    assert_eq!(rect.shape, Some(ShapeKind::Rect));
    assert!(rect.is_shape);
    assert!(rect.stroke_path);

    let text = animatix_std::caps_for_type("Text").unwrap();
    assert_eq!(text.text, Some(crate::timeline::TextKind::Text));
    assert!(text.text_paths);

    let filter = animatix_std::caps_for_type("Filter").unwrap();
    assert!(filter.is_effect_scope());

    let gauge_like_group = animatix_std::caps_for_type("Group").unwrap();
    assert!(gauge_like_group.is_group_like());
    assert!(!gauge_like_group.layout_container);

    let row = animatix_std::caps_for_type("Row").unwrap();
    assert!(row.layout_container);
}

/// A track built from a registry type name must derive caps from that
/// primitive, and the name must survive unchanged.
#[test]
fn track_identity_derives_caps_from_registry() {
    for meta in primitive_catalog().iter() {
        let track = super::AnimationTrack::new("t".to_string(), meta.type_name.clone());
        assert_eq!(track.actor_type, meta.type_name.as_ref());
        if let Some(expected) = animatix_std::caps_for_type(meta.type_name.as_ref()) {
            assert_eq!(track.caps, expected, "caps drifted for {}", meta.type_name);
        }
    }
}

#[test]
fn test_interpolation() {
    let p1: [f32; 2] = [0.0, 0.0];
    let p2: [f32; 2] = [100.0, 50.0];

    let interpolated = p1.interpolate(&p2, 0.5);
    assert_eq!(interpolated, [50.0, 25.0]);
}

#[test]
fn test_property_track_evaluation() {
    let mut track = PropertyTrack::new([0.0, 0.0]);

    track.add_keyframe(0, [0.0, 0.0], Easing::Linear);
    track.add_keyframe(1000, [100.0, 0.0], Easing::Linear);
    track.add_keyframe(2000, [100.0, 100.0], Easing::Linear);

    // Exactly at first keyframe
    assert_eq!(track.evaluate(0), [0.0, 0.0]);

    // Midway between 1st and 2nd
    assert_eq!(track.evaluate(500), [50.0, 0.0]);

    // Exactly at 2nd keyframe
    assert_eq!(track.evaluate(1000), [100.0, 0.0]);

    // Midway between 2nd and 3rd
    assert_eq!(track.evaluate(1500), [100.0, 50.0]);

    // Beyond last keyframe
    assert_eq!(track.evaluate(2500), [100.0, 100.0]);
}

// ────────────────────────────────────────────────────────
// 4.1: field_ref/field_mut coverage tests
// ────────────────────────────────────────────────────────

/// Helper: create a track with a keyframe added to a specific field.
fn create_track_with_f32_keyframe(field: ActorField, time_ms: u64, value: f32) -> AnimationTrack {
    let mut track = AnimationTrack::placeholder("test".to_string());
    if let Some(mut f) = track.field_mut(field) {
        match &mut f {
            TrackFieldMut::F32(opt) => {
                opt.ensure(0.0).add_keyframe(time_ms, value, Easing::Linear);
            },
            _ => panic!("Expected F32 track"),
        }
    }
    track
}

fn create_track_with_vec4_keyframe(
    field: ActorField,
    time_ms: u64,
    value: [f32; 4],
) -> AnimationTrack {
    let mut track = AnimationTrack::placeholder("test".to_string());
    if let Some(mut f) = track.field_mut(field) {
        match &mut f {
            TrackFieldMut::Vec4(opt) => {
                opt.ensure(value).add_keyframe(time_ms, value, Easing::Linear);
            },
            _ => panic!("Expected Vec4 track"),
        }
    }
    track
}

fn create_track_with_vec2_keyframe(
    field: ActorField,
    time_ms: u64,
    value: [f32; 2],
) -> AnimationTrack {
    let mut track = AnimationTrack::placeholder("test".to_string());
    if let Some(mut f) = track.field_mut(field) {
        match &mut f {
            TrackFieldMut::Vec2(opt) => {
                opt.ensure(value).add_keyframe(time_ms, value, Easing::Linear);
            },
            _ => panic!("Expected Vec2 track"),
        }
    }
    track
}

fn create_track_with_string_keyframe(
    field: ActorField,
    time_ms: u64,
    value: &str,
) -> AnimationTrack {
    let mut track = AnimationTrack::placeholder("test".to_string());
    if let Some(mut f) = track.field_mut(field) {
        match &mut f {
            TrackFieldMut::String(opt) => {
                opt.ensure(value.to_string()).add_keyframe(
                    time_ms,
                    value.to_string(),
                    Easing::Linear,
                );
            },
            _ => panic!("Expected String track"),
        }
    }
    track
}

#[test]
fn test_field_ref_ascent_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::Ascent, 0, 10.0);
    let rf = track.field_ref(ActorField::Ascent).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
    assert_eq!(
        rf.evaluate_value(0),
        Some(super::super::property_engine::PropertyValue::F32(10.0))
    );
}

#[test]
fn test_field_ref_descent_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::Descent, 0, 5.0);
    let rf = track.field_ref(ActorField::Descent).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_baseline_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::Baseline, 0, 2.0);
    let rf = track.field_ref(ActorField::Baseline).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_highlight_color_returns_vec4() {
    let track =
        create_track_with_vec4_keyframe(ActorField::HighlightColor, 0, [1.0, 0.0, 0.0, 1.0]);
    let rf = track.field_ref(ActorField::HighlightColor).unwrap();
    assert!(matches!(rf, TrackFieldRef::Vec4(_)));
}

#[test]
fn test_field_ref_highlight_opacity_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::HighlightOpacity, 0, 0.5);
    let rf = track.field_ref(ActorField::HighlightOpacity).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_highlight_padding_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::HighlightPadding, 0, 8.0);
    let rf = track.field_ref(ActorField::HighlightPadding).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_highlight_radius_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::HighlightRadius, 0, 6.0);
    let rf = track.field_ref(ActorField::HighlightRadius).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_font_weight_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::FontWeight, 0, 700.0);
    let rf = track.field_ref(ActorField::FontWeight).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_font_style_returns_string() {
    let track = create_track_with_string_keyframe(ActorField::FontStyle, 0, "italic");
    let rf = track.field_ref(ActorField::FontStyle).unwrap();
    assert!(matches!(rf, TrackFieldRef::String(_)));
}

#[test]
fn test_field_ref_line_height_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::LineHeight, 0, 1.5);
    let rf = track.field_ref(ActorField::LineHeight).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_letter_spacing_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::LetterSpacing, 0, 0.5);
    let rf = track.field_ref(ActorField::LetterSpacing).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_word_spacing_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::WordSpacing, 0, 2.0);
    let rf = track.field_ref(ActorField::WordSpacing).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_min_width_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::MinWidth, 0, 100.0);
    let rf = track.field_ref(ActorField::MinWidth).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_min_height_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::MinHeight, 0, 200.0);
    let rf = track.field_ref(ActorField::MinHeight).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_max_height_returns_f32() {
    let track = create_track_with_f32_keyframe(ActorField::MaxHeight, 0, 500.0);
    let rf = track.field_ref(ActorField::MaxHeight).unwrap();
    assert!(matches!(rf, TrackFieldRef::F32(_)));
}

#[test]
fn test_field_ref_vector_paths_returns_vector_paths() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::VectorPaths);
    assert!(rf.is_some());
    assert!(matches!(rf.unwrap(), TrackFieldRef::VectorPaths(_)));
}

#[test]
fn test_field_ref_text_paths_returns_text_paths() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::TextPaths);
    assert!(rf.is_some());
    assert!(matches!(rf.unwrap(), TrackFieldRef::TextPaths(_)));
}

#[test]
fn test_field_ref_position_binding_returns_position_binding() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::PositionBinding);
    assert!(rf.is_some());
    assert!(matches!(rf.unwrap(), TrackFieldRef::PositionBinding(_)));
}

#[test]
fn test_field_ref_svg_paths_returns_none() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::SvgPaths);
    assert!(rf.is_none());
}

#[test]
fn test_field_ref_returns_correct_type_for_position() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::Position).unwrap();
    assert!(matches!(rf, TrackFieldRef::Vec2(_)));
}

#[test]
fn test_field_ref_returns_correct_type_for_transform() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::Transform).unwrap();
    assert!(matches!(rf, TrackFieldRef::Transform(_)));
}

#[test]
fn test_field_mut_returns_correct_type_for_highlight_color() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let f = track.field_mut(ActorField::HighlightColor).unwrap();
    assert!(matches!(f, TrackFieldMut::Vec4(_)));
}

#[test]
fn test_field_mut_returns_correct_type_for_font_weight() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let f = track.field_mut(ActorField::FontWeight).unwrap();
    assert!(matches!(f, TrackFieldMut::F32(_)));
}

#[test]
fn test_field_mut_returns_correct_type_for_font_style() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    let f = track.field_mut(ActorField::FontStyle).unwrap();
    assert!(matches!(f, TrackFieldMut::String(_)));
}

// ────────────────────────────────────────────────────────
// 4.3: max_keyframe_time / has_any_keyframes regression tests
// ────────────────────────────────────────────────────────

#[test]
fn test_max_keyframe_time_with_transform() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.geometry.transform.ensure([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).add_keyframe(
        5000,
        [2.0, 0.0, 0.0, 2.0, 100.0, 200.0],
        Easing::Linear,
    );
    assert_eq!(track.max_keyframe_time(), Some(5000));
}

#[test]
fn test_max_keyframe_time_with_highlight_color() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
        3000,
        [1.0, 0.0, 0.0, 0.5],
        Easing::Linear,
    );
    assert_eq!(track.max_keyframe_time(), Some(3000));
}

#[test]
fn test_max_keyframe_time_returns_max_across_all_fields() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
    track
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(5000, [100.0, 100.0], Easing::Linear);
    track
        .highlight
        .highlight_opacity
        .ensure(0.0)
        .add_keyframe(3000, 0.8, Easing::Linear);
    assert_eq!(track.max_keyframe_time(), Some(5000));
}

#[test]
fn test_max_keyframe_time_returns_none_for_empty_track() {
    let track = AnimationTrack::placeholder("test".to_string());
    assert_eq!(track.max_keyframe_time(), None);
}

#[test]
fn test_has_any_keyframes_returns_true_for_highlight_fields() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
        1000,
        [1.0, 0.0, 0.0, 0.5],
        Easing::Linear,
    );
    assert!(track.has_any_keyframes());
}

#[test]
fn test_has_any_keyframes_returns_true_for_font_metrics() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.text.ascent.ensure(0.0).add_keyframe(500, 10.0, Easing::Linear);
    assert!(track.has_any_keyframes());
}

#[test]
fn test_has_any_keyframes_returns_false_for_empty_track() {
    let track = AnimationTrack::placeholder("test".to_string());
    assert!(!track.has_any_keyframes());
}

#[test]
fn test_has_any_keyframes_returns_false_for_single_keyframe_at_time_zero() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
    assert!(!track.has_any_keyframes());
}

#[test]
fn test_has_any_keyframes_returns_true_for_multiple_keyframes() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
    track.style.opacity.ensure(0.5).add_keyframe(1000, 1.0, Easing::Linear);
    assert!(track.has_any_keyframes());
}

// ────────────────────────────────────────────────────────
// 4.4: is_currently_animating tests
// ────────────────────────────────────────────────────────

#[test]
fn test_is_currently_animating_false_with_single_keyframe() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    // Single keyframe in the future is not animating at any time
    assert!(!track.is_currently_animating(0));
    assert!(!track.is_currently_animating(500));
    assert!(!track.is_currently_animating(1000));
    assert!(!track.is_currently_animating(2000));
}

#[test]
fn test_is_currently_animating_true_between_two_keyframes() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    assert!(track.is_currently_animating(500));
}

#[test]
fn test_is_currently_animating_false_before_first_keyframe() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(500, 0.5, Easing::Linear);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    assert!(!track.is_currently_animating(0));
    assert!(!track.is_currently_animating(400));
}

#[test]
fn test_is_currently_animating_false_after_last_keyframe() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(500, 0.5, Easing::Linear);
    assert!(!track.is_currently_animating(600));
    assert!(!track.is_currently_animating(1000));
}

#[test]
fn test_is_currently_animating_at_exact_keyframe_not_animating() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    // At exact keyframe time, the next keyframe is at > time_ms
    assert!(!track.is_currently_animating(1000));
}

#[test]
fn test_is_currently_animating_works_with_linear_easing() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(2000, 1.0, Easing::Linear);
    assert!(track.is_currently_animating(1000));
}

// ────────────────────────────────────────────────────────
// 4.5: interpolation_segment tests
// ────────────────────────────────────────────────────────

#[test]
fn test_interpolation_segment_none_for_empty_track() {
    let track: PropertyTrack<f32> = PropertyTrack::new(0.0);
    assert!(track.interpolation_segment(500).is_none());
}

#[test]
fn test_interpolation_segment_none_before_first_keyframe() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    track.add_keyframe(2000, 2.0, Easing::Linear);
    assert!(track.interpolation_segment(500).is_none());
}

#[test]
fn test_interpolation_segment_none_after_last_keyframe() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 1.0, Easing::Linear);
    assert!(track.interpolation_segment(1500).is_none());
}

#[test]
fn test_interpolation_segment_returns_correct_segment() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 100.0, Easing::Linear);
    let result = track.interpolation_segment(500);
    assert!(result.is_some());
    let (found_time, prev_val, found_val, progress, easing) = result.unwrap();
    assert_eq!(found_time, 1000);
    assert_eq!(*prev_val, 0.0);
    assert_eq!(*found_val, 100.0);
    assert!((progress - 0.5).abs() < 0.001);
    assert_eq!(*easing, Easing::Linear);
}

#[test]
fn test_interpolation_segment_progress_at_start() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(2000, 200.0, Easing::EaseIn);
    let result = track.interpolation_segment(500);
    assert!(result.is_some());
    let (_, _, _, progress, easing) = result.unwrap();
    assert!((progress - 0.25).abs() < 0.001);
    assert_eq!(*easing, Easing::EaseIn);
}

#[test]
fn test_interpolation_segment_with_single_keyframe_returns_none() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(500, 1.0, Easing::Linear);
    assert!(track.interpolation_segment(0).is_none());
    assert!(track.interpolation_segment(500).is_none());
    assert!(track.interpolation_segment(1000).is_none());
}

// ────────────────────────────────────────────────────────
// 4.8: Registry-driven iteration tests
// ────────────────────────────────────────────────────────

#[test]
fn test_max_keyframe_time_iterates_through_all_registry_fields() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    // Expect max_keyframe_time to iterate registry-driven fields
    // Put a keyframe on an unconventional registry field
    track.geometry.min_width.ensure(0.0).add_keyframe(7777, 50.0, Easing::Linear);
    assert_eq!(track.max_keyframe_time(), Some(7777));
}

#[test]
fn test_has_any_keyframes_iterates_through_all_registry_fields() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    // Put a keyframe on a field that might have been missed
    track
        .highlight
        .highlight_padding
        .ensure(4.0)
        .add_keyframe(500, 8.0, Easing::Linear);
    assert!(track.has_any_keyframes());
}

#[test]
fn test_registry_driven_max_keyframe_time_finds_min_width() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.geometry.min_height.ensure(0.0).add_keyframe(4000, 200.0, Easing::Linear);
    assert_eq!(track.max_keyframe_time(), Some(4000));
}

#[test]
fn test_registry_driven_max_keyframe_time_finds_letter_spacing() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.text.letter_spacing.ensure(0.0).add_keyframe(2500, 1.0, Easing::Linear);
    assert_eq!(track.max_keyframe_time(), Some(2500));
}

#[test]
fn test_registry_driven_has_any_keyframes_finds_word_spacing() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.text.word_spacing.ensure(0.0).add_keyframe(1000, 5.0, Easing::Linear);
    assert!(track.has_any_keyframes());
}

// ────────────────────────────────────────────────────────
// 4.9: TrackFieldRef helper methods
// ────────────────────────────────────────────────────────

#[test]
fn test_track_field_ref_evaluate_value_f32() {
    let track = create_track_with_f32_keyframe(ActorField::Opacity, 0, 0.5);
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    let val = rf.evaluate_value(0);
    assert_eq!(val, Some(super::super::property_engine::PropertyValue::F32(0.5)));
}

#[test]
fn test_track_field_ref_evaluate_value_vec2() {
    let track = create_track_with_vec2_keyframe(ActorField::Position, 0, [100.0, 200.0]);
    let rf = track.field_ref(ActorField::Position).unwrap();
    let val = rf.evaluate_value(0);
    assert_eq!(val, Some(super::super::property_engine::PropertyValue::Vec2([100.0, 200.0])));
}

#[test]
fn test_track_field_ref_evaluate_value_color() {
    let track = create_track_with_vec4_keyframe(ActorField::Color, 0, [1.0, 0.0, 0.0, 1.0]);
    let rf = track.field_ref(ActorField::Color).unwrap();
    let val = rf.evaluate_value(0);
    assert_eq!(
        val,
        Some(super::super::property_engine::PropertyValue::Color([1.0, 0.0, 0.0, 1.0]))
    );
}

#[test]
fn test_track_field_ref_has_keyframe_at() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    assert!(rf.has_keyframe_at(500));
    assert!(!rf.has_keyframe_at(0));
    assert!(!rf.has_keyframe_at(1000));
}

#[test]
fn test_track_field_ref_keyframe_count() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
    track.style.opacity.ensure(0.5).add_keyframe(1000, 0.0, Easing::Linear);
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    assert_eq!(rf.keyframe_count(), 3);
}

#[test]
fn test_track_field_ref_keyframe_times() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    let mut times = rf.keyframe_times();
    times.sort();
    assert_eq!(times, vec![0, 1000]);
}

#[test]
fn test_track_field_ref_keyframe_easing() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::EaseOut);
    track.style.opacity.ensure(0.0).add_keyframe(500, 0.0, Easing::Linear);
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    assert_eq!(rf.keyframe_easing(0), Some(Easing::EaseOut));
    assert_eq!(rf.keyframe_easing(500), Some(Easing::Linear));
    assert_eq!(rf.keyframe_easing(999), None);
}

#[test]
fn test_track_field_ref_keyframe_count_none_for_empty() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::Opacity).unwrap();
    assert_eq!(rf.keyframe_count(), 0);
}

#[test]
fn test_track_field_ref_evaluate_value_none_for_vector_paths() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::VectorPaths).unwrap();
    assert!(rf.evaluate_value(0).is_none());
}

#[test]
fn test_track_field_ref_evaluate_value_none_for_position_binding() {
    let track = AnimationTrack::placeholder("test".to_string());
    let rf = track.field_ref(ActorField::PositionBinding).unwrap();
    assert!(rf.evaluate_value(0).is_none());
}

// ────────────────────────────────────────────────────────
// 4.10: Cache invalidation tests
// ────────────────────────────────────────────────────────

#[test]
fn test_set_default_value_invalidates_cache() {
    let mut track = PropertyTrack::new(10.0);
    track.add_keyframe(0, 10.0, Easing::Linear);
    track.add_keyframe(1000, 20.0, Easing::Linear);
    // Evaluate to populate cache
    assert_eq!(track.evaluate(500), 15.0);
    // Change default value (should invalidate cache)
    track.set_default_value(0.0);
    // Evaluate at different time to verify cache was invalidated
    // (the new default won't affect interpolation between keyframes, but cache was reset)
    assert_eq!(track.evaluate(500), 15.0); // Interpolation still works

    // Clear keyframes via keyframes_mut (invalidates cache) and verify new default is used
    track.keyframes_mut().clear();
    assert_eq!(track.evaluate(500), 0.0);
}

#[test]
fn test_keyframes_mut_invalidates_cache() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 100.0, Easing::Linear);
    // Evaluate to populate cache
    assert_eq!(track.evaluate(500), 50.0);
    // Mutate keyframes (should invalidate)
    track.keyframes_mut().insert(2000, (200.0, Easing::Linear));
    // Evaluate at different time to verify
    assert_eq!(track.evaluate(1500), 150.0);
}

#[test]
fn test_clone_resets_cache_to_none() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 0.0, Easing::Linear);
    track.add_keyframe(1000, 100.0, Easing::Linear);
    // Evaluate to populate cache
    let _ = track.evaluate(500);
    // Clone should reset cache
    let cloned = track.clone();
    // Cloned track should evaluate correctly (cache starting fresh)
    assert_eq!(cloned.evaluate(500), 50.0);
}

// ────────────────────────────────────────────────────────
// 4.11: Accessor method tests
// ────────────────────────────────────────────────────────

#[test]
fn test_default_value_returns_correct_reference() {
    let track = PropertyTrack::new(42.0);
    assert_eq!(*track.default_value(), 42.0);
}

#[test]
fn test_keyframes_returns_correct_reference() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 10.0, Easing::Linear);
    track.add_keyframe(1000, 20.0, Easing::EaseOut);
    let kfs = track.keyframes();
    assert_eq!(kfs.len(), 2);
    assert_eq!(kfs.get(&0), Some(&(10.0, Easing::Linear)));
    assert_eq!(kfs.get(&1000), Some(&(20.0, Easing::EaseOut)));
}

#[test]
fn test_set_default_value_updates_value() {
    let mut track = PropertyTrack::new(10.0);
    assert_eq!(*track.default_value(), 10.0);
    track.set_default_value(99.0);
    assert_eq!(*track.default_value(), 99.0);
}

#[test]
fn test_keyframes_mut_allows_mutation() {
    let mut track = PropertyTrack::new(0.0);
    track.add_keyframe(0, 10.0, Easing::Linear);
    track.add_keyframe(1000, 20.0, Easing::Linear);
    {
        let kfs = track.keyframes_mut();
        // Insert a new keyframe
        kfs.insert(500, (15.0, Easing::EaseIn));
    }
    assert_eq!(track.keyframes().len(), 3);
    assert_eq!(track.keyframes().get(&500), Some(&(15.0, Easing::EaseIn)));
}

#[test]
fn test_missing_properties() {
    let track = AnimationTrack::placeholder("empty_actor".to_string());

    assert_eq!(track.geometry.position.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(
        track.geometry.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.geometry.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::Absolute
    );
    assert_eq!(track.geometry.size.get(0, [50.0, 50.0]), [50.0, 50.0]);
    assert_eq!(track.shape.line_from.get(0, [-50.0, 0.0]), [-50.0, 0.0]);
    assert_eq!(track.shape.line_to.get(0, [50.0, 0.0]), [50.0, 0.0]);
    assert_eq!(
        track.shape.arc_angles.get(0, [0.0, std::f32::consts::PI]),
        [0.0, std::f32::consts::PI]
    );
    assert_eq!(track.style.color.get(0, [1.0, 1.0, 1.0, 1.0]), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(track.shape.shape_type.get(0, ShapeType::Rect), ShapeType::Rect);
    assert_eq!(track.style.opacity.get(0, 1.0), 1.0);
}
