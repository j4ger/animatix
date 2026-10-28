//! Instant keyframe snapshots shared by the actor declaration paths.
//!
//! End values and declaration timing live with `insert_actor_keyframes` in
//! `actor/helpers.rs`, the only consumer of the end-keyframe writer; this
//! module keeps the start and delayed snapshots both paths reuse.

use super::*;
use crate::timeline::animation_track::{GeometryTracks, ShapeTracks, StyleTracks};

/// Capture current track values and insert start keyframes at `t_start_ms`.
/// Used when an actor declaration has a non-zero duration animation.
pub(crate) fn insert_start_keyframes(
    style: &mut StyleTracks,
    geometry: &mut GeometryTracks,
    shape: &mut ShapeTracks,
    caps: ActorCaps,
    t_start_ms: u64,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, std::f32::consts::PI];

    let start_vector_paths = shape.evaluate_vector_paths_value(style, t_start_ms);
    let start_position = geometry.position.get(t_start_ms, [0.0, 0.0]);
    let start_size = geometry.size.get(t_start_ms, default_size);
    let start_line_from = shape.line_from.get(t_start_ms, [-50.0, 0.0]);
    let start_line_to = shape.line_to.get(t_start_ms, [50.0, 0.0]);
    let start_arc_angles = shape.arc_angles.get(t_start_ms, default_arc);
    let start_corner_radius = shape.corner_radius.get(t_start_ms, 0.0);
    let start_color = style.color.get(t_start_ms, DEFAULT_WHITE);
    let start_shape_type = shape.shape_type.get(t_start_ms, ShapeType::Rect);
    let start_opacity = style.opacity.get(t_start_ms, 1.0);
    let start_stroke_width = style.stroke_width.get(t_start_ms, caps.default_stroke_width());
    let start_stroke_color = style.stroke_color.get(t_start_ms, DEFAULT_WHITE);
    let start_stroke_progress = style.stroke_progress.get(t_start_ms, 1.0);
    let start_fill_opacity = style.fill_opacity.get(t_start_ms, 1.0);

    shape.vector_paths.ensure(Vec::new()).add_keyframe(
        t_start_ms,
        start_vector_paths,
        Easing::Linear,
    );
    geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(t_start_ms, start_position, Easing::Linear);
    geometry
        .size
        .ensure(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    geometry
        .ensure_layout_size(default_size)
        .add_keyframe(t_start_ms, start_size, Easing::Linear);
    shape
        .line_from
        .ensure([-50.0, 0.0])
        .add_keyframe(t_start_ms, start_line_from, Easing::Linear);
    shape
        .line_to
        .ensure([50.0, 0.0])
        .add_keyframe(t_start_ms, start_line_to, Easing::Linear);
    shape
        .corner_radius
        .ensure(0.0)
        .add_keyframe(t_start_ms, start_corner_radius, Easing::Linear);
    shape
        .arc_angles
        .ensure(default_arc)
        .add_keyframe(t_start_ms, start_arc_angles, Easing::Linear);
    style
        .color
        .ensure(DEFAULT_WHITE)
        .add_keyframe(t_start_ms, start_color, Easing::Linear);
    shape.shape_type.ensure(ShapeType::Rect).add_keyframe(
        t_start_ms,
        start_shape_type,
        Easing::Linear,
    );
    style
        .opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
    style.stroke_width.ensure(caps.default_stroke_width()).add_keyframe(
        t_start_ms,
        start_stroke_width,
        Easing::Linear,
    );
    style.stroke_color.ensure(DEFAULT_WHITE).add_keyframe(
        t_start_ms,
        start_stroke_color,
        Easing::Linear,
    );
    style.stroke_progress.ensure(1.0).add_keyframe(
        t_start_ms,
        start_stroke_progress,
        Easing::Linear,
    );
    style
        .fill_opacity
        .ensure(1.0)
        .add_keyframe(t_start_ms, start_fill_opacity, Easing::Linear);
}

/// Preserve current track values at `t_start_ms` for delayed animations.
/// Used when an actor declaration has a delay but no duration.
pub(crate) fn preserve_delayed_values(
    style: &mut StyleTracks,
    geometry: &mut GeometryTracks,
    shape: &mut ShapeTracks,
    t_start_ms: u64,
) {
    preserve_instant_delayed_value(&mut shape.vector_paths, t_start_ms);
    preserve_instant_delayed_value(&mut geometry.position, t_start_ms);
    preserve_instant_delayed_value(&mut geometry.size, t_start_ms);
    preserve_instant_delayed_value(&mut geometry.layout_size, t_start_ms);
    preserve_instant_delayed_value(&mut shape.line_from, t_start_ms);
    preserve_instant_delayed_value(&mut shape.line_to, t_start_ms);
    preserve_instant_delayed_value(&mut shape.arc_angles, t_start_ms);
    preserve_instant_delayed_value(&mut shape.corner_radius, t_start_ms);
    preserve_instant_delayed_value(&mut style.color, t_start_ms);
    preserve_instant_delayed_value(&mut shape.shape_type, t_start_ms);
    preserve_instant_delayed_value(&mut style.opacity, t_start_ms);
    preserve_instant_delayed_value(&mut style.stroke_width, t_start_ms);
    preserve_instant_delayed_value(&mut style.stroke_color, t_start_ms);
    preserve_instant_delayed_value(&mut style.stroke_progress, t_start_ms);
    preserve_instant_delayed_value(&mut style.fill_opacity, t_start_ms);
}
