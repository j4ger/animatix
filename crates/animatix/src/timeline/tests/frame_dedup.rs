//! The content epoch a rendered frame is keyed on: it must move whenever the
//! document moves, and must not move when it is merely read.

use super::*;

const SCENE: &str = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
box: Rect, size: (200, 100), at: (320, 180), color: accent.primary
"#;

#[test]
fn reading_the_epoch_does_not_move_it() {
    let timeline = build_timeline(SCENE);
    assert_eq!(timeline.content_epoch(), timeline.content_epoch());
}

#[test]
fn mutable_access_to_a_track_moves_the_epoch() {
    let mut timeline = build_timeline(SCENE);
    let before = timeline.content_epoch();
    // Every public mutable accessor funnels through `invalidate_frame_cache`,
    // which is the single point the epoch is bumped from — so an edit made in
    // place cannot leave a frame dedup serving the pre-edit pixels.
    assert!(
        timeline.entity_mut("box").is_some(),
        "the fixture did not build a track named `box`"
    );
    assert_eq!(timeline.content_epoch(), before + 1, "a mutation funnel did not move the epoch");
}

#[test]
fn a_fresh_document_restarts_the_epoch_at_zero() {
    // Not a defect, a constraint: the epoch counts a document's *edits*, not its
    // content, so two different scenes both start at 0. That is why
    // `FrameSignature` carries a driver-side document generation beside it — a
    // driver that swaps documents without bumping that would dedup a frame it
    // no longer has.
    let first = build_timeline(SCENE);
    let second = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
dot: Rect, size: (40, 40), at: (320, 180), color: accent.warning
"#,
    );
    assert_eq!(first.content_epoch(), 0);
    assert_eq!(second.content_epoch(), 0);
}

#[test]
fn cloning_preserves_entity_data_and_resets_disposable_frame_caches() {
    let mut original = build_timeline(SCENE);
    let dimensions = SceneDimensions {
        width: 640,
        height: 360,
    };
    let id = original.get_entity_id("box").unwrap();
    original.entity_by_id_mut(id).unwrap();
    let _ = original.evaluate(0.0, dimensions);
    assert!(original.content_epoch() > 0);
    assert!(original.eval_caches.frame_cache.borrow().is_some());

    let mut cloned = original.clone();
    assert_eq!(cloned.get_entity_id("box"), Some(id));
    assert_eq!(cloned.world().hierarchy(id), original.world().hierarchy(id));
    assert_eq!(cloned.world().geometry(id).size.get(0, [0.0; 2]), [100.0, 50.0]);
    assert_eq!(
        cloned.world().style(id).color.get(0, [0.0; 4]),
        original.world().style(id).color.get(0, [0.0; 4])
    );
    assert_eq!(cloned.content_epoch(), 0);
    assert!(cloned.eval_caches.frame_cache.borrow().is_none());
    assert!(cloned.eval_caches.static_subtree_cache.borrow().is_empty());
    assert!(cloned.eval_caches.env_injection_plan.borrow().is_none());

    let _ = cloned.evaluate(0.0, dimensions);
    assert!(cloned.eval_caches.frame_cache.borrow().is_some());
    cloned.entity_by_id_mut(id).unwrap();
    assert!(cloned.eval_caches.frame_cache.borrow().is_none());
    assert!(original.eval_caches.frame_cache.borrow().is_some());
}

#[test]
fn nested_layout_reflow_invalidates_an_already_evaluated_frame() {
    let mut timeline = build_timeline(
        "#0s\ngrid: Grid, size: (300, auto), cols: 1 {\n\
         column: Col, size: fill { box: Rect, size: (40, 30) }\n}",
    );
    let dimensions = SceneDimensions {
        width: 640,
        height: 360,
    };
    let _ = timeline.evaluate(0.0, dimensions);
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());
    let epoch = timeline.content_epoch();
    timeline.reflow_nested_layouts("grid", 0, &mut Vec::new());
    assert!(timeline.content_epoch() > epoch);
    assert!(timeline.eval_caches.frame_cache.borrow().is_none());
    assert!(timeline.eval_caches.static_subtree_cache.borrow().is_empty());
    let _ = timeline.evaluate(0.0, dimensions);
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());
}
