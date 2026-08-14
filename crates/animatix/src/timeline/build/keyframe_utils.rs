use super::*;

/// Capture current track values and insert start keyframes at `t_start_ms`.
/// Used when an actor declaration has a non-zero duration animation.
pub(crate) fn insert_start_keyframes(track: &mut AnimationTrack, t_start_ms: u64) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, std::f32::consts::PI];

    let start_vector_paths = track.evaluate_vector_paths(t_start_ms);
    let start_position = track.geometry.position.get(t_start_ms, [0.0, 0.0]);
    let start_size = track.geometry.size.get(t_start_ms, default_size);
    let start_line_from = track.shape.line_from.get(t_start_ms, [-50.0, 0.0]);
    let start_line_to = track.shape.line_to.get(t_start_ms, [50.0, 0.0]);
    let start_arc_angles = track.shape.arc_angles.get(t_start_ms, default_arc);
    let start_color = track.style.color.get(t_start_ms, DEFAULT_WHITE);
    let start_shape_type = track.shape.shape_type.get(t_start_ms, ShapeType::Rect);
    let start_opacity = track.style.opacity.get(t_start_ms, 1.0);
    let start_stroke_width =
        track.style.stroke_width.get(t_start_ms, default_stroke_width(track.kind));
    let start_stroke_color = track.style.stroke_color.get(t_start_ms, DEFAULT_WHITE);
    let start_stroke_progress = track.style.stroke_progress.get(t_start_ms, 1.0);
    let start_fill_opacity = track.style.fill_opacity.get(t_start_ms, 1.0);

    track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
        t_start_ms,
        start_vector_paths,
        Easing::Linear,
    );
    track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
        t_start_ms,
        start_position,
        Easing::Linear,
    );
    track
        .geometry
        .size
        .ensure(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    track
        .ensure_layout_size(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    track.shape.line_from.ensure([-50.0, 0.0]).add_keyframe(
        t_start_ms,
        start_line_from,
        Easing::Linear,
    );
    track
        .shape
        .line_to
        .ensure([50.0, 0.0])
        .add_keyframe(t_start_ms, start_line_to, Easing::Linear);
    track.shape.arc_angles.ensure(default_arc).add_keyframe(
        t_start_ms,
        start_arc_angles,
        Easing::Linear,
    );
    track
        .style
        .color
        .ensure(DEFAULT_WHITE)
        .add_keyframe(t_start_ms, start_color, Easing::Linear);
    track.shape.shape_type.ensure(ShapeType::Rect).add_keyframe(
        t_start_ms,
        start_shape_type,
        Easing::Linear,
    );
    track
        .style
        .opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
    track.style.stroke_width.ensure(default_stroke_width(track.kind)).add_keyframe(
        t_start_ms,
        start_stroke_width,
        Easing::Linear,
    );
    track.style.stroke_color.ensure(DEFAULT_WHITE).add_keyframe(
        t_start_ms,
        start_stroke_color,
        Easing::Linear,
    );
    track.style.stroke_progress.ensure(1.0).add_keyframe(
        t_start_ms,
        start_stroke_progress,
        Easing::Linear,
    );
    track.style.fill_opacity.ensure(1.0).add_keyframe(
        t_start_ms,
        start_fill_opacity,
        Easing::Linear,
    );
}

/// Preserve current track values at `t_start_ms` for delayed animations.
/// Used when an actor declaration has a delay but no duration.
pub(crate) fn preserve_delayed_values(track: &mut AnimationTrack, t_start_ms: u64) {
    preserve_instant_delayed_value(&mut track.shape.vector_paths, t_start_ms);
    preserve_instant_delayed_value(&mut track.geometry.position, t_start_ms);
    preserve_instant_delayed_value(&mut track.geometry.size, t_start_ms);
    preserve_instant_delayed_value(&mut track.geometry.layout_size, t_start_ms);
    preserve_instant_delayed_value(&mut track.shape.line_from, t_start_ms);
    preserve_instant_delayed_value(&mut track.shape.line_to, t_start_ms);
    preserve_instant_delayed_value(&mut track.shape.arc_angles, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.color, t_start_ms);
    preserve_instant_delayed_value(&mut track.shape.shape_type, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.opacity, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.stroke_width, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.stroke_color, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.stroke_progress, t_start_ms);
    preserve_instant_delayed_value(&mut track.style.fill_opacity, t_start_ms);
}

/// Insert end keyframes at `t_end_ms` with the given values and easing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_end_keyframes(
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
        .shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, vello_paths, easing);
    track
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(t_end_ms, position, easing);
    track.geometry.size.ensure(default_size).add_keyframe(t_end_ms, size, easing);
    track.ensure_layout_size(default_size).add_keyframe(t_end_ms, size, easing);
    track
        .shape
        .line_from
        .ensure([-50.0, 0.0])
        .add_keyframe(t_end_ms, line_from, easing);
    track.shape.line_to.ensure([50.0, 0.0]).add_keyframe(t_end_ms, line_to, easing);
    track
        .shape
        .arc_angles
        .ensure(default_arc)
        .add_keyframe(t_end_ms, arc_angles, easing);
    track.style.color.ensure(DEFAULT_WHITE).add_keyframe(t_end_ms, color, easing);
    track
        .shape
        .shape_type
        .ensure(ShapeType::Rect)
        .add_keyframe(t_end_ms, shape_type, easing);
    track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, opacity, easing);
    track.style.stroke_width.ensure(default_stroke_width(track.kind)).add_keyframe(
        t_end_ms,
        stroke_width,
        easing,
    );
    track
        .style
        .stroke_color
        .ensure(DEFAULT_WHITE)
        .add_keyframe(t_end_ms, stroke_color, easing);
    track
        .style
        .stroke_progress
        .ensure(1.0)
        .add_keyframe(t_end_ms, stroke_progress, easing);
    track
        .style
        .fill_opacity
        .ensure(1.0)
        .add_keyframe(t_end_ms, fill_opacity, easing);
}
