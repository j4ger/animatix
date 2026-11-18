//! Exercise component methods through the public source-to-frame pipeline.

use std::collections::HashMap;
use std::path::Path;

use animatix_syntax::module::ModuleGraph;

use crate::timeline::{BuildOptions, SceneDimensions, Timeline, Value};

fn build(expression: &str) -> Timeline {
    build_with_bindings(expression, "")
}

fn build_with_bindings(expression: &str, bindings: &str) -> Timeline {
    let source = format!(
        r#"
component Badge(offset: Num) {{
  face: Rect, size: (10, 10), opacity: 1
  fn add(amount: Num = 1) {{ offset + amount + i }}
  pub fn read(mult: Num = 2) {{ self.add() * mult }}
}}
component Card(offset: Num) {{
  background: Rect, size: (30, 30), opacity: 1
  badge: Badge, offset: offset
  pub fn nested() {{ self.badge.read() }}
}}
fn sample(target: Actor, weight: Num = 3) {{ target.badge.read(mult: weight) }}
#0s
for unused, i in {{0, 1, 2}} {{ cards[i]: Card, offset: 10 * i }}
out: Rect, size: (10, 10), opacity: 1
{bindings}
always {{ out.opacity = {expression} }}
#3s
"#
    );
    let mut graph = ModuleGraph::new();
    let program = graph
        .load_program_with_source(Path::new("virtual:///frame_methods.amx"), Some(&source))
        .expect("load component methods");
    let expanded = program.expand_components().expect("component expansion");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    assert_eq!(report.output.content_epoch(), 0, "new component scenes have no edits yet");
    report.output
}

fn opacity_override(timeline: &Timeline, time_ms: u64) -> Value {
    let dimensions = SceneDimensions::default();
    let mut overrides = HashMap::new();
    let mut env = timeline.build_frame_env_internal(time_ms, dimensions, &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(program, time_ms, dimensions, &mut env, &mut overrides)
            .expect("pure method must run in a frame");
    }
    overrides
        .get("out")
        .and_then(|props| props.get("opacity"))
        .cloned()
        .expect("output")
}

#[test]
fn dynamic_nested_receiver_preserves_defaults_private_helpers_and_instance_captures() {
    let timeline = build("cards[floor(t)].badge.read()");
    let tables = (0..3)
        .map(|index| {
            timeline
                .entity(&format!("cards__{index}.badge"))
                .expect("expanded badge entity")
                .method_table_id()
                .expect("instance callable binding belongs in World")
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(tables.len(), 3, "each instance has its pre-bound callable table");
    for (time_ms, expected) in [(100, 2.0), (1100, 24.0), (2100, 46.0)] {
        assert_eq!(opacity_override(&timeline, time_ms), Value::Num(expected));
    }
}

#[test]
fn cloned_timeline_keeps_method_handles_valid_through_actor_parameters() {
    let original = build("sample(cards[floor(t)])");
    let cloned = original.clone();
    drop(original);
    for (time_ms, expected) in [(2100, 69.0), (100, 3.0), (1100, 36.0)] {
        assert_eq!(opacity_override(&cloned, time_ms), Value::Num(expected));
    }
}

#[test]
fn method_receiver_can_call_a_child_component_method_in_a_frame() {
    let timeline = build("cards[floor(t)].nested()");
    for (time_ms, expected) in [(100, 2.0), (1100, 24.0), (2100, 46.0)] {
        assert_eq!(opacity_override(&timeline, time_ms), Value::Num(expected));
    }
}

#[test]
fn build_time_closure_captures_receive_resolved_actor_handles() {
    let timeline = build_with_bindings(
        "sampled()",
        "let held = cards[1]\nlet sampled = () => held.badge.read()",
    );
    assert_eq!(opacity_override(&timeline, 100), Value::Num(24.0));
}

#[test]
fn actor_alias_variable_tracks_keep_resolved_method_handles() {
    let timeline = build_with_bindings("sample(held)", "let held = cards[1]");
    assert_eq!(opacity_override(&timeline, 100), Value::Num(36.0));
}

#[test]
fn module_functions_keep_scene_actor_bindings_through_private_helpers() {
    let timeline = build_with_bindings(
        "read_scene()",
        "fn helper() { cards[1].badge.read() }\nfn read_scene() { helper() }",
    );
    assert_eq!(opacity_override(&timeline, 100), Value::Num(24.0));
}

#[test]
fn module_function_helpers_resolve_scene_actor_methods() {
    let source = r#"
component Card {
  face: Rect, size: (10, 10), opacity: 1
  pub fn value() { 0.25 }
}
fn helper() { card.value() }
fn read_scene() { helper() }
#0s
card: Card
out: Rect, size: (10, 10), opacity: 1
always { out.opacity = read_scene() }
#1s
"#;
    let mut graph = ModuleGraph::new();
    let program = graph
        .load_program_with_source(
            Path::new("virtual:///frame_method_scene_actor.amx"),
            Some(source),
        )
        .expect("load scene actor method source");
    let expanded = program.expand_components().expect("component expansion");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(opacity_override(&report.output, 100), Value::Num(0.25));
}

#[test]
fn plot_loop_captures_keep_component_method_handles_after_the_loop_scope_ends() {
    let timeline = build_with_bindings(
        "1",
        r#"
for target, j in {cards[1], cards[2]} {
  curves[j]: PlotCurve, func: (x) => target.badge.read() + x,
    x_domain: (0, 1), y_domain: (0, 100), size: (100, 100), opacity: 1
}
"#,
    );
    let mut env =
        timeline.build_frame_env_internal(100, SceneDimensions::default(), &HashMap::new());
    for (index, expected) in [(0, 24.5), (1, 46.5)] {
        let id = timeline.world.get_id(&format!("curves__{index}")).unwrap();
        let plot = timeline.world.plot_payload.get(id).unwrap().procedural_plot.as_ref().unwrap();
        let actual = crate::timeline::plot::eval_compiled_parts_scalar(
            &plot.func_args,
            &plot.func_body,
            &plot.extra_captures,
            &mut env,
            "x",
            0.5,
            &mut HashMap::new(),
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn resolving_callable_handles_preserves_actor_reference_identity() {
    use std::hash::{Hash, Hasher};

    use crate::timeline::EntityId;
    use crate::timeline::plan_value::ActorRef;
    use crate::timeline::world::MethodTableId;

    let reference = ActorRef::caller("card");
    let resolved = reference.with_world_identity(EntityId(7), Some(MethodTableId::new(3)));
    assert_eq!(reference, resolved);

    let hash = |reference: &ActorRef| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        reference.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(hash(&reference), hash(&resolved));
    assert_ne!(reference, ActorRef::caller("another_card"));
}

#[test]
fn freezing_method_handles_keeps_plan_actor_references_private() {
    let timeline = build_with_bindings("1", "#{ private: Card, offset: 4 }");
    let env = timeline.build_frame_env_internal(0, SceneDimensions::default(), &HashMap::new());
    let mut private_instances = 0;
    for actor in timeline.world.views() {
        if actor.plan_ownership().is_none() || actor.method_table_id().is_none() {
            continue;
        }
        let Some(Value::ActorRef(reference)) = env.get(actor.label()) else {
            panic!("missing bound private instance {}", actor.label());
        };
        assert!(!reference.is_caller(), "binding must preserve the mount's ownership");
        private_instances += 1;
    }
    assert_eq!(private_instances, 2, "private Card and its Badge both have methods");
}
