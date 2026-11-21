use super::*;
use crate::timeline::{BuildOptions, SceneDimensions, Timeline};

fn build(source: &str, options: BuildOptions<'_>) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "{parse_errors:?}");
    let report = Timeline::build_with_options(&ast.unwrap(), &Default::default(), options);
    assert!(
        !report.diagnostics.iter().any(|diagnostic| diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    report.output
}

fn run_both(
    timeline: &Timeline,
    time_ms: u64,
    env_override: Option<(&str, Value)>,
) -> ModifierOverrides {
    let dims = SceneDimensions::default();
    let program = &timeline.modifier_programs[0];
    let mut prepared = ModifierOverrides::new();
    let mut env = timeline.build_frame_env(time_ms, dims, &prepared);
    if let Some((key, value)) = env_override {
        env.set(key, value);
    }
    let mut generic_env = env.clone();
    generic_env.numeric_closures = None;
    let mut generic = ModifierOverrides::new();
    super::super::exec::execute_with_geometry(
        program,
        &mut generic_env,
        &mut generic,
        Some(ModifierGeometry {
            timeline,
            time_ms,
            dimensions: dims,
        }),
    )
    .unwrap();
    timeline
        .apply_modifier_program(program, time_ms, dims, &mut env, &mut prepared)
        .unwrap();
    assert_eq!(prepared, generic);
    prepared
}

#[test]
fn fixed_loops_precompute_targets_and_preserve_iteration_writes() {
    let source = r#"
let indices = {0, 1}
let factor = 2
#0s
  for i in indices {
    tiles[i]: Rect, size: (10, 10), at: (20, 20)
  }
  cursor: Rect
  always {
    let running = 1
    for i in indices {
      let x = (i + 1) * factor
      tiles[i].at = (x + t, 12)
      running = running + i
      tiles[i].scale = running
    }
    cursor.at = tiles[1].center
  }
"#;
    let timeline = build(source, Default::default());
    let prepared = timeline.prepared_modifiers.as_ref().unwrap();
    let Statement::Loop {
        body: LoopBody::Fixed(bodies),
        ..
    } = &prepared.statements[1]
    else {
        panic!("constant iteration list must be specialized");
    };
    assert!(matches!(
        bodies[0][1],
        Statement::Indexed {
            index: Index::Bound(_),
            ..
        }
    ));
    let values = run_both(&timeline, 500, None);
    assert_eq!(values["tiles__1"]["scale"], Value::Num(2.0));
    assert_eq!(values["cursor"]["at"], Value::Vec2([4.5, 12.0]));
    // A caller-provided environment value bypasses folding with its own values.
    let values = run_both(&timeline, 500, Some(("factor", Value::Num(9.0))));
    assert_eq!(values["cursor"]["at"], Value::Vec2([18.5, 12.0]));
}

#[test]
fn frozen_numeric_closures_keep_captures_and_respect_shadowing() {
    let source = r#"
let bias = 2
let local_pulse = (x, y) => abs(x - y) + bias
#0s
  marker: Rect
  always {
    marker.scale = local_pulse(t, 3)
  }
"#;
    let timeline = build(source, Default::default());
    let Value::Closure(_, body, captures) = timeline.env_base.get("local_pulse").unwrap() else {
        panic!("local_pulse should be a closure");
    };
    assert!(
        timeline
            .env
            .numeric_closures
            .as_ref()
            .is_some_and(|plans| plans.contains_key("local_pulse")),
        "ineligible body {body:?} with bias {:?}, abs {:?}, and frozen abs {:?}",
        captures.0.get("bias"),
        captures.0.get("abs"),
        timeline.env_base.get("abs")
    );
    let env = timeline.build_frame_env(1000, SceneDimensions::default(), &Default::default());
    let argument = |n| (None, CompiledExpr::Const(Value::Num(n)));
    assert_eq!(
        super::super::numeric_closure::try_named_call(
            &env,
            "local_pulse",
            &[argument(1.0), argument(3.0)],
            None
        )
        .expect("static track uses the frozen closure")
        .unwrap(),
        Value::Num(4.0)
    );
    let values = run_both(&timeline, 1000, Some(("bias", Value::Num(99.0))));
    assert_eq!(values["marker"]["scale"], Value::Num(4.0));
    let shadow = Value::NativeFn(Arc::new(|_, _| Ok(Value::Num(12.0))));
    let values = run_both(&timeline, 1000, Some(("local_pulse", shadow)));
    assert_eq!(values["marker"]["scale"], Value::Num(12.0));
}

#[test]
fn rebuild_reuses_unbound_ir_and_binds_new_scene_entities() {
    let source = r#"
#0s
  tiles[0]: Rect
  always {
    for i in {0} { tiles[i].rotation = sin(t) }
  }
"#;
    let first = build(source, Default::default());
    let cache = [first.modifier_ir_cache().unwrap()];
    let edited = source.replace("#0s\n", "#0s\n  added: Ellipse\n");
    let rebuilt = build(
        &edited,
        BuildOptions {
            modifier_ir_caches: &cache,
            ..Default::default()
        },
    );
    let second_cache = rebuilt.modifier_ir_cache().unwrap();
    assert!(Arc::ptr_eq(&cache[0], &second_cache));
    assert_ne!(first.world.get_id("tiles__0"), rebuilt.world.get_id("tiles__0"));
    let values = run_both(&rebuilt, 250, None);
    assert_eq!(values["tiles__0"]["rotation"], Value::Num(0.25_f64.sin()));
    let changed = edited.replace("rotation = sin(t)", "rotation = sin(t + 1)");
    let changed = build(
        &changed,
        BuildOptions {
            modifier_ir_caches: &cache,
            ..Default::default()
        },
    );
    assert!(!Arc::ptr_eq(&cache[0], &changed.modifier_ir_cache().unwrap()));
}

#[test]
fn modifier_slots_keep_loop_carried_values_and_publish_successful_writes_on_error() {
    let timeline = build(
        r#"
let wave = (x) => abs(x - 0.5)
#0s
  tiles[0]: Rect
  tiles[1]: Rect
  always {
    let accumulated = 0
    for i in {0, 1} {
      let amount = i + 2
      accumulated = accumulated + amount
      let displacement = wave(t) + accumulated
      tiles[i].scale = clamp(displacement, 0, 10)
    }
    let bad = -true
  }
"#,
        Default::default(),
    );
    let plan = timeline.prepared_modifiers.as_ref().unwrap();
    assert_eq!(plan.locals.names, ["accumulated", "amount", "bad", "displacement"]);
    assert!(matches!(plan.statements[1], Statement::Unrolled(_)));
    let dims = SceneDimensions::default();
    let program = &timeline.modifier_programs[0];
    let mut env = timeline.build_frame_env(250, dims, &Default::default());
    assert!(plan.matches(program, &env));
    let mut generic_env = env.clone();
    let mut prepared = ModifierOverrides::new();
    let mut generic = ModifierOverrides::new();
    let geometry = ModifierGeometry {
        timeline: &timeline,
        time_ms: 250,
        dimensions: dims,
    };
    let expected = super::super::exec::execute_with_geometry(
        program,
        &mut generic_env,
        &mut generic,
        Some(geometry),
    )
    .unwrap_err();
    let actual = timeline
        .apply_modifier_program(program, 250, dims, &mut env, &mut prepared)
        .unwrap_err();
    assert_eq!(actual.to_string(), expected.to_string());
    assert_eq!(prepared, generic);
    assert_eq!(prepared["tiles__1"]["scale"], Value::Num(5.25));
    for name in &plan.locals.names {
        assert_eq!(env.get(name), generic_env.get(name));
    }
    assert_eq!(env.get("i"), generic_env.get("i"));
    assert!(env.modifier_locals.iter().all(Option::is_none));

    let shadow = Value::NativeFn(Arc::new(|_, env| Ok(env.get("accumulated").unwrap())));
    env.set("wave", shadow);
    assert!(!plan.matches(program, &env));
}

#[test]
fn opaque_modifier_calls_keep_environment_visible() {
    let timeline = build(
        r#"
#0s
  tiles[0]: Rect
  always {
    let visible_value = t + 1
    tiles[0].scale = inspect()
  }
"#,
        Default::default(),
    );
    assert!(
        timeline
            .prepared_modifiers
            .as_ref()
            .is_none_or(|plan| plan.locals.names.is_empty())
    );
    let inspect = Value::NativeFn(Arc::new(|_, env| Ok(env.get("visible_value").unwrap())));
    let values = run_both(&timeline, 500, Some(("inspect", inspect)));
    assert_eq!(values["tiles__0"]["scale"], Value::Num(1.5));
}

#[test]
fn scalar_assignments_keep_the_straight_line_fast_path() {
    let timeline = build(
        r#"
#0s
  marker: Rect
  always {
    let angle = t + 1
    marker.at = (100 + cos(angle), 75 + sin(angle))
  }
"#,
        Default::default(),
    );
    assert!(timeline.prepared_modifiers.is_none());
    let values = run_both(&timeline, 1000, None);
    assert_eq!(
        values["marker"]["at"],
        Value::Vec2([100.0 + 2.0_f64.cos(), 75.0 + 2.0_f64.sin()])
    );
}
