//! Integration contracts for ordinary functions and inert timeline expressions.

use std::collections::HashMap;

use crate::diagnostics::BuildReport;
use crate::timeline::{SceneDimensions, Timeline, TrackAccessor, Value};

fn report(source: &str) -> BuildReport<Timeline> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}\n{source}");
    Timeline::build_with_diagnostics(&ast.expect("AST"), &HashMap::new())
}

fn clean(source: &str) -> Timeline {
    let report = report(source);
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    report.output
}

fn opacity(timeline: &Timeline, label: &str, ms: u64) -> f32 {
    let id = timeline.world.get_id(label).expect("actor");
    timeline.world.style(id).opacity.get(ms, 1.0)
}

#[test]
fn recursive_value_functions_report_a_depth_error_and_release_the_guard() {
    let result = report("fn recurse() -> Num { recurse() }\nlet result = recurse()");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("user function call depth exceeded 32")
        }),
        "{:?}",
        result.diagnostics
    );
    let subsequent = clean("fn answer() -> Num { 42 }\nlet result = answer()");
    assert_eq!(subsequent.env.get("result"), Some(Value::Num(42.0)));
}

#[test]
fn constructing_a_plan_does_not_mount_its_actors() {
    let timeline = clean("let unused = #{ local: Rect, size: (20, 20) }");
    assert!(timeline.world.root_entities().is_empty());
    assert!(matches!(timeline.env.get("unused"), Some(Value::TimelinePlan(_))));
}

#[test]
fn repeated_mounts_allocate_private_actors_without_time_entities() {
    let timeline = clean(
        r#"
fn badge() { #{ local: Rect, size: (20, 20), opacity: 1 } }
let clip = badge()
#1s
clip
#3s
clip
"#,
    );
    assert_eq!(timeline.world.root_entities().len(), 2);
    assert_eq!(timeline.world.tracks.len(), 2);
    assert!(
        timeline.world.get_id("local").is_none(),
        "plan locals cannot become scene labels"
    );
}

#[test]
fn plans_in_containers_use_the_real_spatial_parent() {
    let timeline = clean(
        r#"
fn badge() { #{ local: Rect, size: (20, 20), opacity: 1
  local.opacity = (local.opacity / 2) [500ms]
} }
#0s
row: Row, gap: 8 { badge() badge() }
"#,
    );
    let root = timeline.world.get_id("row").unwrap();
    assert_eq!(timeline.world.root_entities(), vec![root]);
    assert_eq!(timeline.world.child_labels(root).count(), 2);
    assert_eq!(timeline.world.tracks.len(), 3, "there is no implicit time wrapper entity");
    for child in timeline.world.child_labels(root) {
        assert_eq!(opacity(&timeline, child, 500), 0.5);
    }
}

#[test]
fn captured_actors_are_shared_while_plan_locals_are_private() {
    let timeline = clean(
        r#"
external: Rect, opacity: 1
let clip = #{
  local: Rect, opacity: 1
  external.opacity = 0.25 [500ms]
}
#1s
clip
#2s
clip
"#,
    );
    assert_eq!(timeline.world.tracks.len(), 3);
    assert_eq!(opacity(&timeline, "external", 1500), 0.25);
    assert_eq!(opacity(&timeline, "external", 2500), 0.25);
}

#[test]
fn sequence_waits_for_whole_children_and_preserves_parallel_bodies() {
    let timeline = clean(
        r#"
#0s
a: Rect, opacity: 1
b: Rect, opacity: 1
c: Rect, opacity: 1
let clip = #{
  a.opacity = 0.5 [1s]
  b.opacity = 0.5 [2s]
}
sequence {
  clip
  c.opacity = 0.25 [500ms]
}
"#,
    );
    assert_eq!(opacity(&timeline, "a", 1000), 0.5);
    assert_eq!(opacity(&timeline, "b", 2000), 0.5);
    assert_eq!(opacity(&timeline, "c", 1999), 1.0);
    assert_eq!(opacity(&timeline, "c", 2500), 0.25);
}

#[test]
fn duration_expression_markers_use_local_zero_and_previous_marker() {
    let timeline = clean(
        r#"
r: Rect, opacity: 1
let tick = 500ms
#2s
#{
  #(tick * 2)
  r.opacity = 0.5
  #+(tick)
  r.opacity = 0.25
}
"#,
    );
    assert_eq!(opacity(&timeline, "r", 2999), 1.0);
    assert_eq!(opacity(&timeline, "r", 3000), 0.5);
    assert_eq!(opacity(&timeline, "r", 3500), 0.25);
}

#[test]
fn retiming_scales_the_whole_child_clock() {
    let timeline = clean(
        r#"
r: Rect, opacity: 1
let clip = #{
  #1s
  r.opacity = 0.25 [1s]
}
#2s
stretch(clip, factor: 2)
"#,
    );
    assert_eq!(opacity(&timeline, "r", 3999), 1.0);
    assert_eq!(opacity(&timeline, "r", 6000), 0.25);
}

fn overrides(timeline: &Timeline, ms: u64) -> HashMap<String, HashMap<String, Value>> {
    let dimensions = SceneDimensions::default();
    let mut values = HashMap::new();
    let mut env = timeline.build_frame_env_internal(ms, dimensions, &values);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(program, ms, dimensions, &mut env, &mut values)
            .unwrap();
    }
    values
}

#[test]
fn local_always_has_a_finite_half_open_interval_and_local_seconds() {
    let timeline = clean(
        r#"
r: Rect, opacity: 1
#1s
#{
  always { r.opacity = t }
  #2s
}

"#,
    );
    assert!(overrides(&timeline, 999).is_empty());
    assert_eq!(
        overrides(&timeline, 1500).get("r").and_then(|p| p.get("opacity")),
        Some(&Value::Num(0.5))
    );
    assert!(
        overrides(&timeline, 3000).is_empty(),
        "local programs cannot hold their last output past the clip end"
    );
}

#[test]
fn reactive_plans_keep_actor_aliases_callable_captures_and_inner_shadowing() {
    let result = report(
        r#"
fn twice(x: Num) { x * 2 }
fn highlight(dot: Actor) { let op = twice
  #{ always {
    let local = (dot) => dot + 0.1
    dot.opacity = op(local(0.15))
  }
  #1s }
}
#0s
needle: Rect, opacity: 1
highlight(needle)
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let values = overrides(&result.output, 500);
    assert_eq!(
        values.get("needle").and_then(|p| p.get("opacity")),
        Some(&Value::Num(0.5)),
        "overrides={values:?}; modifiers={:?}; programs={:?}",
        result.output.modifiers,
        result.output.modifier_programs
    );
    assert!(!values.contains_key("dot"));
}

#[test]
fn invalid_plan_timing_is_reported() {
    for source in [
        "r: Rect\n#{ always { r.opacity = t } }",
        "r: Rect\nstretch(#{ r.opacity = 1 [1s] }, 0)",
        "r: Rect\nfit_duration(#{}, 1s)",
        "r: Rect\n#(2)\nr.opacity = 1",
    ] {
        let result = report(source);
        assert!(
            result.diagnostics.iter().any(|d| d.is_error()),
            "invalid timing accepted: {source}\n{:?}",
            result.diagnostics
        );
    }
}

#[test]
fn standard_and_user_calls_share_positional_named_and_default_rules() {
    let timeline = clean(
        r#"
fn add(a: Num, b: Num = 2) { a + b }
let default_sum = add(3)
let named_sum = add(b: 4, a: 3)
r: Rect, opacity: 0
#0s
std::fade_in(r, 500ms)
#1s
fade_out(target: r, duration: 500ms)
"#,
    );
    assert_eq!(timeline.env.get("default_sum"), Some(Value::Num(5.0)));
    assert_eq!(timeline.env.get("named_sum"), Some(Value::Num(7.0)));
    assert_eq!(opacity(&timeline, "r", 500), 1.0);
    assert_eq!(opacity(&timeline, "r", 1500), 0.0);
}

#[test]
fn namespaced_functions_use_private_helpers_and_values_from_their_definition() {
    use animatix_syntax::module::{ModuleGraph, SourceAccess};
    use std::path::Path;
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    for (path, source) in [
        (
            "/project/one.amx",
            "let hidden = 2\nfn helper() { hidden }\npub fn amount() { helper() + 1 }",
        ),
        (
            "/project/two.amx",
            "let hidden = 20\nfn helper() { hidden }\npub fn amount() { helper() + 1 }",
        ),
    ] {
        graph.upsert_source(Path::new(path).to_path_buf(), source);
    }
    let source = r#"
import "one.amx" as one
import "two.amx" as two
let hidden = 100
fn helper() { hidden }
let local = helper()
let first = one::amount()
let second = two::amount()
"#;
    let mut program = graph
        .load_program_with_source(Path::new("/project/main.amx"), Some(source))
        .unwrap();
    let type_errors = program.typecheck();
    assert!(type_errors.iter().all(|d| !d.is_error()), "{type_errors:?}");
    let mut errors = Vec::new();
    let expanded = program.expand_components(&mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    // The module-function context is supplied here by the module build boundary.
    let result = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(result.diagnostics.iter().all(|d| !d.is_error()), "{:?}", result.diagnostics);
    assert_eq!(result.output.env.get("local"), Some(Value::Num(100.0)));
    assert_eq!(result.output.env.get("first"), Some(Value::Num(3.0)));
    assert_eq!(result.output.env.get("second"), Some(Value::Num(21.0)));
    assert!(result.output.env.get("one::helper").is_none());
}

#[test]
fn lexical_functions_can_shadow_the_standard_prelude() {
    let timeline = clean(
        r#"
fn sin(value: Num) { value + 1 }
let local = sin(2)
let standard = std::sin(0)
"#,
    );
    assert_eq!(timeline.env.get("local"), Some(Value::Num(3.0)));
    assert_eq!(timeline.env.get("standard"), Some(Value::Num(0.0)));
}

#[test]
fn callable_values_keep_named_arguments_and_standard_identity() {
    let timeline = clean(
        r#"
fn rgb(r: Num, g: Num, b: Num) { r + g + b }
let calculation = std::sin
let result = calculation(value: 0)
let color = std::rgb(1, 0, 0)
let reveal = fade_in
r: Rect, opacity: 0
#0s
reveal(r, duration: 500ms)
"#,
    );
    assert_eq!(timeline.env.get("result"), Some(Value::Num(0.0)));
    assert_eq!(timeline.env.get("color"), Some(Value::Color([1.0, 0.0, 0.0, 1.0])));
    assert_eq!(opacity(&timeline, "r", 500), 1.0);
}

#[test]
fn ast_builds_preserve_lexical_globals_across_nested_calls() {
    let timeline = clean(
        r#"
let x = 1
fn inner() { x }
fn outer(x: Num) { inner() }
let result = outer(2)
"#,
    );
    assert_eq!(timeline.env.get("result"), Some(Value::Num(1.0)));
}

#[test]
fn fitting_duration_and_nested_retiming_preserve_marker_offsets() {
    let timeline = clean(
        r#"
r: Rect, opacity: 1
let clip = #{
  #500ms
  r.opacity = 0.5 [500ms]
  #2s
}
#1s
fit_duration(stretch(clip, 2), 4s)
"#,
    );
    assert_eq!(opacity(&timeline, "r", 1999), 1.0);
    assert_eq!(opacity(&timeline, "r", 3000), 0.5);
    assert_eq!(timeline.duration_seconds(), 5.0);
}

#[test]
fn a_child_plan_in_control_flow_gets_its_own_clock() {
    let timeline = clean(
        r#"
r: Rect, opacity: 1
#2s
if true {
  #{
    #500ms
    r.opacity = 0.25
  }
}
"#,
    );
    assert_eq!(opacity(&timeline, "r", 2499), 1.0);
    assert_eq!(opacity(&timeline, "r", 2500), 0.25);
}

#[test]
fn duration_arithmetic_and_boolean_results_keep_their_types() {
    let timeline = clean(
        "fn invert(value: Bool) -> Bool { !value }\n\
         fn compare(a: Num, b: Num) -> Bool { a > b && b >= 0 }\n\
         fn earlier(a: Duration, b: Duration) -> Bool { a < b }\n\
         let opposite = invert(1 < 2)\n\
         let ordered = compare(3, 2)\n\
         let before = earlier(500ms, 1s)\n\
         let negative = -500ms\n\
         let difference = 1s - 1500ms\n\
         let doubled = 250ms * 2",
    );
    assert_eq!(timeline.env.get("opposite"), Some(Value::Bool(false)));
    assert_eq!(timeline.env.get("ordered"), Some(Value::Bool(true)));
    assert_eq!(timeline.env.get("before"), Some(Value::Bool(true)));
    assert_eq!(timeline.env.get("negative"), Some(Value::Duration(-0.5)));
    assert_eq!(timeline.env.get("difference"), Some(Value::Duration(-0.5)));
    assert_eq!(timeline.env.get("doubled"), Some(Value::Duration(0.5)));
}

#[test]
fn standard_calls_can_animate_private_actors_of_the_containing_plan() {
    let timeline =
        clean("let clip = #{ local: Rect, opacity: 0\n fade_in(local, 500ms) }\n#1s\nclip");
    let roots = timeline.world.root_entities();
    assert_eq!(roots.len(), 1);
    assert_eq!(timeline.world.style(roots[0]).opacity.get(1500, 1.0), 1.0);
    assert!(timeline.world.get_id("local").is_none());
}

#[test]
fn private_plan_actors_cannot_be_persisted_through_a_child_callable() {
    let result = report("#0s\n#{ local: Rect, opacity: 1\n persist(local) }");
    assert!(
        result.diagnostics.iter().any(|d| d.is_error()
            && d.message.contains("reusable plan")
            && d.message.contains("persist")),
        "{:?}",
        result.diagnostics
    );
}

#[test]
fn actor_list_arguments_and_color_arguments_reach_the_property_backends() {
    let result = report(
        r##"
#0s
row: Row, opacity: 1 {
  a: Rect, opacity: 1
  b: Rect, opacity: 1
}
reorder(row, order: {b, a})
highlight(a, color: white, blend: normal)
highlight(b, color: "#ff0000", blend: difference)
"##,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let timeline = result.output;
    assert_eq!(timeline.get_child_order("row", 0), vec!["b", "a"]);
    for (label, color) in [("a", [1.0, 1.0, 1.0, 1.0]), ("b", [1.0, 0.0, 0.0, 1.0])] {
        let actor = timeline.world.get_id(label).unwrap();
        assert_eq!(
            timeline.world.highlight(actor).unwrap().highlight_color.get(0, [0.0; 4]),
            color
        );
    }
}

#[test]
fn repeated_plans_keep_nested_actor_declarations_and_closure_scopes_private() {
    let result = report(
        r#"
fn pair() { #{
  frame: Group, opacity: 1 {
    one: Rect, opacity: 1
    two: Rect, opacity: 1
  }
  always {
    let weight = (one) => one * 2
    one.opacity = weight(0.25)
  }
  #1s
} }
#0s
pair()
pair()
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let timeline = result.output;
    assert_eq!(timeline.world.tracks.len(), 6);
    assert_eq!(timeline.world.root_entities().len(), 2);
    for root in timeline.world.root_entities() {
        assert_eq!(timeline.world.child_labels(*root).count(), 2);
    }
    let values = overrides(&timeline, 500);
    assert_eq!(values.len(), 2);
    assert!(values.values().all(|p| p.get("opacity") == Some(&Value::Num(0.5))));
}

#[test]
fn bound_private_actor_identity_is_distinct_from_an_equal_blueprint_id() {
    use crate::timeline::{ActorRef, PlanDuration, TimelinePlanBuilder};
    let mut timeline = clean("#0s\nr: Rect, opacity: 1");
    let mut builder = TimelinePlanBuilder::default();
    let blueprint_id = builder.local_actor("unused").local_plan_id().unwrap();
    let bound = ActorRef::mounted(blueprint_id, "r");
    assert!(!bound.is_caller(), "bound private references stay private");
    builder.append_property_op(
        bound,
        "opacity",
        Value::Num(0.25),
        PlanDuration::default(),
        PlanDuration::from_seconds(0.5).unwrap(),
        None,
    );
    let mut diagnostics = Vec::new();
    timeline.mount_plan(&builder.finish(), 0.0, None, 1.0, None, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(opacity(&timeline, "r", 500), 0.25);
    assert_eq!(timeline.world.tracks.len(), 1);
}

#[test]
fn reused_plans_can_call_standard_functions_on_their_private_actors() {
    let result = report(
        r#"
let clip = #{ local: Rect, opacity: 1
  fade_out(local, duration: 500ms)
}
#0s
clip
#1s
clip
"#,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.output.world.tracks.len(), 2);
    for label in result.output.world.tracks.keys() {
        assert_eq!(opacity(&result.output, label, 1500), 0.0);
    }
}
