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
    let mut errors = Vec::new();
    let expanded = program.expand_components(&mut errors);
    assert!(errors.is_empty(), "{errors:?}");
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
    let expanded = program.expand_components(&mut Vec::new());
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
