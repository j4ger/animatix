use super::*;
use crate::easing::Easing;
use crate::timeline::animation_track::GeometryTracks;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{
    CapturedEnv, ContainerMetadata, CoordinateFrameComponent, EntityId, LayoutType, Timeline,
};

fn make_timeline_with_persistent_actors() -> Timeline {
    let mut timeline = Timeline::new();

    // Create a simple Rect actor
    let rect_id = timeline.world.spawn("rect1", "Rect");
    timeline
        .world
        .style_mut(rect_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);
    timeline
        .world
        .style_mut(rect_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(1000, 0.5, Easing::Linear);

    // Create a child actor
    let child_id = timeline.world.spawn("child1", "Rect");
    timeline.world.geometry_mut(child_id).position.ensure([0.0, 0.0]).add_keyframe(
        0,
        [0.0, 0.0],
        Easing::Linear,
    );
    timeline.world.geometry_mut(child_id).position.ensure([0.0, 0.0]).add_keyframe(
        500,
        [100.0, 50.0],
        Easing::Linear,
    );

    // Set up parent-child relationship
    timeline.world.link(rect_id, child_id);

    // Mark rect1 as persistent
    timeline.set_persistence_flag("rect1", true);

    timeline
}

#[test]
fn snapshot_collapses_opacity_to_single_keyframe() {
    let mut style = crate::timeline::animation_track::StyleTracks::default();
    style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
    style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

    let snapshot = snapshot_style_at(&style, 500);
    let opacity_track = snapshot.opacity.unwrap();
    assert_eq!(opacity_track.keyframes.len(), 1);
    assert_eq!(opacity_track.keyframes.get(&0).unwrap().0, 0.75); // interpolated at 500ms
}

#[test]
fn snapshot_collapses_position_to_single_keyframe() {
    let mut geometry = GeometryTracks::default();
    geometry.position.ensure([0.0, 0.0]).add_keyframe(0, [0.0, 0.0], Easing::Linear);
    geometry
        .position
        .ensure([0.0, 0.0])
        .add_keyframe(1000, [200.0, 100.0], Easing::Linear);

    let snapshot = snapshot_geometry_at(&geometry, 500);
    let pos_track = snapshot.position.unwrap();
    assert_eq!(pos_track.keyframes.len(), 1);
    assert_eq!(pos_track.keyframes.get(&0).unwrap().0, [100.0, 50.0]); // interpolated at 500ms
}

#[test]
fn snapshot_preserves_kind() {
    // Identity lives in World (label -> actor_type); a carried entry records
    // the type explicitly so injection can re-install it.
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("test", "Rect");
    assert_eq!(timeline.world.actor_type(id), "Rect");
}

#[test]
fn snapshot_preserves_svg_paths() {
    use crate::timeline::VelloPath;
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("test", "Rect");
    timeline.world.ensure_svg_payload(id).svg_paths.push(VelloPath::default());
    let payload = timeline.world.payload(id).cloned().unwrap_or_default();
    let snapshot = snapshot_payload_at(&payload, 0);
    assert_eq!(snapshot.as_svg().expect("svg payload").svg_paths.len(), 1);
}

#[test]
fn snapshot_clears_func_transitions() {
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("test", "Rect");
    timeline
        .world
        .func_transitions_mut(id)
        .push(crate::timeline::plot::FuncTransition {
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
    let payload = timeline.world.payload(id).cloned().unwrap_or_default();
    let snapshot = snapshot_payload_at(&payload, 0);
    assert!(snapshot.as_plot().expect("plot payload").func_transitions.is_empty());
}

#[test]
fn snapshot_preserves_procedural_plot() {
    use crate::ast::Expr;
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("test", "Rect");
    timeline.world.ensure_plot_payload(id).procedural_plot =
        Some(crate::timeline::plot::ProceduralPlot {
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
    let payload = timeline.world.payload(id).cloned().unwrap_or_default();
    let snapshot = snapshot_payload_at(&payload, 0);
    assert!(snapshot.as_plot().and_then(|plot| plot.procedural_plot.as_ref()).is_some());
}

#[test]
fn snapshot_empty_track_has_no_keyframes() {
    let style = crate::timeline::animation_track::StyleTracks::default();
    let snapshot = snapshot_style_at(&style, 0);
    let geometry = GeometryTracks::default();
    let geometry_snapshot = snapshot_geometry_at(&geometry, 0);
    // No property tracks should have keyframes
    assert!(snapshot.opacity.is_none() || snapshot.opacity.unwrap().keyframes.is_empty());
    assert!(
        geometry_snapshot.position.is_none()
            || geometry_snapshot.position.unwrap().keyframes.is_empty()
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
    let opacity = entry.style.opacity.as_ref().expect("snapshot should have opacity track");
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

    let rect1_id = timeline.world.spawn("rect1", "Rect");
    timeline
        .world
        .style_mut(rect1_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);

    let rect2_id = timeline.world.spawn("rect2", "Rect");
    timeline
        .world
        .style_mut(rect2_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);

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

    let actor_id = timeline.world.spawn("circle", "Rect");
    timeline
        .world
        .style_mut(actor_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);

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
    let actor_id = source.world.spawn("dot", "Rect");
    source
        .world
        .style_mut(actor_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);
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
    let actor_id = source.world.spawn("narrator", "Rect");
    source
        .world
        .style_mut(actor_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);
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
    let row_id = source.world.spawn("row", "Row");
    source
        .world
        .style_mut(row_id)
        .opacity
        .ensure(1.0)
        .add_keyframe(0, 1.0, Easing::Linear);
    source.set_persistence_flag("row", true);
    let source_a = source.spawn_entity("a", "Rect");
    let source_b = source.spawn_entity("b", "Rect");
    source.world.link(row_id, source_a);
    source.world.link(row_id, source_b);

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [12.0, 6.0],
        padding: [4.0, 2.0, 4.0, 2.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![source_a, source_b],
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

    // The destination creates the same labels in the opposite order, so the
    // snapshot's source ids cannot coincide with the destination's.
    let mut dest = Timeline::new();
    let dest_b = dest.spawn_entity("b", "Rect");
    let dest_a = dest.spawn_entity("a", "Rect");
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let mut expected = metadata.clone();
    expected.child_order = vec![dest_a, dest_b];
    assert_eq!(
        dest.container_layout("row"),
        Some(&expected),
        "the injected entity must carry the remapped container layout component"
    );
    // Still a real component on the destination entity, not a side table.
    let row = dest.entity("row").expect("row must exist after injection");
    assert_eq!(row.container_layout(), Some(&expected));

    // The field is new payload state: it must survive a serde carry-bag trip
    // (still holding the source ids — injection is where they are remapped)
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

#[test]
fn carry_bag_preserves_coordinate_frame() {
    let mut source = Timeline::new();
    source.world.spawn("g", "Graph");
    let frame = CoordinateFrameComponent {
        x_domain: [0.0, 4.0],
        y_domain: [0.0, 2.0],
        padding: [4.0, 4.0, 2.0, 2.0],
        ..CoordinateFrameComponent::default()
    };
    assert!(source.set_coordinate_frame("g", frame.clone()));
    source.set_persistence_flag("g", true);

    let bag = source.compute_carry_bag(0, true);
    assert_eq!(
        bag.entries.get("g").and_then(|entry| entry.coordinate_frame.clone()),
        Some(frame.clone()),
        "the carry bag must snapshot the plot host's coordinate frame"
    );

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());
    assert_eq!(
        dest.coordinate_frame_for("g"),
        Some(&frame),
        "the injected entity must carry the coordinate frame component"
    );

    // New payload state must survive a serde carry-bag trip; older bags must
    // default it to `None`.
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&bag).expect("CarryBag should serialize");
        let round_tripped: CarryBag =
            serde_json::from_str(&json).expect("CarryBag should deserialize");
        assert_eq!(
            round_tripped.entries["g"].coordinate_frame,
            Some(frame),
            "coordinate frame must survive the carry-bag serde round-trip"
        );
    }
}

#[test]
fn carry_remaps_container_child_order_ids() {
    let mut source = Timeline::new();
    let row = source.spawn_entity("row", "Row");
    let source_a = source.spawn_entity("a", "Rect");
    let source_b = source.spawn_entity("b", "Rect");
    source.world.link(row, source_a);
    source.world.link(row, source_b);
    source.set_persistence_flag("row", true);
    assert!(source.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [0.0, 0.0],
            padding: [0.0; 4],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            // `EntityId(9999)` is stale: no carried child owns it, so remapping
            // must drop it instead of keeping a source-space id.
            child_order: vec![source_b, source_a, EntityId(9999)],
        }
    ));

    let bag = source.compute_carry_bag(0, true);

    let mut dest = Timeline::new();
    let dest_b = dest.spawn_entity("b", "Rect");
    let dest_a = dest.spawn_entity("a", "Rect");
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let order = &dest.container_layout("row").expect("row layout").child_order;
    assert_eq!(
        order,
        &vec![dest_b, dest_a],
        "source ids must remap to destination entities and missing children drop"
    );
    assert_ne!(order, &vec![source_b, source_a], "source-scene ids must not leak through");
    assert!(!order.contains(&EntityId(9999)), "a stale source id must be dropped");
}

// -----------------------------------------------------------------------
// Special actor kind carry (Svg, Image)
// -----------------------------------------------------------------------

#[test]
fn snapshot_preserves_svg_actor_kind() {
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("icon", "Svg");
    timeline
        .world
        .ensure_svg_payload(id)
        .svg_paths
        .push(crate::timeline::VelloPath::default());

    assert_eq!(timeline.world.actor_type(id), "Svg");

    let payload = timeline.world.payload(id).cloned().unwrap_or_default();
    let payload_snapshot = snapshot_payload_at(&payload, 0);
    assert_eq!(
        payload_snapshot.as_svg().expect("svg payload").svg_paths.len(),
        1,
        "svg_paths must survive snapshot"
    );
}

#[test]
fn snapshot_preserves_image_actor_kind() {
    let mut timeline = Timeline::new();
    let id = timeline.world.spawn("pic", "Image");
    assert_eq!(timeline.world.actor_type(id), "Image");
}

#[test]
fn inject_carry_bag_preserves_svg_paths() {
    use crate::timeline::VelloPath;

    let mut source = Timeline::new();
    let icon_id = source.world.spawn("icon", "Svg");
    source.world.ensure_svg_payload(icon_id).svg_paths.push(VelloPath::default());
    source.world.ensure_svg_payload(icon_id).svg_paths.push(VelloPath::default());
    source.set_persistence_flag("icon", true);

    let bag = source.compute_carry_bag(0, true);

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let icon = dest.world.get_id("icon").expect("icon must be carried");
    assert_eq!(dest.world.actor_type(icon), "Svg");
    assert_eq!(dest.world.svg_paths(icon).len(), 2, "svg_paths must survive carry");
}

#[test]
fn inject_carry_bag_preserves_procedural_plot() {
    use crate::ast::Expr;
    use crate::timeline::plot::{PlotCurveKind, ProceduralPlot};

    let mut source = Timeline::new();
    let curve_id = source.world.spawn("curve", "PlotCurve");
    source.world.ensure_plot_payload(curve_id).procedural_plot = Some(ProceduralPlot {
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
    source.set_persistence_flag("curve", true);

    let bag = source.compute_carry_bag(0, true);
    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());

    let carried = dest.world.get_id("curve").expect("curve must be carried");
    assert_eq!(dest.world.actor_type(carried), "PlotCurve");
    assert!(
        dest.world.procedural_plot(carried).is_some(),
        "procedural_plot must survive carry"
    );
}

#[test]
fn compute_carry_bag_propagates_child_persistence_flag() {
    let mut timeline = Timeline::new();

    let parent_id = timeline.world.spawn("parent", "Rect");

    let child_id = timeline.world.spawn("child", "Rect");
    timeline.world.geometry_mut(child_id).position.ensure([0.0, 0.0]).add_keyframe(
        0,
        [10.0, 20.0],
        Easing::Linear,
    );
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
    let opacity = entry.style.opacity.as_ref().expect("opacity track must survive round-trip");
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

#[test]
fn carry_bag_preserves_primitive_payload() {
    let mut source = Timeline::new();
    let id = source.world.spawn("asset", "Svg");
    source.world.ensure_svg_payload(id);
    source.set_persistence_flag("asset", true);

    let bag = source.compute_carry_bag(0, true);
    assert!(
        bag.entries.get("asset").and_then(|entry| entry.payload.as_svg()).is_some(),
        "the carry bag must snapshot the primitive payload"
    );

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());
    let dest_id = dest.world.get_id("asset").expect("carried entity");
    assert!(
        dest.world.svg_payload(dest_id).is_some(),
        "injection must restore the payload component"
    );
}

// The tagged tier rides the carry bag like every other authored tier: without
// it, a persistent actor using tagged properties (`legend`, extension tags)
// would silently lose them across a scene change.
#[test]
fn carry_bag_roundtrips_tagged_tracks() {
    use crate::timeline::{PropertyTrack, PropertyValue};

    let mut source = Timeline::new();
    let id = source.world.spawn("card", "Rect");
    let mut track = PropertyTrack::new(PropertyValue::F32(0.0));
    track.add_keyframe(0, PropertyValue::F32(1.0), Easing::Linear);
    track.add_keyframe(1000, PropertyValue::F32(5.0), Easing::Linear);
    source
        .world_mut()
        .tagged_tracks_mut(id)
        .insert("legend".to_string(), Some(track));
    source.set_persistence_flag("card", true);

    let bag = source.compute_carry_bag(500, true);
    let entry = bag.entries.get("card").expect("carried");
    let carried = entry
        .tagged
        .get("legend")
        .and_then(|track| track.as_ref())
        .expect("tagged entry carried");
    assert_eq!(carried.keyframes.len(), 1, "collapsed to a single keyframe");
    assert_eq!(carried.keyframes.get(&0).expect("collapsed at t=0").0, PropertyValue::F32(3.0));

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());
    let restored = dest.world.get_id("card").expect("injected");
    let track = dest
        .world
        .tagged_tracks(restored)
        .get("legend")
        .and_then(|track| track.as_ref())
        .expect("tagged restored");
    assert_eq!(track.keyframes.get(&0).expect("collapsed at t=0").0, PropertyValue::F32(3.0));
}

#[cfg(feature = "serde")]
#[test]
fn legacy_carry_bag_without_tagged_key_deserializes() {
    let mut source = Timeline::new();
    source.world.spawn("card", "Rect");
    source.set_persistence_flag("card", true);
    let bag = source.compute_carry_bag(0, true);

    // Strip the (empty) tagged member to imitate a bag serialized before the
    // tier existed; `serde(default)` must fill it back in.
    let json = serde_json::to_string(&bag).expect("serialize");
    let stripped = json.replace(",\"tagged\":{}", "");
    assert_ne!(stripped, json, "the serialized bag mentions the tagged tier");
    let restored: CarryBag = serde_json::from_str(&stripped).expect("legacy bag deserializes");
    assert!(
        restored.entries.get("card").expect("entry").tagged.is_empty(),
        "missing key falls back to the default tier"
    );
}

#[test]
fn carry_bag_carries_caps_meta_and_child_order() {
    use crate::timeline::{ContainerMetadata, LayoutType, PropertyTrack};

    let mut source = Timeline::new();
    let row = source.world.spawn("row", "Row");
    let a = source.world.spawn("a", "Rect");
    let b = source.world.spawn("b", "Rect");
    source.world.link(row, a);
    source.world.link(row, b);
    for child in [a, b] {
        source.world.geometry_mut(child).layout_size = Some(PropertyTrack::new([10.0, 10.0]));
    }
    source.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [0.0; 2],
            padding: [0.0; 4],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: vec![a, b],
        },
    );
    // A swap that completed at t=500: the end state lives on the track only.
    let mut track = PropertyTrack::new(vec![a, b]);
    track.add_keyframe(500, vec![b, a], Easing::Linear);
    assert!(source.set_child_order_track("row", track));
    source.world_mut().set_caps(
        row,
        animatix_core::caps::ActorCaps {
            time_dependent: true,
            ..Default::default()
        },
    );
    source.world_mut().meta_mut(row).parallax = 0.5;
    source.world_mut().meta_mut(row).camera_follow = false;
    source.set_persistence_flag("row", true);

    let bag = source.compute_carry_bag(1000, true);
    let entry = bag.entries.get("row").expect("carried");
    let carried = entry.child_order.as_ref().expect("child-order track carried");
    assert_eq!(carried.default_order, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(
        carried.keyframes.last().map(|(_, order, _)| order.clone()),
        Some(vec!["b".to_string(), "a".to_string()]),
        "the completed swap's end state rides the bag"
    );
    assert_eq!(entry.meta.map(|meta| meta.parallax), Some(0.5));

    let mut dest = Timeline::new();
    dest.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());
    let row2 = dest.get_entity_id("row").expect("injected");
    let a2 = dest.get_entity_id("a").expect("child a injected");
    let b2 = dest.get_entity_id("b").expect("child b injected");
    assert!(
        dest.world.caps(row2).time_dependent,
        "the caps override survives (set_identity would otherwise re-derive default caps)"
    );
    assert_eq!(dest.world.meta(row2).parallax, 0.5);
    assert!(!dest.world.meta(row2).camera_follow);
    assert_eq!(
        dest.get_child_order_ids("row", 1000),
        vec![b2, a2],
        "the completed swap's end state is remapped to destination ids"
    );
}
