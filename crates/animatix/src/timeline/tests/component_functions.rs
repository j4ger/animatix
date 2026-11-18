//! Component methods retain lexical scope through plans and scene builds.

use std::path::Path;

use animatix_syntax::module::ModuleGraph;

use crate::composition::BuildTarget;
use crate::timeline::{BuildOptions, Timeline, TrackAccessor};

fn build(source: &str) -> BuildTarget {
    let mut graph = ModuleGraph::new();
    let mut program = graph
        .load_program_with_source(Path::new("virtual:///component_functions.amx"), Some(source))
        .unwrap();
    let diagnostics = program.typecheck();
    assert!(diagnostics.iter().all(|d| !d.is_error()), "{diagnostics:?}");
    let expanded = program.expand_components().expect("component expansion");
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &program.namespaces,
        None,
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    report.output
}

fn root_widths(timeline: &Timeline) -> Vec<f32> {
    let mut widths = timeline
        .world
        .root_entities()
        .iter()
        .map(|id| timeline.world.geometry(*id).size.get(0, [0.0; 2])[0] * 2.0)
        .collect::<Vec<_>>();
    widths.sort_by(f32::total_cmp);
    widths
}

const CARD: &str = r#"
component Card(width: Num) {
  box: Rect, size: (width, 20), opacity: 1
  fn scaled(factor: Num) { width * factor }
  pub fn resize(factor: Num = 2) { #{ self.size = (scaled(factor), 20) } }
}
"#;

#[test]
fn independent_plan_sites_keep_same_named_component_methods_private() {
    let source = format!(
        r#"{CARD}
let first = #{{ local: Card, width: 10
  local.resize()
}}
let second = #{{ local: Card, width: 20
  local.resize(factor: 3)
}}
#0s
first
second
"#
    );
    let BuildTarget::SingleScene(timeline) = build(&source) else {
        panic!("single scene")
    };
    assert_eq!(root_widths(&timeline), vec![20.0, 60.0]);
    assert!(timeline.world.get_id("local").is_none());
}

#[test]
fn returned_plans_expand_components_and_bind_methods_on_each_mount() {
    let source = format!(
        r#"{CARD}
fn card(authored_width: Num) {{ #{{ local: Card, width: authored_width
  local.resize()
}} }}
#0s
card(15)
card(25)
"#
    );
    let BuildTarget::SingleScene(timeline) = build(&source) else {
        panic!("single scene")
    };
    assert_eq!(root_widths(&timeline), vec![30.0, 50.0]);
}

#[test]
fn scenes_can_reuse_component_labels_with_distinct_method_bindings() {
    let source = format!(
        r#"{CARD}
# One
#0s
card: Card, width: 12
card.resize()
#1s
play Two
# Two
#0s
card: Card, width: 34
card.resize(factor: 3)
"#
    );
    let BuildTarget::MultiScene(composition) = build(&source) else {
        panic!("composition")
    };
    assert_eq!(root_widths(&composition.scenes["One"].timeline), vec![24.0]);
    assert_eq!(root_widths(&composition.scenes["Two"].timeline), vec![102.0]);
}

#[test]
fn imported_scenes_expand_private_component_imports_in_their_defining_modules() {
    use animatix_syntax::module::SourceAccess;
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        Path::new("/project/ui.amx").to_path_buf(),
        r#"
let secret = 5
fn base() { secret }
pub component Card(width: Num = 10) {
  box: Rect, size: (width, 20), opacity: 1
  pub fn resize() { #{ self.size = (width + base(), 20) } }
}
"#,
    );
    graph.upsert_source(
        Path::new("/project/scenes.amx").to_path_buf(),
        r#"
import "ui.amx" as widgets
# Remote
#0s
card: widgets::Card, width: 20
card.resize()
#1s
"#,
    );
    let program = graph
        .load_program_with_source(
            Path::new("/project/main.amx"),
            Some(
                r#"
import "scenes.amx" as remote
# Intro
#0s
title: Rect, size: (10, 20), opacity: 1
#1s
play remote::Remote
"#,
            ),
        )
        .unwrap();
    let expanded = program.expand_components().expect("component expansion");
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &program.namespaces,
        None,
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let BuildTarget::MultiScene(composition) = report.output else {
        panic!("composition")
    };
    assert_eq!(root_widths(&composition.scenes["remote::Remote"].timeline), vec![25.0]);
}

#[test]
fn same_named_inline_children_keep_instance_identity_and_animation_isolated() {
    // Regression for the old component label collector: it collected `body`
    // but not its inline `arm`, so both instances shared the bare `arm` track.
    let BuildTarget::SingleScene(timeline) = build(
        r#"
component Switch(x: Num) {
  body: Group, at: (x, 100), opacity: 1 {
    arm: Line, from: (0, 0), to: (40, 50), opacity: 1
  }
}
a: Switch, x: 100
b: Switch, x: 300
#1s
a.arm.to = (0, 50) [1s]
#3s
b.arm.to = (20, 50) [1s]
"#,
    ) else {
        panic!("single scene")
    };
    let a = timeline.get_entity_id("a.arm").expect("a owns its inline arm");
    let b = timeline.get_entity_id("b.arm").expect("b owns its inline arm");
    assert_ne!(a, b, "same-named children must be distinct entities");
    assert!(timeline.get_entity_id("arm").is_none(), "no unqualified shared child");
    assert_eq!(timeline.world.resolve_path(&["a", "arm"]).as_deref(), Some("a.arm"));
    assert_eq!(timeline.world.resolve_path(&["b", "arm"]).as_deref(), Some("b.arm"));

    let endpoint =
        |entity, time_ms| timeline.world.shape_or_default(entity).line_to.get(time_ms, [0.0; 2]);
    assert_eq!(endpoint(a, 0), [40.0, 50.0]);
    assert_eq!(endpoint(b, 0), [40.0, 50.0]);
    let a_mid = endpoint(a, 1500);
    assert!(a_mid[0] > 0.0 && a_mid[0] < 40.0, "a must move continuously: {a_mid:?}");
    assert_eq!(a_mid[1], 50.0);
    for time_ms in [1000, 1500, 2000, 2500] {
        assert_eq!(endpoint(b, time_ms), [40.0, 50.0], "a must not animate b at {time_ms}");
    }
    let b_mid = endpoint(b, 3500);
    assert!(b_mid[0] > 20.0 && b_mid[0] < 40.0, "b must move continuously: {b_mid:?}");
    assert_eq!(b_mid[1], 50.0);
    for time_ms in [2000, 3000, 3500, 4000] {
        assert_eq!(endpoint(a, time_ms), [0.0, 50.0], "b must not animate a at {time_ms}");
    }
    assert_eq!(endpoint(b, 4000), [20.0, 50.0]);
}
