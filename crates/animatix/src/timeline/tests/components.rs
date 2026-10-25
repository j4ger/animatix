//! Round-trip tests for the non-layout components migrated in Phase 3a.

use animatix_text::transcript::SpeechTranscript;

use crate::timeline::Timeline;
use crate::timeline::{ContainerMetadata, Easing, EntityId, LayoutType, PropertyTrack};

fn sample_transcript() -> SpeechTranscript {
    SpeechTranscript {
        duration: 2.0,
        markers: std::collections::HashMap::from([("intro".to_string(), 0.25)]),
        segments: Vec::new(),
    }
}

fn sample_container_layout() -> ContainerMetadata {
    ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [8.0, 4.0],
        padding: [1.0, 2.0, 3.0, 4.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    }
}

#[test]
fn persistence_auto_color_transcript_components_roundtrip() {
    let mut timeline = Timeline::new();
    let id = timeline.spawn_entity("card", "Rect");
    assert_eq!(timeline.get_entity_id("card"), Some(id));

    // Spawn: an entity carries none of the optional components yet.
    let entity = timeline.entity("card").expect("card must exist");
    assert!(entity.persistence().is_none());
    assert!(entity.auto_color().is_none());
    assert!(entity.transcript().is_none());
    assert!(!timeline.persistence_flag("card"));
    assert_eq!(timeline.auto_color_slot("card"), None);
    assert!(timeline.transcript("card").is_none());

    // Set: every write goes through a Timeline entry point.
    timeline.set_persistence_flag("card", true);
    timeline.set_auto_color_slot("card", 4);
    let transcript = sample_transcript();
    assert!(timeline.set_transcript("card", transcript.clone()));

    // Read: EntityRef accessors and the Timeline helpers agree.
    let entity = timeline.entity("card").expect("card must exist");
    assert_eq!(entity.id(), id);
    assert!(entity.persistence().expect("persistence component").persistent);
    assert_eq!(entity.auto_color().expect("auto-color component").slot, 4);
    assert_eq!(&entity.transcript().expect("transcript component").0, &transcript);

    assert!(timeline.persistence_flag("card"));
    assert_eq!(timeline.auto_color_slot("card"), Some(4));
    assert_eq!(timeline.transcript("card"), Some(&transcript));

    let transcripts: Vec<_> = timeline.transcripts_iter().collect();
    assert_eq!(transcripts.len(), 1);
    assert_eq!(transcripts[0].0, id);
    assert_eq!(transcripts[0].1, &transcript);

    // EntityMut hands out the same components mutably.
    {
        let mut entity = timeline.entity_by_id_mut(id).expect("card must exist");
        entity.persistence_mut().expect("persistence component").persistent = false;
        entity.auto_color_mut().expect("auto-color component").slot = 5;
        entity.transcript_mut().expect("transcript component").0.duration = 3.0;
    }
    assert!(!timeline.persistence_flag("card"));
    assert_eq!(timeline.auto_color_slot("card"), Some(5));
    assert_eq!(timeline.transcript("card").map(|t| t.duration), Some(3.0));

    // Missing labels resolve like the old maps' absent entries.
    assert!(!timeline.persistence_flag("ghost"));
    assert_eq!(timeline.auto_color_slot("ghost"), None);
    assert!(timeline.transcript("ghost").is_none());

    // Replacement (`spawn_replacing`, the carry-injection path) clears the
    // optional components and does not inherit the previous entity's state.
    timeline.world.spawn_replacing("card", "Rect");
    let entity = timeline.entity("card").expect("card must exist");
    assert!(entity.persistence().is_none());
    assert!(entity.auto_color().is_none());
    assert!(entity.transcript().is_none());
}

#[test]
fn container_layout_component_roundtrip() {
    let mut timeline = Timeline::new();
    let row_id = timeline.spawn_entity("row", "Row");
    let a_id = timeline.spawn_entity("a", "Rect");

    // Spawn: no optional container-layout component yet.
    let entity = timeline.entity("row").expect("row must exist");
    assert!(entity.container_layout().is_none());
    assert!(timeline.container_layout("row").is_none());
    assert_eq!(timeline.container_layouts().count(), 0);
    assert!(timeline.container_layout_ids().is_empty());

    // Insert: the Timeline setter is the only write entry point.
    let mut metadata = sample_container_layout();
    metadata.child_order = vec![a_id];
    assert!(timeline.set_container_layout("row", metadata.clone()));
    assert!(
        !timeline.set_container_layout("ghost", metadata.clone()),
        "a layout cannot attach to a missing entity"
    );

    // Read through EntityRef and the Timeline helper.
    let entity = timeline.entity("row").expect("row must exist");
    assert_eq!(entity.id(), row_id);
    assert_eq!(entity.container_layout(), Some(&metadata));
    assert_eq!(timeline.container_layout("row"), Some(&metadata));

    // Timeline mutable access invalidates the frame cache.
    let epoch = timeline.content_epoch();
    {
        let layout = timeline.container_layout_mut("row").expect("container layout component");
        layout.gap = [16.0, 16.0];
    }
    assert!(
        timeline.content_epoch() > epoch,
        "container_layout_mut must invalidate the frame cache"
    );
    assert_eq!(timeline.container_layout("row").unwrap().gap, [16.0, 16.0]);

    // EntityMut hands out the same component mutably.
    {
        let mut entity = timeline.entity_mut("row").expect("row must exist");
        entity.container_layout_mut().expect("container layout component").align = "start".into();
    }
    assert_eq!(timeline.container_layout("row").unwrap().align, "start");

    // Iterator accessors resolve the destination entity, in slot order.
    let layouts: Vec<_> = timeline.container_layouts().collect();
    assert_eq!(layouts.len(), 1);
    assert_eq!(layouts[0].0, row_id);
    assert_eq!(layouts[0].1.align, "start");
    assert_eq!(timeline.container_layout_ids(), vec![row_id]);

    // Replacement (`spawn_replacing`, the carry-injection path) clears the
    // optional component and does not inherit the previous entity's state.
    timeline.world.spawn_replacing("row", "Rect");
    assert!(timeline.container_layout("row").is_none());
    assert!(timeline.container_layout_ids().is_empty());
}

#[test]
fn child_order_component_roundtrip() {
    let mut timeline = Timeline::new();
    let row = timeline.spawn_entity("row", "Row");
    let a = timeline.spawn_entity("a", "Rect");
    let b = timeline.spawn_entity("b", "Rect");

    // Spawn: the animated child order is an optional component.
    let entity = timeline.entity("row").expect("row must exist");
    assert!(entity.child_order().is_none());
    assert!(timeline.child_order("row").is_none());
    assert_eq!(timeline.child_orders_iter().count(), 0);
    assert!(!timeline.world.has_child_orders());

    // Insert: the Timeline setter is the only write entry point.
    let mut track = PropertyTrack::new(vec![a, b]);
    track.add_keyframe(0, vec![a, b], Easing::Linear);
    track.add_keyframe(500, vec![b, a], Easing::Linear);
    assert!(timeline.set_child_order_track("row", track));
    assert!(
        !timeline.set_child_order_track("ghost", PropertyTrack::new(Vec::new())),
        "a child-order track cannot attach to a missing entity"
    );

    // Read through EntityRef and the Timeline helper; Vec interpolation snaps
    // at the midpoint.
    let component = timeline.child_order("row").expect("child order component");
    assert_eq!(component.track.evaluate(0), vec![a, b]);
    assert_eq!(component.track.evaluate(250), vec![b, a]);
    assert_eq!(component.track.evaluate(500), vec![b, a]);
    let entity = timeline.entity("row").expect("row must exist");
    assert_eq!(entity.id(), row);
    assert_eq!(
        entity.child_order().expect("child order component").track.evaluate(500),
        vec![b, a]
    );
    assert!(timeline.world.has_child_orders());
    assert_eq!(timeline.child_orders_iter().count(), 1);

    // Every write invalidates the frame cache: Timeline's accessor and the
    // EntityMut view (the invalidating view constructor + the component write).
    let epoch = timeline.content_epoch();
    timeline
        .child_order_mut("row")
        .expect("child order component")
        .track
        .add_keyframe(1000, vec![a], Easing::Linear);
    assert!(
        timeline.content_epoch() > epoch,
        "child_order_mut must invalidate the frame cache"
    );
    let epoch = timeline.content_epoch();
    {
        let mut entity = timeline.entity_mut("row").expect("row must exist");
        entity.child_order_mut().expect("child order component").track.add_keyframe(
            1500,
            vec![b],
            Easing::Linear,
        );
    }
    assert!(timeline.content_epoch() > epoch, "entity_mut must invalidate the frame cache");
    assert_eq!(timeline.child_order("row").unwrap().track.evaluate(1500), vec![b]);

    // Replacement (`spawn_replacing`, the carry-injection path) clears the
    // optional component and does not inherit the previous entity's state.
    timeline.world.spawn_replacing("row", "Rect");
    assert!(timeline.child_order("row").is_none());
    assert_eq!(timeline.child_orders_iter().count(), 0);
    assert!(!timeline.world.has_child_orders());
}

#[test]
fn child_order_ids_follow_labels() {
    let mut timeline = Timeline::new();
    timeline.spawn_entity("row", "Row");
    let a = timeline.spawn_entity("a", "Rect");
    let b = timeline.spawn_entity("b", "Rect");
    let c = timeline.spawn_entity("c", "Rect");

    let mut metadata = sample_container_layout();
    metadata.child_order = vec![a, b, c];
    assert!(timeline.set_container_layout("row", metadata));

    // No animated component: both accessors read the authored order.
    assert_eq!(timeline.get_child_order_ids("row", 0), vec![a, b, c]);
    assert_eq!(timeline.get_child_order("row", 0), vec!["a", "b", "c"]);

    // An animated track samples entity ids; labels are derived from the ids.
    let mut track = PropertyTrack::new(vec![a, b, c]);
    track.add_keyframe(0, vec![a, b, c], Easing::Linear);
    track.add_keyframe(1000, vec![c, a, b], Easing::Linear);
    assert!(timeline.set_child_order_track("row", track));
    assert_eq!(timeline.get_child_order_ids("row", 0), vec![a, b, c]);
    assert_eq!(timeline.get_child_order("row", 0), vec!["a", "b", "c"]);
    assert_eq!(timeline.get_child_order_ids("row", 1000), vec![c, a, b]);
    assert_eq!(timeline.get_child_order("row", 1000), vec!["c", "a", "b"]);
    // `Vec<EntityId>` snaps at the midpoint, matching `Vec<String>`.
    assert_eq!(timeline.get_child_order_ids("row", 750), vec![c, a, b]);

    // An empty animated track falls back to the authored ids.
    assert!(timeline.set_child_order_track("row", PropertyTrack::new(Vec::new())));
    assert_eq!(timeline.get_child_order_ids("row", 0), vec![a, b, c]);

    // Layout admission carries ids and drops unadmitted children.
    let b_id = timeline.world.get_id("b").expect("b must exist");
    timeline.world_mut().geometry_mut(b_id).layout_size = Some(PropertyTrack::new([10.0, 10.0]));
    let admitted = timeline.container_layout("row").unwrap().layout_children(&timeline.world);
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].id, b_id);

    assert!(timeline.get_child_order("ghost", 0).is_empty());
    assert!(timeline.get_child_order_ids("ghost", 0).is_empty());
}

#[test]
fn effects_component_roundtrip() {
    use crate::timeline::effects::{EffectId, EffectParamKind, EffectStage};

    let mut timeline = Timeline::new();
    let scope = timeline.spawn_entity("scope", "Filter");
    assert!(timeline.entity("scope").expect("scope").effects().is_none());

    let mut stage = EffectStage::new("soft".to_string(), EffectId::new("Blur"));
    stage
        .param_track_mut("radius", EffectParamKind::F32)
        .add_keyframe(500, crate::timeline::property_engine::PropertyValue::F32(8.0));
    timeline.world.ensure_effects(scope).stages.push(stage);

    let effects = timeline.entity("scope").expect("scope").effects().expect("effects");
    assert_eq!(effects.stages.len(), 1);
    assert_eq!(effects.stages[0].label, "soft");
    assert_eq!(timeline.world.effects(scope).expect("effects").stages.len(), 1);
    // The effect parameter is part of the actor's duration.
    assert_eq!(timeline.duration_seconds(), 0.5);

    // Replacement (carry injection path) clears the optional component.
    timeline.world.spawn_replacing("scope", "Rect");
    assert!(timeline.entity("scope").expect("scope").effects().is_none());
}

// The ECS storage contract (`docs/ecs.md` §2.1, "Required means present"): an
// entity that exists has every `RequiredStore` entry. This is the rebuilt
// pin for the invariant the `TrackStore` test suite used to carry.
#[test]
fn every_entity_has_required_components() {
    let mut timeline = Timeline::new();
    timeline.world_mut().spawn("a", "Rect");
    timeline.world_mut().spawn("b", "Text");
    timeline.world_mut().spawn("c", "Equation");
    let world = timeline.world();

    for (label, id) in world.tracks.iter() {
        let required: Vec<(&str, bool)> = vec![
            ("hierarchy", world.hierarchy.try_get(id).is_some()),
            ("highlight", world.highlight.try_get(id).is_some()),
            ("tagged_tracks", world.tagged_tracks.try_get(id).is_some()),
            ("actor_types", world.actor_types.try_get(id).is_some()),
            ("shape_memos", world.shape_memos.try_get(id).is_some()),
            ("pending_shape_bounds", world.pending_shape_bounds.try_get(id).is_some()),
            ("bounds_slots", world.bounds_slots.try_get(id).is_some()),
            ("style", world.style.try_get(id).is_some()),
            ("geometry", world.geometry.try_get(id).is_some()),
            ("text", world.text.try_get(id).is_some()),
            ("shape", world.shape.try_get(id).is_some()),
            ("property_plan", world.property_plan.try_get(id).is_some()),
        ];
        let missing: Vec<&str> =
            required.iter().filter(|(_, ok)| !ok).map(|(name, _)| *name).collect();
        assert!(
            missing.is_empty(),
            "entity '{label}' (e{}) is missing required stores: {missing:?}",
            id.index()
        );
    }

    // And replacement keeps the invariant: spawn_replacing resets components
    // but must never leave a store slot empty.
    timeline.world_mut().spawn_replacing("b", "Graph");
    let world = timeline.world();
    let b = world.get_id("b").expect("b still exists");
    assert!(world.property_plan.try_get(b).is_some());
    assert!(world.geometry.try_get(b).is_some());
}

// `World::spawn_replacing` must reset every authored-data required store
// (`docs/ecs.md` §2.1): `tagged_tracks` once survived a replacement because
// no test pinned its reset. The memo stores (`shape_memos`,
// `pending_shape_bounds` is reset here, `shape_memos` has private fields and
// is covered by the presence invariant above) hold no authored state.
#[test]
fn spawn_replacing_resets_every_authored_required_store() {
    use crate::timeline::DynTrack;
    use crate::timeline::plan::{PropertyKind, PropertyPlan, PropertySlot};

    let mut timeline = Timeline::new();
    let parent = timeline.spawn_entity("parent", "Rect");
    let id = timeline.spawn_entity("card", "Rect");

    // Dirty every required store that holds authored state.
    timeline.world_mut().link(parent, id);
    timeline.world_mut().set_bounds_slot(id, 7);
    timeline
        .world_mut()
        .pending_shape_bounds
        .get_mut(id)
        .set(Some(Some(kurbo::Rect::ZERO)));
    {
        let world = timeline.world_mut();
        world.highlight.get_mut(id).highlight_color = Some({
            let mut track = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            track.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
            track
        });
        world.tagged_tracks_mut(id).insert("legend".to_string(), None);
        world.style_mut(id).color = Some(PropertyTrack::new([0.0, 0.0, 0.0, 1.0]));
        world.geometry_mut(id).position = Some(PropertyTrack::new([1.0, 2.0]));
        world.text_mut(id).text_content = Some(PropertyTrack::new("hi".to_string()));
        world.shape_mut(id).shape_type = Some(PropertyTrack::new(crate::timeline::ShapeType::Rect));
        *world.property_plan.get_mut(id) = PropertyPlan::new(vec![PropertySlot {
            id: animatix_core::property::PropertyId(0),
            kind: PropertyKind::F32,
            track: DynTrack::empty(PropertyKind::F32),
        }]);
    }

    // The bounds slot is stamped lazily by `EvalCaches::ensure_bounds_registry`
    // (a fresh spawn reads `0`), so pin "back to fresh-spawn state", not a
    // sentinel value.
    let fresh = timeline.spawn_entity("fresh", "Rect");
    timeline.world_mut().spawn_replacing("card", "Ellipse");

    let world = timeline.world();
    assert_eq!(world.actor_type(id), "Ellipse");
    assert_eq!(world.hierarchy(parent).children, Vec::new(), "unlink ran first");
    assert!(world.hierarchy(id).parent.is_none(), "hierarchy resets");
    assert_eq!(
        world.bounds_slot(id),
        world.bounds_slot(fresh),
        "bounds slot resets to unstamped state"
    );
    assert!(
        world.pending_shape_bounds.get(id).get().is_none(),
        "pending bounds handoff resets"
    );
    assert!(world.highlight.get(id).highlight_color.is_none(), "highlight resets");
    assert!(world.tagged_tracks(id).is_empty(), "tagged tracks reset");
    assert!(world.style.get(id).color.is_none(), "style resets");
    assert!(world.geometry.get(id).position.is_none(), "geometry resets");
    assert!(world.text.get(id).text_content.is_none(), "text resets");
    assert!(world.shape.get(id).shape_type.is_none(), "shape resets");
    assert!(world.property_plan.get(id).is_empty(), "property plan resets");
}
