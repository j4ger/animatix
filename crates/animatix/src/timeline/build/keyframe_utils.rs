use super::*;

/// Capture current track values and insert start keyframes at `t_start_ms`.
/// Used when an actor declaration has a non-zero duration animation.
pub(super) fn insert_start_keyframes(track: &mut AnimationTrack, t_start_ms: u64) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, std::f32::consts::PI];

    let start_vector_paths = track.evaluate_vector_paths(t_start_ms);
    let start_position = track.position.get(t_start_ms, [0.0, 0.0]);
    let start_size = track.size.get(t_start_ms, default_size);
    let start_line_from = track.line_from.get(t_start_ms, [-50.0, 0.0]);
    let start_line_to = track.line_to.get(t_start_ms, [50.0, 0.0]);
    let start_arc_angles = track.arc_angles.get(t_start_ms, default_arc);
    let start_color = track.color.get(t_start_ms, [1.0, 1.0, 1.0, 1.0]);
    let start_shape_type = track.shape_type.get(t_start_ms, ShapeType::Rect);
    let start_opacity = track.opacity.get(t_start_ms, 1.0);
    let start_stroke_width = track.stroke_width.get(t_start_ms, 2.0);
    let start_stroke_color = track.stroke_color.get(t_start_ms, [1.0, 1.0, 1.0, 1.0]);
    let start_stroke_progress = track.stroke_progress.get(t_start_ms, 1.0);
    let start_fill_opacity = track.fill_opacity.get(t_start_ms, 1.0);

    track
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_start_ms, start_vector_paths, Easing::Linear);
    track
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(t_start_ms, start_position, Easing::Linear);
    track
        .size
        .ensure(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    track
        .ensure_layout_size(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    track
        .line_from
        .ensure([-50.0, 0.0])
        .add_keyframe(t_start_ms, start_line_from, Easing::Linear);
    track
        .line_to
        .ensure([50.0, 0.0])
        .add_keyframe(t_start_ms, start_line_to, Easing::Linear);
    track
        .arc_angles
        .ensure(default_arc)
        .add_keyframe(t_start_ms, start_arc_angles, Easing::Linear);
    track
        .color
        .ensure([1.0, 1.0, 1.0, 1.0])
        .add_keyframe(t_start_ms, start_color, Easing::Linear);
    track
        .shape_type
        .ensure(ShapeType::Rect)
        .add_keyframe(t_start_ms, start_shape_type, Easing::Linear);
    track
        .opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
    track
        .stroke_width
        .ensure(2.0)
        .add_keyframe(t_start_ms, start_stroke_width, Easing::Linear);
    track
        .stroke_color
        .ensure([1.0, 1.0, 1.0, 1.0])
        .add_keyframe(t_start_ms, start_stroke_color, Easing::Linear);
    track
        .stroke_progress
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_stroke_progress, Easing::Linear);
    track
        .fill_opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_fill_opacity, Easing::Linear);
}

/// Preserve current track values at `t_start_ms` for delayed animations.
/// Used when an actor declaration has a delay but no duration.
pub(super) fn preserve_delayed_values(track: &mut AnimationTrack, t_start_ms: u64) {
    preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
    preserve_instant_delayed_value(&mut track.position, t_start_ms);
    preserve_instant_delayed_value(&mut track.size, t_start_ms);
    preserve_instant_delayed_value(&mut track.layout_size, t_start_ms);
    preserve_instant_delayed_value(&mut track.line_from, t_start_ms);
    preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
    preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
    preserve_instant_delayed_value(&mut track.color, t_start_ms);
    preserve_instant_delayed_value(&mut track.shape_type, t_start_ms);
    preserve_instant_delayed_value(&mut track.opacity, t_start_ms);
    preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
    preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
    preserve_instant_delayed_value(&mut track.stroke_progress, t_start_ms);
    preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
}

/// Insert end keyframes at `t_end_ms` with the given values and easing.
pub(super) fn insert_end_keyframes(
    track: &mut AnimationTrack,
    t_end_ms: u64,
    position: [f32; 2],
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
    color: [f32; 4],
    shape_type: ShapeType,
    opacity: f32,
    stroke_width: f32,
    stroke_color: [f32; 4],
    stroke_progress: f32,
    fill_opacity: f32,
    vello_paths: Vec<VelloPath>,
    easing: Easing,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, std::f32::consts::PI];

    track
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, vello_paths, easing);
    track
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(t_end_ms, position, easing);
    track
        .size
        .ensure(default_size)
        .add_keyframe(t_end_ms, size, easing);
    track
        .ensure_layout_size(default_size)
        .add_keyframe(t_end_ms, size, easing);
    track
        .line_from
        .ensure([-50.0, 0.0])
        .add_keyframe(t_end_ms, line_from, easing);
    track
        .line_to
        .ensure([50.0, 0.0])
        .add_keyframe(t_end_ms, line_to, easing);
    track
        .arc_angles
        .ensure(default_arc)
        .add_keyframe(t_end_ms, arc_angles, easing);
    track
        .color
        .ensure([1.0, 1.0, 1.0, 1.0])
        .add_keyframe(t_end_ms, color, easing);
    track
        .shape_type
        .ensure(ShapeType::Rect)
        .add_keyframe(t_end_ms, shape_type, easing);
    track
        .opacity
        .ensure(1.0)
        .add_keyframe(t_end_ms, opacity, easing);
    track
        .stroke_width
        .ensure(2.0)
        .add_keyframe(t_end_ms, stroke_width, easing);
    track
        .stroke_color
        .ensure([1.0, 1.0, 1.0, 1.0])
        .add_keyframe(t_end_ms, stroke_color, easing);
    track
        .stroke_progress
        .ensure(1.0)
        .add_keyframe(t_end_ms, stroke_progress, easing);
    track
        .fill_opacity
        .ensure(1.0)
        .add_keyframe(t_end_ms, fill_opacity, easing);
}
