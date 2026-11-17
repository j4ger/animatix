//! Regression coverage for source-ordered scene layers and nested-anchor follow.

use std::path::PathBuf;

use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_syntax::module::ModuleGraph;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1280,
    height: 720,
};

fn build_example() -> Timeline {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/layout/painter_order_annotations.amx");
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(&path).expect("example should parse");
    let type_diagnostics = program.typecheck();
    assert!(type_diagnostics.is_empty(), "type diagnostics: {type_diagnostics:?}");

    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        animatix::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);
    report.output
}

fn frame(timeline: &Timeline, time_s: f64) -> animatix::timeline::scene_program::SceneProgram {
    let mut backend = None;
    timeline.evaluate_program_with_debug(
        time_s,
        DIMS,
        DebugRenderOptions {
            compute_hit_regions: true,
            ..DebugRenderOptions::default()
        },
        &mut backend,
    )
}

fn item_index(program: &animatix::timeline::scene_program::SceneProgram, label: &str) -> usize {
    program.items.iter().position(|item| item.label == label).unwrap_or_else(|| {
        panic!(
            "missing scene item {label:?}; observed {:?}",
            program.items.iter().map(|item| &item.label).collect::<Vec<_>>()
        )
    })
}

#[test]
fn background_connector_precedes_nodes_and_root_callout_tracks_nested_marker() {
    let timeline = build_example();
    let before = frame(&timeline, 0.5);
    let after = frame(&timeline, 2.5);

    // A Group paints its complete child subtree before the next root. The
    // connector stays behind the component instances; the root-level callout
    // remains on top even though its target is nested inside `moving`.
    assert!(item_index(&before, "edge") < item_index(&before, "source.body"));
    assert!(item_index(&before, "source.body") < item_index(&before, "moving.body"));
    assert!(item_index(&before, "moving.marker") < item_index(&before, "note"));

    let before_marker = before.precise_bounds["moving.marker"].center().x;
    let after_marker = after.precise_bounds["moving.marker"].center().x;
    assert!(
        (after_marker - before_marker - 200.0).abs() < 1.0,
        "component child should inherit the instance's 200px move: {before_marker} -> {after_marker}"
    );

    // The connector and callout are both resolved against the current world
    // bounds, so their observed ink bounds move with the nested target.
    let before_edge = before.precise_bounds["edge"];
    let after_edge = after.precise_bounds["edge"];
    assert!(
        after_edge.x1 > before_edge.x1 + 150.0,
        "background connector endpoint should follow the moved marker: {before_edge:?} -> {after_edge:?}"
    );
    let before_note = before.precise_bounds["note"];
    let after_note = after.precise_bounds["note"];
    assert!(
        (after_note.x1 - before_note.x1).abs() > 100.0,
        "root callout ink should follow the moved marker: {before_note:?} -> {after_note:?}"
    );

    // Picking consumes the same paint order, with the last rendered actor
    // listed first by the GUI's reverse hit-region traversal.
    let hit_labels = timeline.hit_regions();
    let note_hit = hit_labels.iter().position(|(label, _)| label == "note").unwrap();
    let marker_hit = hit_labels.iter().position(|(label, _)| label == "moving.marker").unwrap();
    assert!(note_hit > marker_hit, "hit regions should preserve painter order");
}

#[test]
fn completed_move_holds_at_the_resting_frame() {
    let timeline = build_example();
    let settled = frame(&timeline, 2.5);
    let resting = frame(&timeline, 3.8);

    assert_eq!(
        settled.precise_bounds["moving.marker"], resting.precise_bounds["moving.marker"],
        "the node should remain at its completed position for the outro hold"
    );
    assert_eq!(settled.precise_bounds["note"], resting.precise_bounds["note"]);
    assert_eq!(settled.precise_bounds["edge"], resting.precise_bounds["edge"]);
}
