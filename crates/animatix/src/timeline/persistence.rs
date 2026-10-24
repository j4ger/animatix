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

use animatix_text::transcript::SpeechTranscript;

use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::animation_track::{
    GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use crate::timeline::effects::EffectChainTrack;
use crate::timeline::property_track::{PropertyTrack, TrackAccessor};
use crate::timeline::{
    AnimationTrack, ContainerMetadata, CoordinateFrameComponent, PlacementMode, PositionBinding,
    SceneDimensions, Timeline,
};

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
    /// The scene camera's axes, when the scene wrote `persist camera`.
    ///
    /// The camera is not an actor and owns no track, so it cannot ride in
    /// `entries`; a scene that ends mid-push hands over three numbers instead.
    pub camera: Option<CarriedCamera>,
}

/// The three camera axes as one scene left them.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug, Default)]
pub struct CarriedCamera {
    /// `camera.at` / `camera.position` / `camera.pan`, in screen pixels.
    pub pan: [f32; 2],
    /// `camera.zoom` / `camera.scale`; 1.0 is no magnification.
    pub zoom: f32,
    /// `camera.rotation` / `camera.spin`, in radians.
    pub spin: f32,
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
    /// Container layout metadata, if the actor is a `Row`/`Col`/`Grid`/`Stack`.
    ///
    /// Carried so a re-rooted container keeps its layout inside the successor
    /// scene instead of falling back to a plain, un-laid-out actor.
    #[cfg_attr(feature = "serde", serde(default))]
    pub container_layout: Option<ContainerMetadata>,
    /// Speech transcript attached to the actor, if it declares one.
    ///
    /// Carried as data rather than re-read from the destination scene's assets:
    /// the transcript belongs to the actor, so a carried `Captions`/`Audio`
    /// actor keeps word timings that its destination scene never declares.
    #[cfg_attr(feature = "serde", serde(default))]
    pub transcript: Option<SpeechTranscript>,
    /// Post-processing effect chain snapshot for a compositing scope.
    #[cfg_attr(feature = "serde", serde(default))]
    pub effects: Option<EffectChainTrack>,
    /// Coordinate frame of a plot host (a `Graph` and any `plot_host`
    /// primitive), if the actor has one.
    ///
    /// Carried so a persistent `Graph` keeps its domains/scales/padding in the
    /// successor scene, where its child plots resolve coordinates from it.
    /// Domain values are scene-relative, so no entity-id remap is needed.
    #[cfg_attr(feature = "serde", serde(default))]
    pub coordinate_frame: Option<CoordinateFrameComponent>,
    /// Primitive payload snapshot (plot parametrisation, SVG/image state).
    #[cfg_attr(feature = "serde", serde(default))]
    pub payload: crate::primitives::PrimitivePayload,
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
    let is_persistent = timeline.persistence_flag(label) || ancestors_are_persistent;

    if !is_persistent {
        return;
    }

    // Build child entries recursively before creating this entry so that
    // children references can be moved in.
    let mut child_entries = BTreeMap::new();
    for child_label in timeline.child_labels(label) {
        collect_persistent_entries(
            timeline,
            child_label,
            time_ms,
            true, // parent is persistent, so children carry too
            &mut child_entries,
        );
    }

    let Some(track) = timeline.world.tracks.get(label) else {
        tracing::warn!("Skipping persistent entry for missing track '{}'", label);
        return;
    };
    let effects = timeline.world.effects(track.id).map(|chain| snapshot_effects(chain, time_ms));
    let payload = timeline
        .world
        .payload(track.id)
        .map_or_else(crate::primitives::PrimitivePayload::default, |payload| {
            snapshot_payload_at(payload, time_ms)
        });
    let track = snapshot_track_at(track, time_ms);

    entries.insert(
        label.to_string(),
        CarryEntry {
            track,
            children: child_entries,
            persistent: timeline.persistence_flag(label),
            auto_color_slot: timeline.auto_color_slot(label),
            container_layout: timeline.container_layout(label).cloned(),
            transcript: timeline.transcript(label).cloned(),
            coordinate_frame: timeline.coordinate_frame_for(label).cloned(),
            effects,
            payload,
        },
    );
}

impl Timeline {
    /// Snapshot the timeline at a given time, returning a `CarryBag`.
    ///
    /// Iterates the entities' [`crate::timeline::PersistenceComponent`]s, finds
    /// entries where `persistent == true`, snapshots the tracks via
    /// `snapshot_track_at`, and recursively snapshots children.
    ///
    /// `has_successor` — when `false`, any persisted actors will emit a
    /// `PersistTargetNotCarried` warning since there is no next scene.
    pub fn compute_carry_bag(&self, time_ms: u64, _has_successor: bool) -> CarryBag {
        let mut entries = BTreeMap::new();

        // `persist camera` is the camera's own entry: no track to snapshot, so
        // the axes are sampled where the scene ends.
        let camera = self.camera_persistent.then(|| {
            let (pan, zoom, spin) = self.camera.values_at(time_ms);
            CarriedCamera { pan, zoom, spin }
        });

        // Collect labels whose persistence flag is set.
        let persistent_labels: Vec<&str> = self
            .persistence_flags_iter()
            .filter(|(_, persistent)| *persistent)
            .map(|(label, _)| label)
            .collect();

        for label in persistent_labels {
            if label == crate::timeline::camera::CAMERA_TARGET {
                // Not an actor: handled above, and `collect_persistent_entries`
                // would only warn that the track is missing.
                continue;
            }
            collect_persistent_entries(self, label, time_ms, false, &mut entries);
        }

        CarryBag { entries, camera }
    }
}

// ---------------------------------------------------------------------------
// inject_carry_bag
// ---------------------------------------------------------------------------

/// Inject a single `CarryEntry` into a target timeline.
///
/// `is_root` — when `true` the entry is a top-level carry-bag entry and its
/// entity is re-rooted.  When `false` the entry is a child of a carried
/// container; it is inserted and linked under its parent by the recursive
/// caller.  The hierarchy is rebuilt through [`crate::timeline::World`] after
/// the snapshot track is inserted — a `CarryEntry` stores only the track, the
/// links are re-established from `entry.children`.
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
    // A carry entry may come from a legacy payload with no recorded
    // `actor_type`; resolve it from `kind` once here (the single compatibility
    // boundary — runtime lookups never fall back).
    track.normalize_identity();

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

    // ── Recursively inject children first, then insert this entity and link
    //    the children through `World` (the hierarchy is not stored on the
    //    snapshot track). ──
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

    // Insert the (possibly rewritten) snapshot track. `spawn_replacing` keeps
    // the label's entity id and resets its hierarchy component, matching the
    // old `TrackStore::insert` replacement semantics.
    let id = dest.world.spawn_replacing(label, track);
    dest.world.meta_mut(id).first_seen_ms = 0;
    dest.world.payload.insert(id, entry.payload.clone());
    if let Some(effects) = entry.effects.clone() {
        dest.world.effects.insert(id, effects);
    }

    // Re-root top-level entries only. `adopt` already placed the entity in the
    // root set; the call makes the intent explicit and keeps the ordering rule
    // (append on declaration) in one place.
    if is_root {
        dest.world.set_parent_label(label, None);
    }

    for child_label in entry.children.keys() {
        if let Some(child_id) = dest.world.get_id(child_label) {
            dest.world.link(id, child_id);
        }
    }

    // Legend data is a component, so it is not inside the carried track
    // snapshot and `spawn_replacing` cleared it. Restore the source actor's
    // participation data (color and presentation settings) from the source
    // timeline: without it, a carried colored actor would silently drop out of
    // the destination scene's legend scan. Entries are derived data — the
    // post-build scan overwrites them for legend hosts in the destination.
    if let Some(source_id) = source_timeline.world.get_id(label)
        && let Some(legend) = source_timeline.world.legend(source_id)
    {
        dest.world.ensure_legend(id).clone_from(legend);
    }

    // Seed the persistence flag so chain-persistence works automatically.
    dest.set_persistence_flag(label, entry.persistent);

    // Carry auto-color slot so `color: auto` stays consistent.
    if let Some(slot) = entry.auto_color_slot {
        dest.set_auto_color_slot(label, slot);
        // Ensure next_auto_color_index is large enough to not collide.
        if slot >= dest.next_auto_color_index {
            dest.next_auto_color_index = slot + 1;
        }
    }

    // Carry the speech transcript so a carried Audio/Captions actor keeps its
    // word timings in the destination scene.
    if let Some(transcript) = &entry.transcript {
        dest.set_transcript(label, transcript.clone());
    }

    // Carry the coordinate frame so a persistent `Graph` keeps its domain and
    // hosted children resolve against it in the destination scene. The domain
    // values are scene-relative, so there is no id remap to do.
    if let Some(frame) = &entry.coordinate_frame {
        dest.set_coordinate_frame(label, frame.clone());
    }

    // Carry the container layout so a re-parented container still lays out in
    // the destination scene. `spawn_replacing` cleared the optional component
    // above, so this restores it from the bag rather than re-reading the
    // source timeline.
    //
    // `CarryEntry::container_layout` stores *source-scene* entity ids in
    // `child_order`, so remap each id through its source label to the
    // destination entity of the same label. A child that was not carried (or
    // whose id is stale) has no destination entity and is dropped, matching
    // the layout admission's missing-child behaviour.
    if let Some(metadata) = &entry.container_layout {
        let mut metadata = metadata.clone();
        metadata.child_order = metadata
            .child_order
            .iter()
            .filter_map(|source_id| source_timeline.get_entity_label(*source_id))
            .filter_map(|child_label| dest.get_entity_id(child_label))
            .collect();
        dest.set_container_layout(label, metadata);
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
        // The camera starts where the previous scene left it. Seeded at t=0, so
        // this scene's own `camera.*` writes still take over from their stamps;
        // identity axes are skipped, which keeps a scene that inherited nothing
        // meaningful out of the per-frame camera path entirely.
        if let Some(camera) = carry.camera {
            self.camera.seed_from(camera.pan, camera.zoom, camera.spin, 0);
            // Sticky, like every other carried target: the flag propagates so a
            // pushed-in camera stays pushed through a chain of scenes until an
            // explicit `remove camera`.
            self.set_persistence_flag(crate::timeline::camera::CAMERA_TARGET, true);
        }
    }
}

// ---------------------------------------------------------------------------
// snapshot_track_at
// ---------------------------------------------------------------------------

/// Snapshot a primitive payload at `time_ms` (plot params collapsed, live
/// transitions dropped), mirroring [`snapshot_track_at`] for the payload tier.
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

/// Snapshot all animated tiers of a track at `time_ms`, collapsing each to a
/// single t=0 keyframe. Primitive payload state lives in a component and is
/// snapshotted separately by [`snapshot_payload_at`].
pub fn snapshot_track_at(track: &AnimationTrack, time_ms: u64) -> AnimationTrack {
    let mut snapshot = track.clone();

    // Collapse each tier's property tracks to a single t=0 keyframe.
    collapse_geometry_tracks(&mut snapshot.geometry, time_ms);
    collapse_style_tracks(&mut snapshot.style, time_ms);
    collapse_shape_tracks(&mut snapshot.shape, time_ms);
    collapse_text_tracks(&mut snapshot.text, time_ms);
    collapse_highlight_tracks(&mut snapshot.highlight, time_ms);

    // Collapse registry-driven extension slots to the same snapshot semantics
    // as built-in property tracks.
    for slot in snapshot.property_plan.iter_mut() {
        collapse_dyn_track(&mut slot.track, time_ms);
    }

    snapshot
}

fn snapshot_effects(chain: &EffectChainTrack, time_ms: u64) -> EffectChainTrack {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
