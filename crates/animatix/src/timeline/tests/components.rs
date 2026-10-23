//! Round-trip tests for the non-layout components migrated in Phase 3a.

use animatix_text::transcript::SpeechTranscript;

use crate::timeline::AnimationTrack;
use crate::timeline::Timeline;

fn sample_transcript() -> SpeechTranscript {
    SpeechTranscript {
        duration: 2.0,
        markers: std::collections::HashMap::from([("intro".to_string(), 0.25)]),
        segments: Vec::new(),
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
