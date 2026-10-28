//! Carry-bag injection into a successor scene (`Timeline::inject_carry_bag`).

use super::*;

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
    let mut geometry = entry.geometry.clone();

    // ── Phase 3: re-root layout-managed entries to absolute world position ──
    if is_root {
        let placement_mode = geometry
            .placement_mode
            .as_ref()
            .map(|pt| pt.evaluate(0))
            .unwrap_or(PlacementMode::Manual);

        let binding = geometry
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
                let pos_track = geometry.position.ensure([0.0, 0.0]);
                pos_track.keyframes.clear();
                pos_track.add_keyframe(0, world_pos, Easing::Linear);

                let bind_track = geometry.position_binding.ensure(PositionBinding::Absolute);
                bind_track.keyframes.clear();
                bind_track.add_keyframe(0, PositionBinding::Absolute, Easing::Linear);

                let mode_track = geometry.placement_mode.ensure(PlacementMode::Manual);
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

    // (Re-)create the entity from the snapshot's per-tier data. `spawn_replacing`
    // keeps the label's entity id and resets its components, matching the old
    // insert-replacement semantics. Every component write goes through one
    // `world_mut()` borrow so the invalidation funnel holds on the injection
    // path too (`docs/ecs.md` §2.1).
    let world = dest.world_mut();
    let id = world.spawn_replacing(label, &entry.actor_type);
    // Identity lives in `World` and `spawn_replacing` reset it; restore the
    // carried primitive type so caps and plan lookups resolve again.
    world.set_identity(id, &entry.actor_type);
    world.meta_mut(id).first_seen_ms = 0;
    world.set_payload(id, entry.payload.clone());
    let caps = world.caps(id);
    if caps.is_shape
        || caps.shape.is_some()
        || caps.connector
        || caps.callout
        || entry.shape.has_any_keyframes()
        || entry.shape.shape_type.is_some()
    {
        world.shape.insert(id, entry.shape.clone());
    }
    if caps.text.is_some()
        || caps.text_paths
        || entry.text.has_any_keyframes()
        || entry.text.text_content.is_some()
    {
        world.text.insert(id, entry.text.clone());
    }
    if entry.highlight.has_any_keyframes()
        || entry.highlight.highlight_color.is_some()
        || entry.highlight.highlight_opacity.is_some()
    {
        world.highlight.insert(id, entry.highlight.clone());
    }
    *world.property_plan.get_mut(id) = entry.property_plan.clone();
    *world.tagged_tracks_mut(id) = entry.tagged.clone();
    *world.geometry.get_mut(id) = geometry.clone();
    *world.style.get_mut(id) = entry.style.clone();
    if let Some(effects) = entry.effects.clone() {
        world.effects.insert(id, effects);
    }
    if let Some(bounds) = entry.local_bounds {
        world.set_local_bounds(id, bounds);
    }

    // Re-root top-level entries only. `adopt` already placed the entity in the
    // root set; the call makes the intent explicit and keeps the ordering rule
    // (append on declaration) in one place.
    if is_root {
        world.set_parent_label(label, None);
    }

    for child_label in entry.children.keys() {
        if let Some(child_id) = world.get_id(child_label) {
            world.link(id, child_id);
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
        world.ensure_legend(id).clone_from(legend);
    }

    // Caps override and declaration-authored meta flags ride the bag:
    // `set_identity` above re-derives caps from the type (clearing any
    // override) and `spawn_replacing` reset `EntityMeta`.
    if let Some(caps) = entry.caps {
        world.set_caps(id, caps);
    }
    if let Some(meta) = entry.meta {
        let entity_meta = world.meta_mut(id);
        entity_meta.visible = meta.visible;
        entity_meta.camera_follow = meta.camera_follow;
        entity_meta.parallax = meta.parallax;
        entity_meta.hidden_by_default = meta.hidden_by_default;
    }

    // The animated child-order track holds a completed reorder's end state;
    // its keyframes store scene-local ids, so remap the carried labels back
    // through this scene's entities (a label that was not carried drops out,
    // matching the layout admission's missing-child behaviour).
    if let Some(carried) = &entry.child_order {
        let resolve = |labels: &[String]| -> Vec<EntityId> {
            labels.iter().filter_map(|l| world.get_id(l)).collect()
        };
        let mut track = PropertyTrack::new(resolve(&carried.default_order));
        for (time, labels, easing) in &carried.keyframes {
            track.add_keyframe(*time, resolve(labels), *easing);
        }
        world.child_order.insert(id, ChildOrderComponent { track });
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
