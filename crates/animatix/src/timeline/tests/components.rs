//! Round-trip tests for the non-layout components migrated in Phase 3a.

use animatix_text::transcript::SpeechTranscript;

use crate::timeline::AnimationTrack;
use crate::timeline::Timeline;
use crate::timeline::{ContainerMetadata, LayoutType};

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
        child_order: vec!["a".to_string(), "b".to_string()],
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
    timeline.spawn_entity("a", "Rect");

    // Spawn: no optional container-layout component yet.
    let entity = timeline.entity("row").expect("row must exist");
    assert!(entity.container_layout().is_none());
    assert!(timeline.container_layout("row").is_none());
    assert_eq!(timeline.container_layouts().count(), 0);
    assert!(timeline.container_layout_ids().is_empty());

    // Insert: the Timeline setter is the only write entry point.
    let mut metadata = sample_container_layout();
    metadata.child_order = vec!["a".to_string()];
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
