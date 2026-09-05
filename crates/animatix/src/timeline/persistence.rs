//! # Scene Persistence (CarryBag)
//!
//! CarryBag is the mechanism for carrying actors from one scene to the next
//! across a `play` edge in a multi-scene composition.  Actors marked with
//! `persist` are snapshotted at exit time and injected into the next scene's
//! timeline.
//!
//! ## Phase 1 scope
//!
//! This module provides:
//!
//! - `CarryBag` — bag holding snapshot entries keyed by actor label.
//! - `CarryEntry` — a single snapshotted actor plus its recursive subtree.
//! - [`Timeline::compute_carry_bag`] — snapshots all `persistent == true` actors and their children
//!   at a given time.
//! - `snapshot_track_at` — collapses all keyframes of a single track to a single frame at `t=0`
//!   sampled at `time_ms`.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::animation_track::{
    FilterTracks, GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use crate::timeline::property_track::{PropertyTrack, TrackAccessor};
use crate::timeline::{AnimationTrack, PlacementMode, PositionBinding, SceneDimensions, Timeline};

// ---------------------------------------------------------------------------
// CarryBag
// ---------------------------------------------------------------------------

/// Bag of actors to carry from one scene to the next.
///
/// Created by [`Timeline::compute_carry_bag`] and consumed by the composition
/// engine when injecting into the next scene.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CarryBag {
    /// Persistent actor entries, keyed by actor label.
    pub entries: BTreeMap<String, CarryEntry>,
}

/// A single actor to carry, with its snapshot and recursive subtree.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CarryEntry {
    /// Single-keyframe snapshot of the actor at t=0.
    pub track: AnimationTrack,
    /// Recursive subtree of carried children (keyed by child label).
    pub children: BTreeMap<String, CarryEntry>,
    /// Whether the actor carries the `persistent` flag forward.
    pub persistent: bool,
    /// Auto-color slot index (for `color: auto`), if the actor uses auto-color.
    pub auto_color_slot: Option<usize>,
}

// ---------------------------------------------------------------------------
// carry_bag build helpers
// ---------------------------------------------------------------------------

/// Recursively collect persistent actors and their children into a CarryBag.
fn collect_persistent_entries(
    timeline: &Timeline,
    label: &str,
    time_ms: u64,
    ancestors_are_persistent: bool,
    entries: &mut BTreeMap<String, CarryEntry>,
) {
    let is_persistent =
        timeline.persistence_flags.get(label).copied().unwrap_or(false) || ancestors_are_persistent;

    if !is_persistent {
        return;
    }

    // Build child entries recursively before creating this entry so that
    // children references can be moved in.
    let mut child_entries = BTreeMap::new();
    if let Some(track) = timeline.tracks.get(label) {
        for child_label in &track.children {
            collect_persistent_entries(
                timeline,
                child_label,
                time_ms,
                true, // parent is persistent, so children carry too
                &mut child_entries,
            );
        }
    }

    let Some(track) = timeline.tracks.get(label) else {
        tracing::warn!("Skipping persistent entry for missing track '{}'", label);
        return;
    };
    let track = snapshot_track_at(track, time_ms);

    let auto_color_slot = timeline.auto_color_assignments.get(label).copied();

    entries.insert(
        label.to_string(),
        CarryEntry {
            track,
            children: child_entries,
            persistent: timeline.persistence_flags.get(label).copied().unwrap_or(false),
            auto_color_slot,
        },
    );
}

impl Timeline {
    /// Snapshot the timeline at a given time, returning a `CarryBag`.
    ///
    /// Iterates `persistence_flags`, finds entries where `persistent == true`,
    /// snapshots the tracks via `snapshot_track_at`, and recursively
    /// snapshots children.
    ///
    /// `has_successor` — when `false`, any persisted actors will emit a
    /// `PersistTargetNotCarried` warning since there is no next scene.
    pub fn compute_carry_bag(&self, time_ms: u64, _has_successor: bool) -> CarryBag {
        let mut entries = BTreeMap::new();

        // Collect labels whose persistence flag is set.
        let persistent_labels: Vec<String> = self
            .persistence_flags
            .iter()
            .filter(|(_, persistent)| **persistent)
            .map(|(label, _)| label.clone())
            .collect();

        for label in persistent_labels {
            collect_persistent_entries(self, &label, time_ms, false, &mut entries);
        }

        CarryBag { entries }
    }
}

// ---------------------------------------------------------------------------
// inject_carry_bag
// ---------------------------------------------------------------------------

/// Inject a single `CarryEntry` into a target timeline.
///
/// `is_root` — when `true` the entry is a top-level carry-bag entry and its
/// label is added to `root_nodes`.  When `false` the entry is a child of a
/// carried container; it is inserted into `tracks` but not re-rooted.
///
/// Phase 3 re-rooting: if a root entry has a layout-managed or container-
/// relative position binding, we resolve its world-space position from the
/// source timeline and rewrite the binding to `Absolute`.
fn inject_entry(
    dest: &mut Timeline,
    label: &str,
    entry: &CarryEntry,
    is_root: bool,
    source_timeline: &Timeline,
    source_duration_ms: u64,
    dims: [f64; 2],
    _diagnostics: &mut Vec<Diagnostic>,
) {
    let mut track = entry.track.clone();

    // ── Phase 3: re-root layout-managed entries to absolute world position ──
    if is_root {
        let placement_mode = track
            .geometry
            .placement_mode
            .as_ref()
            .map(|pt| pt.evaluate(0))
            .unwrap_or(PlacementMode::Manual);

        let binding = track
            .geometry
            .position_binding
            .as_ref()
            .map(|pt| pt.evaluate(0))
            .unwrap_or(PositionBinding::Absolute);

        let needs_reroot = placement_mode == PlacementMode::LayoutManaged
            || matches!(
                binding,
                PositionBinding::ContainerDefault { .. } | PositionBinding::ContainerPercent { .. }
            );

        if needs_reroot {
            let scene_dims = SceneDimensions {
                width: dims[0] as u32,
                height: dims[1] as u32,
            };
            if let Some(world_affine) =
                source_timeline.actor_world_affine(label, source_duration_ms, scene_dims)
            {
                let t = world_affine.translation();
                let world_pos = [t.x as f32, t.y as f32];

                // Rewrite snapshot position to the resolved world coordinate.
                let pos_track = track.geometry.position.ensure([0.0, 0.0]);
                pos_track.keyframes.clear();
                pos_track.add_keyframe(0, world_pos, Easing::Linear);

                let bind_track = track.geometry.position_binding.ensure(PositionBinding::Absolute);
                bind_track.keyframes.clear();
                bind_track.add_keyframe(0, PositionBinding::Absolute, Easing::Linear);

                let mode_track = track.geometry.placement_mode.ensure(PlacementMode::Manual);
                mode_track.keyframes.clear();
                mode_track.add_keyframe(0, PlacementMode::Manual, Easing::Linear);
            }
        }
    }

    // ── Recursively inject children first so the parent track's child-label
    //    references resolve when the parent is later evaluated. ──
    for (child_label, child_entry) in &entry.children {
        inject_entry(
            dest,
            child_label,
            child_entry,
            false, // children are not re-rooted
            source_timeline,
            source_duration_ms,
            dims,
            _diagnostics,
        );
    }

    // Insert the (possibly rewritten) snapshot track.
    dest.tracks.insert(label.to_string(), track);

    // Re-root top-level entries only.
    if is_root && !dest.root_nodes.contains(&label.to_string()) {
        dest.root_nodes.push(label.to_string());
    }

    // Seed the persistence flag so chain-persistence works automatically.
    dest.persistence_flags.insert(label.to_string(), entry.persistent);

    // Carry auto-color slot so `color: auto` stays consistent.
    if let Some(slot) = entry.auto_color_slot {
        dest.auto_color_assignments.insert(label.to_string(), slot);
        // Ensure next_auto_color_index is large enough to not collide.
        if slot >= dest.next_auto_color_index {
            dest.next_auto_color_index = slot + 1;
        }
    }

    // Carry container metadata so layout still resolves inside scene B.
    if let Some(metadata) = source_timeline.container_metadata.get(label) {
        dest.container_metadata.insert(label.to_string(), metadata.clone());
    }
}

impl Timeline {
    /// Inject a `CarryBag` into this timeline **before** statement processing.
    ///
    /// Called by [`Timeline::build_with_carry`] immediately after construction
    /// and configuration loading, so that carried actors are visible to
    /// subsequent statements (assignments, action verbs, re-declarations).
    ///
    /// `source_timeline` — the predecessor scene's timeline, used for Phase 3
    /// world-position resolution of layout-managed roots.
    ///
    /// `source_duration_ms` — the predecessor scene's duration, sampled as the
    /// carry snapshot time.
    ///
    /// `dims` — scene pixel dimensions `[width, height]` used for percentage
    /// and anchor position resolution.
    pub fn inject_carry_bag(
        &mut self,
        carry: &CarryBag,
        source_timeline: &Timeline,
        source_duration_ms: u64,
        dims: [f64; 2],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for (label, entry) in &carry.entries {
            inject_entry(
                self,
                label,
                entry,
                true, // top-level entries are re-rooted
                source_timeline,
                source_duration_ms,
                dims,
                diagnostics,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// snapshot_track_at
// ---------------------------------------------------------------------------

/// Snapshot a single track at a given time, collapsing all keyframes to t=0.
///
/// The resulting `AnimationTrack` has exactly one keyframe per property at
/// `t=0` with the value sampled at `time_ms`.  Non-animated metadata
/// (`kind`, `procedural_plot`, `image`, `svg_paths`, `text_paths`,
/// `vector_paths`) is preserved unchanged.
///
/// `first_seen_ms` is set to `0` on the snapshot.
///
/// The `parent` and `children` fields are preserved as-is via `clone()` since
/// they are labels (not animated), so no special collapse is needed.  Any
/// consumer that re-inserts a snapshot into a fresh timeline should verify
/// that hierarchy labels still resolve (see `inject_carry_bag`).
pub fn snapshot_track_at(track: &AnimationTrack, time_ms: u64) -> AnimationTrack {
    let mut snapshot = track.clone();
    snapshot.first_seen_ms = 0;

    // Collapse each tier's property tracks to a single t=0 keyframe.
    collapse_geometry_tracks(&mut snapshot.geometry, time_ms);
    collapse_style_tracks(&mut snapshot.style, time_ms);
    collapse_filter_tracks(&mut snapshot.filter, time_ms);
    collapse_shape_tracks(&mut snapshot.shape, time_ms);
    collapse_text_tracks(&mut snapshot.text, time_ms);
    collapse_highlight_tracks(&mut snapshot.highlight, time_ms);

    // Collapse plot parameter tracks.
    for pt in snapshot.plot_param_tracks.values_mut() {
        collapse_property_track_inner(pt, time_ms);
    }

    // Clear func transitions — they represent live animation transitions,
    // not static snapshot state.
    snapshot.func_transitions.clear();

    // Collapse registry-driven extension slots to the same snapshot semantics
    // as built-in property tracks.
    for slot in snapshot.property_plan.iter_mut() {
        collapse_dyn_track(&mut slot.track, time_ms);
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

fn collapse_filter_tracks(tracks: &mut FilterTracks, time_ms: u64) {
    collapse_optional_track(&mut tracks.filter_blur, time_ms);
    collapse_optional_track(&mut tracks.filter_brightness, time_ms);
    collapse_optional_track(&mut tracks.filter_contrast, time_ms);
    collapse_optional_track(&mut tracks.filter_saturate, time_ms);
    collapse_optional_track(&mut tracks.filter_hue_rotate, time_ms);
    collapse_optional_track(&mut tracks.filter_sepia, time_ms);
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::dispatch::AnimationTrack;
    use crate::timeline::property_track::TrackAccessor;
    use crate::timeline::{CapturedEnv, Timeline};

    fn make_timeline_with_persistent_actors() -> Timeline {
        let mut timeline = Timeline::new();

        // Create a simple Rect actor
        let mut rect = AnimationTrack::new("rect1".to_string());
        rect.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        rect.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
        timeline.tracks.insert("rect1".to_string(), rect);

        // Create a child actor
        let mut child = AnimationTrack::new("child1".to_string());
        child
            .geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(0, [0.0, 0.0], Easing::Linear);
        child
            .geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(500, [100.0, 50.0], Easing::Linear);
        timeline.tracks.insert("child1".to_string(), child);

        // Set up parent-child relationship
        timeline.tracks.get_mut("rect1").unwrap().children.push("child1".to_string());

        // Mark rect1 as persistent
        timeline.persistence_flags.insert("rect1".to_string(), true);

        timeline
    }

    // -----------------------------------------------------------------------
    // snapshot_track_at
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_collapses_opacity_to_single_keyframe() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        track.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

        let snapshot = snapshot_track_at(&track, 500);
        let opacity_track = snapshot.style.opacity.unwrap();
        assert_eq!(opacity_track.keyframes.len(), 1);
        assert_eq!(opacity_track.keyframes.get(&0).unwrap().0, 0.75); // interpolated at 500ms
    }

    #[test]
    fn snapshot_collapses_position_to_single_keyframe() {
        let mut track = AnimationTrack::new("test".to_string());
        track
            .geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(0, [0.0, 0.0], Easing::Linear);
        track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
            1000,
            [200.0, 100.0],
            Easing::Linear,
        );

        let snapshot = snapshot_track_at(&track, 500);
        let pos_track = snapshot.geometry.position.unwrap();
        assert_eq!(pos_track.keyframes.len(), 1);
        assert_eq!(pos_track.keyframes.get(&0).unwrap().0, [100.0, 50.0]); // interpolated at 500ms
    }

    #[test]
    fn snapshot_preserves_kind() {
        let track = AnimationTrack::new("test".to_string());
        let snapshot = snapshot_track_at(&track, 0);
        assert_eq!(snapshot.kind, track.kind);
    }

    #[test]
    fn snapshot_sets_first_seen_ms_to_zero() {
        let mut track = AnimationTrack::new("test".to_string());
        track.first_seen_ms = 500;
        let snapshot = snapshot_track_at(&track, 500);
        assert_eq!(snapshot.first_seen_ms, 0);
    }

    #[test]
    fn snapshot_preserves_svg_paths() {
        use crate::timeline::VelloPath;
        let mut track = AnimationTrack::new("test".to_string());
        track.svg_paths.push(VelloPath::default());
        let snapshot = snapshot_track_at(&track, 0);
        assert_eq!(snapshot.svg_paths.len(), 1);
    }

    #[test]
    fn snapshot_clears_func_transitions() {
        let mut track = AnimationTrack::new("test".to_string());
        track.func_transitions.push(crate::timeline::plot::FuncTransition {
            start_ms: 0,
            end_ms: 1000,
            easing: Easing::Linear,
            from: crate::timeline::plot::FuncSource::Compiled(
                vec![],
                Box::new(crate::timeline::modifier_runtime::ir::CompiledExpr::Const(
                    crate::timeline::Value::Num(0.0),
                )),
                CapturedEnv::default(),
            ),
            to: crate::timeline::plot::FuncSource::Compiled(
                vec![],
                Box::new(crate::timeline::modifier_runtime::ir::CompiledExpr::Const(
                    crate::timeline::Value::Num(1.0),
                )),
                CapturedEnv::default(),
            ),
            blend_mode: crate::timeline::plot::FuncBlendMode::Output,
        });
        let snapshot = snapshot_track_at(&track, 0);
        assert!(snapshot.func_transitions.is_empty());
    }

    #[test]
    fn snapshot_preserves_procedural_plot() {
        use crate::ast::Expr;
        let mut track = AnimationTrack::new("test".to_string());
        track.procedural_plot = Some(crate::timeline::plot::ProceduralPlot {
            plot_type: crate::timeline::plot::ProceduralPlotKind::Curve(
                crate::timeline::plot::PlotCurveKind::Cartesian,
            ),
            kind: crate::timeline::plot::PlotCurveKind::Cartesian,
            func_args: vec!["x".to_string()],
            func_body: crate::timeline::modifier_runtime::ir::compile_expr(&Expr::Ident(
                "x".to_string(),
            ))
            .expect("test body should compile"),
            actor_label: "test".to_string(),
            param_names: vec![],
            p_x_domain: [0.0, 1.0],
            p_y_domain: [0.0, 1.0],
            p_size: [100.0, 100.0],
            padding: [0.0; 4],
            t_domain: [0.0, 1.0],
            tolerance: 0.1,
            max_depth: 8,
            resolution: 64,
            density: 16,
            levels: vec![],
            stroke_width: 2.0,
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [1.0, 1.0, 1.0, 1.0],
            params: vec![],
            extra_captures: CapturedEnv::default(),
        });
        let snapshot = snapshot_track_at(&track, 0);
        assert!(snapshot.procedural_plot.is_some());
    }

    #[test]
    fn snapshot_empty_track_has_no_keyframes() {
        let track = AnimationTrack::new("test".to_string());
        let snapshot = snapshot_track_at(&track, 0);
        // No property tracks should have keyframes
        assert!(
            snapshot.style.opacity.is_none()
                || snapshot.style.opacity.unwrap().keyframes.is_empty()
        );
        assert!(
            snapshot.geometry.position.is_none()
                || snapshot.geometry.position.unwrap().keyframes.is_empty()
        );
    }

    // -----------------------------------------------------------------------
    // compute_carry_bag
    // -----------------------------------------------------------------------

    #[test]
    fn compute_carry_bag_returns_empty_for_no_persistent_flags() {
        let timeline = Timeline::new();
        let bag = timeline.compute_carry_bag(0, true);
        assert!(bag.entries.is_empty());
    }

    #[test]
    fn compute_carry_bag_snapshots_persistent_actors() {
        let timeline = make_timeline_with_persistent_actors();
        let bag = timeline.compute_carry_bag(500, true);

        assert_eq!(bag.entries.len(), 1);
        let entry = bag.entries.get("rect1").expect("rect1 should be in carry bag");
        assert!(entry.persistent);

        // The snapshot should have opacity ≈ 0.75 (interpolated at 500ms between 1.0 and 0.5)
        let opacity =
            entry.track.style.opacity.as_ref().expect("snapshot should have opacity track");
        assert_eq!(opacity.keyframes.len(), 1);
        let (_, (val, _)) = opacity.keyframes.iter().next().unwrap();
        assert!((*val - 0.75).abs() < 1e-6);
    }

    #[test]
    fn compute_carry_bag_includes_children_of_persistent_actors() {
        let timeline = make_timeline_with_persistent_actors();
        let bag = timeline.compute_carry_bag(500, true);

        let entry = bag.entries.get("rect1").expect("rect1 should be in carry bag");
        assert_eq!(entry.children.len(), 1, "rect1's child should be carried");
        let child_entry = entry.children.get("child1").expect("child1 should be in carry bag");

        // child1 should be snapshotted at 500ms: position = [100.0, 50.0]
        let pos_track = child_entry
            .track
            .geometry
            .position
            .as_ref()
            .expect("child snapshot should have position track");
        assert_eq!(pos_track.keyframes.len(), 1);
        let (_, (val, _)) = pos_track.keyframes.iter().next().unwrap();
        assert_eq!(*val, [100.0, 50.0]);
    }

    #[test]
    fn compute_carry_bag_skips_non_persistent_actors() {
        let mut timeline = Timeline::new();

        let mut rect1 = AnimationTrack::new("rect1".to_string());
        rect1.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        timeline.tracks.insert("rect1".to_string(), rect1);

        let mut rect2 = AnimationTrack::new("rect2".to_string());
        rect2.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        timeline.tracks.insert("rect2".to_string(), rect2);

        // Only rect2 is persistent
        timeline.persistence_flags.insert("rect2".to_string(), true);

        let bag = timeline.compute_carry_bag(0, true);
        assert_eq!(bag.entries.len(), 1);
        assert!(bag.entries.contains_key("rect2"));
        assert!(!bag.entries.contains_key("rect1"));
    }

    // -----------------------------------------------------------------------
    // auto_color_slot carry
    // -----------------------------------------------------------------------

    #[test]
    fn compute_carry_bag_carries_auto_color_slot() {
        let mut timeline = Timeline::new();

        let mut actor = AnimationTrack::new("circle".to_string());
        actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        timeline.tracks.insert("circle".to_string(), actor);

        // Simulate the color: auto assignment — slot 0 was assigned.
        timeline.auto_color_assignments.insert("circle".to_string(), 0);
        timeline.next_auto_color_index = 1;

        timeline.persistence_flags.insert("circle".to_string(), true);

        let bag = timeline.compute_carry_bag(0, true);
        let entry = bag.entries.get("circle").expect("circle should be in bag");
        assert_eq!(entry.auto_color_slot, Some(0), "auto_color_slot must be carried");
    }

    #[test]
    fn inject_carry_bag_seeds_auto_color_assignments() {
        let mut source = Timeline::new();
        let mut actor = AnimationTrack::new("dot".to_string());
        actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        source.tracks.insert("dot".to_string(), actor);
        source.auto_color_assignments.insert("dot".to_string(), 2);
        source.next_auto_color_index = 3;
        source.persistence_flags.insert("dot".to_string(), true);

        let bag = source.compute_carry_bag(0, true);

        let mut dest = Timeline::new();
        dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

        assert_eq!(
            dest.auto_color_assignments.get("dot"),
            Some(&2),
            "injected auto_color_slot must appear in dest.auto_color_assignments"
        );
        // next_auto_color_index must be bumped to avoid collision
        assert!(
            dest.next_auto_color_index >= 3,
            "next_auto_color_index must be at least slot+1 to avoid collision"
        );
    }

    // -----------------------------------------------------------------------
    // Special actor kind carry (Svg, Image)
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_preserves_svg_actor_kind() {
        let mut track = AnimationTrack::new("icon".to_string());
        track.kind = crate::timeline::ActorKindId::Svg;
        track.svg_paths.push(crate::timeline::VelloPath::default());

        let snapshot = snapshot_track_at(&track, 0);
        assert_eq!(snapshot.kind, crate::timeline::ActorKindId::Svg);
        assert_eq!(snapshot.svg_paths.len(), 1, "svg_paths must survive snapshot");
    }

    #[test]
    fn snapshot_preserves_image_actor_kind() {
        let mut track = AnimationTrack::new("pic".to_string());
        track.kind = crate::timeline::ActorKindId::Image;

        let snapshot = snapshot_track_at(&track, 0);
        assert_eq!(snapshot.kind, crate::timeline::ActorKindId::Image);
    }

    #[test]
    fn inject_carry_bag_preserves_svg_paths() {
        use crate::timeline::VelloPath;

        let mut source = Timeline::new();
        let mut actor = AnimationTrack::new("icon".to_string());
        actor.kind = crate::timeline::ActorKindId::Svg;
        actor.svg_paths.push(VelloPath::default());
        actor.svg_paths.push(VelloPath::default());
        source.tracks.insert("icon".to_string(), actor);
        source.persistence_flags.insert("icon".to_string(), true);

        let bag = source.compute_carry_bag(0, true);

        let mut dest = Timeline::new();
        dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

        let icon = dest.tracks.get("icon").expect("icon must be carried");
        assert_eq!(icon.kind, crate::timeline::ActorKindId::Svg);
        assert_eq!(icon.svg_paths.len(), 2, "svg_paths must survive carry");
    }

    #[test]
    fn inject_carry_bag_preserves_procedural_plot() {
        use crate::ast::Expr;
        use crate::timeline::plot::{PlotCurveKind, ProceduralPlot};

        let mut source = Timeline::new();
        let mut actor = AnimationTrack::new("curve".to_string());
        actor.kind = crate::timeline::ActorKindId::PlotCurve;
        actor.procedural_plot = Some(ProceduralPlot {
            plot_type: crate::timeline::plot::ProceduralPlotKind::Curve(PlotCurveKind::Cartesian),
            kind: PlotCurveKind::Cartesian,
            func_args: vec!["x".to_string()],
            func_body: crate::timeline::modifier_runtime::ir::compile_expr(&Expr::Ident(
                "x".to_string(),
            ))
            .expect("test body should compile"),
            actor_label: "curve".to_string(),
            param_names: vec![],
            p_x_domain: [0.0, 10.0],
            p_y_domain: [-1.0, 1.0],
            p_size: [400.0, 300.0],
            padding: [0.0; 4],
            t_domain: [0.0, 1.0],
            tolerance: 0.1,
            max_depth: 8,
            resolution: 64,
            density: 16,
            levels: vec![],
            stroke_width: 2.0,
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [1.0, 1.0, 1.0, 1.0],
            params: vec![],
            extra_captures: CapturedEnv::default(),
        });
        source.tracks.insert("curve".to_string(), actor);
        source.persistence_flags.insert("curve".to_string(), true);

        let bag = source.compute_carry_bag(0, true);
        let mut dest = Timeline::new();
        dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

        let carried = dest.tracks.get("curve").expect("curve must be carried");
        assert_eq!(carried.kind, crate::timeline::ActorKindId::PlotCurve);
        assert!(carried.procedural_plot.is_some(), "procedural_plot must survive carry");
    }

    #[test]
    fn compute_carry_bag_propagates_child_persistence_flag() {
        let mut timeline = Timeline::new();

        let mut parent = AnimationTrack::new("parent".to_string());
        parent.children.push("child".to_string());
        timeline.tracks.insert("parent".to_string(), parent);

        let mut child = AnimationTrack::new("child".to_string());
        child
            .geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(0, [10.0, 20.0], Easing::Linear);
        timeline.tracks.insert("child".to_string(), child);

        // Only parent is persistent; child should be carried because it's a descendant
        timeline.persistence_flags.insert("parent".to_string(), true);

        let bag = timeline.compute_carry_bag(0, true);

        let entry = bag.entries.get("parent").expect("parent should be carried");
        assert!(entry.persistent);
        assert_eq!(entry.children.len(), 1);
        // Child should have persistent = false (not directly flagged)
        let child_entry = entry.children.get("child").expect("child should be in carry bag");
        assert!(!child_entry.persistent);
    }

    // -----------------------------------------------------------------------
    // Serde round-trip tests (feature-gated)
    // -----------------------------------------------------------------------

    #[cfg(feature = "serde")]
    #[test]
    fn carry_bag_serde_round_trip() {
        let timeline = make_timeline_with_persistent_actors();
        let bag = timeline.compute_carry_bag(500, true);

        // Serialize to JSON and back.
        let json = serde_json::to_string(&bag).expect("CarryBag should serialize");
        let bag2: CarryBag = serde_json::from_str(&json).expect("CarryBag should deserialize");

        // Verify the round-tripped bag has the same structure.
        assert_eq!(bag2.entries.len(), bag.entries.len());
        let entry = bag2.entries.get("rect1").expect("rect1 must survive round-trip");
        assert!(entry.persistent);
        assert_eq!(entry.children.len(), 1);

        // Opacity should be ≈ 0.75 (interpolated at 500ms).
        let opacity = entry
            .track
            .style
            .opacity
            .as_ref()
            .expect("opacity track must survive round-trip");
        let (_, (val, _)) = opacity.keyframes.iter().next().unwrap();
        assert!((*val - 0.75).abs() < 1e-4, "opacity expected ≈0.75, got {}", val);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn value_serde_round_trip() {
        use crate::timeline::env::Value;

        let cases: &[Value] = &[
            Value::Num(3.14),
            Value::Str("hello".into()),
            Value::Bool(true),
            Value::Vec2([1.0, 2.0]),
            Value::Vec4([0.1, 0.2, 0.3, 1.0]),
            Value::Color([0.5, 0.6, 0.7, 1.0]),
            Value::List(vec![crate::timeline::Value::Num(1.0), Value::Bool(false)].into()),
        ];
        for v in cases {
            let json = serde_json::to_string(v)
                .unwrap_or_else(|e| panic!("serialize failed for {:?}: {}", v, e));
            let v2: Value = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("deserialize failed for {:?}: {}", json, e));
            assert_eq!(*v, v2, "round-trip mismatch for {:?}", v);
        }
    }
}
