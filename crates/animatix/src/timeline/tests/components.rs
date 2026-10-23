//! Round-trip tests for the non-layout components migrated in Phase 3a.

use animatix_text::transcript::SpeechTranscript;

use crate::timeline::AnimationTrack;
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
    timeline
        .world
        .spawn_replacing("card", AnimationTrack::new("card".to_string(), "Ellipse"));
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
    timeline
        .world
        .spawn_replacing("row", AnimationTrack::new("row".to_string(), "Row"));
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
    timeline
        .world
        .spawn_replacing("row", AnimationTrack::new("row".to_string(), "Row"));
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

    // Layout admission maps ids to labels and drops unadmitted children.
    timeline.entity_mut("b").expect("b must exist").track_mut().geometry.layout_size =
        Some(PropertyTrack::new([10.0, 10.0]));
    let admitted = timeline.container_layout("row").unwrap().layout_children(&timeline.world);
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].label, "b");

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
    timeline
        .world
        .spawn_replacing("scope", AnimationTrack::new("scope".to_string(), "Filter"));
    assert!(timeline.entity("scope").expect("scope").effects().is_none());
}
