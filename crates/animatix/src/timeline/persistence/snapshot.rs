//! Per-tier snapshot/collapse helpers for `CarryBag` entries.

use super::*;

// ---------------------------------------------------------------------------
// snapshot_track_at
// ---------------------------------------------------------------------------

/// Snapshot a primitive payload at `time_ms` (plot params collapsed, live
/// transitions dropped), mirroring the per-tier collapse helpers.
pub fn snapshot_payload_at(
    payload: &crate::primitives::PrimitivePayload,
    time_ms: u64,
) -> crate::primitives::PrimitivePayload {
    let mut snapshot = payload.clone();
    if let crate::primitives::PrimitivePayload::Plot(plot) = &mut snapshot {
        for pt in plot.plot_param_tracks.values_mut() {
            collapse_property_track_inner(pt, time_ms);
        }
        plot.func_transitions.clear();
    }
    snapshot
}

/// Snapshot the highlight tier at `time_ms` (single keyframe per track).
pub fn snapshot_highlight_at(
    highlight: &crate::timeline::animation_track::HighlightTracks,
    time_ms: u64,
) -> crate::timeline::animation_track::HighlightTracks {
    let mut snapshot = highlight.clone();
    collapse_highlight_tracks(&mut snapshot, time_ms);
    snapshot
}

pub(super) fn snapshot_effects(chain: &EffectChainTrack, time_ms: u64) -> EffectChainTrack {
    let mut snapshot = chain.clone();
    for stage in snapshot.stages.iter_mut() {
        for param in stage.params.values_mut() {
            collapse_dyn_track(param, time_ms);
        }
        collapse_dyn_track(&mut stage.enabled, time_ms);
    }
    snapshot
}

fn collapse_dyn_track(track: &mut crate::timeline::DynTrack, time_ms: u64) {
    match track {
        crate::timeline::DynTrack::F32(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::U32(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::Bool(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::Vec2(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::Vec4(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::String(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::PointList(t) => collapse_optional_track(t, time_ms),
        crate::timeline::DynTrack::Generic(t) => collapse_optional_track(t, time_ms),
    }
}

// ---------------------------------------------------------------------------
// Per-tier collapse helpers
// ---------------------------------------------------------------------------

fn collapse_geometry_tracks(tracks: &mut GeometryTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.position, time_ms);
    collapse_optional_track(&mut tracks.motion_offset, time_ms);
    collapse_optional_track(&mut tracks.rotation, time_ms);
    collapse_optional_track(&mut tracks.scale, time_ms);
    collapse_optional_track(&mut tracks.transform, time_ms);
    collapse_optional_track(&mut tracks.placement_mode, time_ms);
    collapse_optional_track(&mut tracks.position_binding, time_ms);
    collapse_optional_track(&mut tracks.size, time_ms);
    collapse_optional_track(&mut tracks.layout_size, time_ms);
    collapse_optional_track(&mut tracks.min_width, time_ms);
    collapse_optional_track(&mut tracks.min_height, time_ms);
    collapse_optional_track(&mut tracks.max_height, time_ms);
    collapse_optional_track(&mut tracks.label_at, time_ms);
    // size_spec is non-animated configuration, keep as-is
}

fn collapse_style_tracks(tracks: &mut StyleTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.color, time_ms);
    collapse_optional_track(&mut tracks.opacity, time_ms);
    collapse_optional_track(&mut tracks.stroke_width, time_ms);
    collapse_optional_track(&mut tracks.stroke_color, time_ms);
    collapse_optional_track(&mut tracks.stroke_progress, time_ms);
    collapse_optional_track(&mut tracks.fill_opacity, time_ms);
    collapse_optional_track(&mut tracks.line_cap, time_ms);
    collapse_optional_track(&mut tracks.line_join, time_ms);
    collapse_optional_track(&mut tracks.morph_options, time_ms);
}

fn collapse_shape_tracks(tracks: &mut ShapeTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.shape_type, time_ms);
    collapse_optional_track(&mut tracks.line_from, time_ms);
    collapse_optional_track(&mut tracks.line_to, time_ms);
    collapse_optional_track(&mut tracks.head_size, time_ms);
    collapse_optional_track(&mut tracks.arc_angles, time_ms);
    collapse_optional_track(&mut tracks.points, time_ms);
    collapse_optional_track(&mut tracks.commands, time_ms);
    // vector_paths is preserved unchanged per spec
}

fn collapse_text_tracks(tracks: &mut TextTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.text_content, time_ms);
    collapse_optional_track(&mut tracks.font_family, time_ms);
    collapse_optional_track(&mut tracks.font_size, time_ms);
    collapse_optional_track(&mut tracks.font_weight, time_ms);
    collapse_optional_track(&mut tracks.font_style, time_ms);
    collapse_optional_track(&mut tracks.line_height, time_ms);
    collapse_optional_track(&mut tracks.letter_spacing, time_ms);
    collapse_optional_track(&mut tracks.word_spacing, time_ms);
    collapse_optional_track(&mut tracks.text_max_width, time_ms);
    collapse_optional_track(&mut tracks.text_align, time_ms);
    collapse_optional_track(&mut tracks.text_anchor, time_ms);
    collapse_optional_track(&mut tracks.overflow, time_ms);
    // text_paths is preserved unchanged per spec
    collapse_optional_track(&mut tracks.ascent, time_ms);
    collapse_optional_track(&mut tracks.descent, time_ms);
    collapse_optional_track(&mut tracks.baseline, time_ms);
}

fn collapse_highlight_tracks(tracks: &mut HighlightTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.highlight_color, time_ms);
    collapse_optional_track(&mut tracks.highlight_opacity, time_ms);
    collapse_optional_track(&mut tracks.highlight_padding, time_ms);
    collapse_optional_track(&mut tracks.highlight_radius, time_ms);
    // highlight_blend is non-animated configuration, keep as-is
}

// ---------------------------------------------------------------------------
// Generic collapse helper
// ---------------------------------------------------------------------------

/// Collapse an `Option<PropertyTrack<T>>` to a single keyframe at t=0
/// holding the sampled value at `time_ms`.
fn collapse_optional_track<T: crate::timeline::property_track::Interpolate>(
    track: &mut Option<PropertyTrack<T>>,
    time_ms: u64,
) {
    if let Some(pt) = track.as_mut() {
        collapse_property_track_inner(pt, time_ms);
    }
}

/// Collapse a `PropertyTrack<T>` to a single keyframe at t=0.
fn collapse_property_track_inner<T: crate::timeline::property_track::Interpolate>(
    pt: &mut PropertyTrack<T>,
    time_ms: u64,
) {
    let value = pt.evaluate(time_ms);
    pt.keyframes.clear();
    pt.keyframes.insert(0, (value, Easing::Linear));
}

/// Snapshot a property plan at `time_ms`, collapsing each slot track.
pub fn snapshot_property_plan_at(
    plan: &crate::timeline::plan::PropertyPlan,
    time_ms: u64,
) -> crate::timeline::plan::PropertyPlan {
    let mut snapshot = plan.clone();
    for slot in snapshot.iter_mut() {
        collapse_dyn_track(&mut slot.track, time_ms);
    }
    snapshot
}

/// Snapshot the shape tier at `time_ms` (single keyframe per track).
pub fn snapshot_shape_at(
    shape: &crate::timeline::animation_track::ShapeTracks,
    time_ms: u64,
) -> crate::timeline::animation_track::ShapeTracks {
    let mut snapshot = shape.clone();
    collapse_shape_tracks(&mut snapshot, time_ms);
    snapshot
}

/// Snapshot the text tier at `time_ms` (single keyframe per track).
pub fn snapshot_text_at(
    text: &crate::timeline::animation_track::TextTracks,
    time_ms: u64,
) -> crate::timeline::animation_track::TextTracks {
    let mut snapshot = text.clone();
    collapse_text_tracks(&mut snapshot, time_ms);
    snapshot
}

/// Snapshot the geometry tier at `time_ms` (single keyframe per track).
pub fn snapshot_geometry_at(
    geometry: &crate::timeline::animation_track::GeometryTracks,
    time_ms: u64,
) -> crate::timeline::animation_track::GeometryTracks {
    let mut snapshot = geometry.clone();
    collapse_geometry_tracks(&mut snapshot, time_ms);
    snapshot
}

/// Snapshot the style tier at `time_ms` (single keyframe per track).
pub fn snapshot_style_at(
    style: &crate::timeline::animation_track::StyleTracks,
    time_ms: u64,
) -> crate::timeline::animation_track::StyleTracks {
    let mut snapshot = style.clone();
    collapse_style_tracks(&mut snapshot, time_ms);
    snapshot
}
