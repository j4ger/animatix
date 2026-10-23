use super::*;
use crate::easing::Easing;
use crate::timeline::dispatch::AnimationTrack;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{CapturedEnv, ContainerMetadata, LayoutType, Timeline};

fn make_timeline_with_persistent_actors() -> Timeline {
    let mut timeline = Timeline::new();

    // Create a simple Rect actor
    let mut rect = AnimationTrack::placeholder("rect1".to_string());
    rect.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    rect.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
    let rect_id = timeline.world.spawn_with_track("rect1", rect);

    // Create a child actor
    let mut child = AnimationTrack::placeholder("child1".to_string());
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
    let child_id = timeline.world.spawn_with_track("child1", child);

    // Set up parent-child relationship
    timeline.world.link(rect_id, child_id);

    // Mark rect1 as persistent
    timeline.set_persistence_flag("rect1", true);

    timeline
}

// -----------------------------------------------------------------------
// snapshot_track_at
// -----------------------------------------------------------------------

#[test]
fn snapshot_collapses_opacity_to_single_keyframe() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    track.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

    let snapshot = snapshot_track_at(&track, 500);
    let opacity_track = snapshot.style.opacity.unwrap();
    assert_eq!(opacity_track.keyframes.len(), 1);
    assert_eq!(opacity_track.keyframes.get(&0).unwrap().0, 0.75); // interpolated at 500ms
}

#[test]
fn snapshot_collapses_position_to_single_keyframe() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(0, [0.0, 0.0], Easing::Linear);
    track
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(1000, [200.0, 100.0], Easing::Linear);

    let snapshot = snapshot_track_at(&track, 500);
    let pos_track = snapshot.geometry.position.unwrap();
    assert_eq!(pos_track.keyframes.len(), 1);
    assert_eq!(pos_track.keyframes.get(&0).unwrap().0, [100.0, 50.0]); // interpolated at 500ms
}

#[test]
fn snapshot_preserves_kind() {
    let track = AnimationTrack::placeholder("test".to_string());
    let snapshot = snapshot_track_at(&track, 0);
    assert_eq!(snapshot.actor_type, track.actor_type);
}

#[test]
fn snapshot_sets_first_seen_ms_to_zero() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.first_seen_ms = 500;
    let snapshot = snapshot_track_at(&track, 500);
    assert_eq!(snapshot.first_seen_ms, 0);
}

#[test]
fn snapshot_preserves_svg_paths() {
    use crate::timeline::VelloPath;
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.ensure_svg_payload().svg_paths.push(VelloPath::default());
    let snapshot = snapshot_track_at(&track, 0);
    assert_eq!(snapshot.svg_paths().len(), 1);
}

#[test]
fn snapshot_clears_func_transitions() {
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.func_transitions_mut().push(crate::timeline::plot::FuncTransition {
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
    assert!(snapshot.func_transitions().is_empty());
}

#[test]
fn snapshot_preserves_procedural_plot() {
    use crate::ast::Expr;
    let mut track = AnimationTrack::placeholder("test".to_string());
    track.ensure_plot_payload().procedural_plot = Some(crate::timeline::plot::ProceduralPlot {
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
    assert!(snapshot.procedural_plot().is_some());
}

#[test]
fn snapshot_empty_track_has_no_keyframes() {
    let track = AnimationTrack::placeholder("test".to_string());
    let snapshot = snapshot_track_at(&track, 0);
    // No property tracks should have keyframes
    assert!(
        snapshot.style.opacity.is_none() || snapshot.style.opacity.unwrap().keyframes.is_empty()
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
    let opacity = entry.track.style.opacity.as_ref().expect("snapshot should have opacity track");
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

    let mut rect1 = AnimationTrack::placeholder("rect1".to_string());
    rect1.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    timeline.world.spawn_with_track("rect1", rect1);

    let mut rect2 = AnimationTrack::placeholder("rect2".to_string());
    rect2.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    timeline.world.spawn_with_track("rect2", rect2);

    // Only rect2 is persistent
    timeline.set_persistence_flag("rect2", true);

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

    let mut actor = AnimationTrack::placeholder("circle".to_string());
    actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    timeline.world.spawn_with_track("circle", actor);

    // Simulate the color: auto assignment — slot 0 was assigned.
    timeline.set_auto_color_slot("circle", 0);
    timeline.next_auto_color_index = 1;

    timeline.set_persistence_flag("circle", true);

    let bag = timeline.compute_carry_bag(0, true);
    let entry = bag.entries.get("circle").expect("circle should be in bag");
    assert_eq!(entry.auto_color_slot, Some(0), "auto_color_slot must be carried");
}

#[test]
fn inject_carry_bag_seeds_auto_color_assignments() {
    let mut source = Timeline::new();
    let mut actor = AnimationTrack::placeholder("dot".to_string());
    actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    source.world.spawn_with_track("dot", actor);
    source.set_auto_color_slot("dot", 2);
    source.next_auto_color_index = 3;
    source.set_persistence_flag("dot", true);

    let bag = source.compute_carry_bag(0, true);

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    assert_eq!(
        dest.auto_color_slot("dot"),
        Some(2),
        "injected auto_color_slot must appear in dest's auto_color store"
    );
    // next_auto_color_index must be bumped to avoid collision
    assert!(
        dest.next_auto_color_index >= 3,
        "next_auto_color_index must be at least slot+1 to avoid collision"
    );
}

#[test]
fn carry_bag_preserves_auto_color_and_transcript() {
    let mut source = Timeline::new();
    let mut actor = AnimationTrack::placeholder("narrator".to_string());
    actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    source.world.spawn_with_track("narrator", actor);
    source.set_auto_color_slot("narrator", 2);
    source.next_auto_color_index = 3;
    source.set_persistence_flag("narrator", true);

    let transcript = SpeechTranscript {
        duration: 1.5,
        markers: std::collections::HashMap::from([("intro".to_string(), 0.5)]),
        segments: Vec::new(),
    };
    assert!(
        source.set_transcript("narrator", transcript.clone()),
        "transcript must attach to the narrator entity"
    );

    let bag = source.compute_carry_bag(0, true);
    let entry = bag.entries.get("narrator").expect("narrator should be carried");
    assert_eq!(entry.auto_color_slot, Some(2));
    assert_eq!(entry.transcript, Some(transcript.clone()));

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    assert_eq!(dest.auto_color_slot("narrator"), Some(2), "slot must carry");
    assert_eq!(dest.transcript("narrator"), Some(&transcript), "transcript must carry");
    assert_eq!(dest.transcript("narrator").unwrap().duration, 1.5);
    assert_eq!(dest.next_auto_color_index, 3, "next slot must stay collision-free");
}

#[test]
fn carry_bag_preserves_container_layout() {
    let mut source = Timeline::new();
    let mut actor = AnimationTrack::placeholder("row".to_string());
    actor.set_identity("Row");
    actor.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    source.world.spawn_with_track("row", actor);
    source.set_persistence_flag("row", true);

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [12.0, 6.0],
        padding: [4.0, 2.0, 4.0, 2.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".to_string(), "b".to_string()],
    };
    assert!(
        source.set_container_layout("row", metadata.clone()),
        "container layout must attach to the container entity"
    );

    let bag = source.compute_carry_bag(0, true);
    let entry = bag.entries.get("row").expect("row should be carried");
    assert_eq!(
        entry.container_layout,
        Some(metadata.clone()),
        "the carry bag must snapshot the container layout component"
    );

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    assert_eq!(
        dest.container_layout("row"),
        Some(&metadata),
        "the injected entity must carry the container layout component"
    );
    // Still a real component on the destination entity, not a side table.
    let row = dest.entity("row").expect("row must exist after injection");
    assert_eq!(row.container_layout(), Some(&metadata));

    // The field is new payload state: it must survive a serde carry-bag trip
    // and older bags must default it to `None`.
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&bag).expect("CarryBag should serialize");
        let round_tripped: CarryBag =
            serde_json::from_str(&json).expect("CarryBag should deserialize");
        assert_eq!(
            round_tripped.entries["row"].container_layout,
            Some(metadata.clone()),
            "container layout must survive the carry-bag serde round-trip"
        );
    }
}

// -----------------------------------------------------------------------
// Special actor kind carry (Svg, Image)
// -----------------------------------------------------------------------

#[test]
fn snapshot_preserves_svg_actor_kind() {
    let mut track = AnimationTrack::placeholder("icon".to_string());
    track.set_identity("Svg");
    track.ensure_svg_payload().svg_paths.push(crate::timeline::VelloPath::default());

    let snapshot = snapshot_track_at(&track, 0);
    assert_eq!(snapshot.actor_type, "Svg");
    assert_eq!(snapshot.svg_paths().len(), 1, "svg_paths must survive snapshot");
}

#[test]
fn snapshot_preserves_image_actor_kind() {
    let mut track = AnimationTrack::placeholder("pic".to_string());
    track.set_identity("Image");

    let snapshot = snapshot_track_at(&track, 0);
    assert_eq!(snapshot.actor_type, "Image");
}

#[test]
fn inject_carry_bag_preserves_svg_paths() {
    use crate::timeline::VelloPath;

    let mut source = Timeline::new();
    let mut actor = AnimationTrack::placeholder("icon".to_string());
    actor.set_identity("Svg");
    actor.ensure_svg_payload().svg_paths.push(VelloPath::default());
    actor.ensure_svg_payload().svg_paths.push(VelloPath::default());
    source.world.spawn_with_track("icon", actor);
    source.set_persistence_flag("icon", true);

    let bag = source.compute_carry_bag(0, true);

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let icon = dest.world.tracks.get("icon").expect("icon must be carried");
    assert_eq!(icon.actor_type, "Svg");
    assert_eq!(icon.svg_paths().len(), 2, "svg_paths must survive carry");
}

#[test]
fn inject_carry_bag_preserves_procedural_plot() {
    use crate::ast::Expr;
    use crate::timeline::plot::{PlotCurveKind, ProceduralPlot};

    let mut source = Timeline::new();
    let mut actor = AnimationTrack::placeholder("curve".to_string());
    actor.set_identity("PlotCurve");
    actor.ensure_plot_payload().procedural_plot = Some(ProceduralPlot {
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
    source.world.spawn_with_track("curve", actor);
    source.set_persistence_flag("curve", true);

    let bag = source.compute_carry_bag(0, true);
    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let carried = dest.world.tracks.get("curve").expect("curve must be carried");
    assert_eq!(carried.actor_type, "PlotCurve");
    assert!(carried.procedural_plot().is_some(), "procedural_plot must survive carry");
}

#[test]
fn compute_carry_bag_propagates_child_persistence_flag() {
    let mut timeline = Timeline::new();

    let parent = AnimationTrack::placeholder("parent".to_string());
    let parent_id = timeline.world.spawn_with_track("parent", parent);

    let mut child = AnimationTrack::placeholder("child".to_string());
    child
        .geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(0, [10.0, 20.0], Easing::Linear);
    let child_id = timeline.world.spawn_with_track("child", child);
    timeline.world.link(parent_id, child_id);

    // Only parent is persistent; child should be carried because it's a descendant
    timeline.set_persistence_flag("parent", true);

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
