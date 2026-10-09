use animatix_syntax::parser::parse_source;

use super::*;

fn build_source(source: &str) -> (Timeline, Vec<animatix_syntax::diagnostics::Diagnostic>) {
    let (ast, _errors) = parse_source(source);
    let report = Timeline::build_with_diagnostics(
        &ast.expect("fixture should parse"),
        &std::collections::HashMap::new(),
    );
    (report.output, report.diagnostics)
}

#[test]
fn scene_stats_are_baked_when_referenced() {
    let source = r#"
config { duration: 4, resolution: (1280, 720) }

dot: Ellipse, size: (60, 60), color: accent.primary, at: (200, 300), opacity: 1.0

#0s
dot.at = (200, 300)
#2s
dot.at = (800, 300)

always {
  let m = scene.stats.motion
  let fx = scene.stats.focus_x
}
"#;

    let (timeline, diags) = build_source(source);
    let real_diags = without_content_lints(&diags).collect::<Vec<_>>();
    assert!(real_diags.is_empty(), "unexpected diagnostics: {real_diags:?}");

    assert!(timeline.stats_used, "stats_used flag must be true");

    for key in [
        "scene.stats.motion",
        "scene.stats.ink",
        "scene.stats.focus_x",
        "scene.stats.focus_y",
        "scene.stats.spread_x",
        "scene.stats.spread_y",
        "scene.stats.cast",
    ] {
        assert!(timeline.env_base.contains_key(key), "env_base must contain baked curve '{key}'");
        let val = timeline.env_base.get(key).unwrap();
        let Value::List(items) = val else {
            panic!("baked stat must be a List, got {val:?}");
        };
        assert!(
            items.len() >= 4,
            "curve must have at least 2 points (4 elements), got {}",
            items.len()
        );
        assert_eq!(items.len() % 2, 0, "curve must be interleaved [t, v] pairs");
    }

    // Verify focus_x is near 200 at t=0 and near 800 at t=2
    let fx = timeline.env_base.get("scene.stats.focus_x").unwrap();
    let fx0 =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fx.clone(), Value::Num(0.0)])
            .unwrap()
            .as_num();
    let fx2 =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fx.clone(), Value::Num(2.0)])
            .unwrap()
            .as_num();
    assert!((fx0 - 200.0).abs() < 5.0, "focus_x at t=0 should be ~200, got {fx0}");
    assert!((fx2 - 800.0).abs() < 5.0, "focus_x at t=2 should be ~800, got {fx2}");
}

#[test]
fn scene_stats_not_baked_when_unreferenced() {
    let source = r#"
config { duration: 2, resolution: (1280, 720) }

dot: Ellipse, size: (60, 60), color: accent.primary, at: (200, 300), opacity: 1.0

#0s
dot.at = (200, 300)
#2s
dot.at = (800, 300)
"#;

    let (timeline, _) = build_source(source);
    assert!(!timeline.stats_used, "stats_used must be false when unreferenced");
    assert!(
        !timeline.env_base.contains_key("scene.stats.motion"),
        "scene.stats curves must not be baked when not referenced"
    );
}

#[test]
fn scene_stats_purity_across_builds() {
    let source = r#"
config { duration: 3, resolution: (1280, 720) }

a: Rect, size: (100, 100), at: (100, 100), opacity: 1.0
b: Ellipse, size: (80, 80), at: (600, 400), opacity: 1.0

#0s
a.at = (100, 100)
#1s
a.at = (400, 200)

always {
  let e = curve_smooth(scene.stats.motion, t, 0.5)
}
"#;

    let (t1, _) = build_source(source);
    let (t2, _) = build_source(source);

    for key in [
        "scene.stats.motion",
        "scene.stats.ink",
        "scene.stats.focus_x",
        "scene.stats.focus_y",
        "scene.stats.spread_x",
        "scene.stats.spread_y",
        "scene.stats.cast",
    ] {
        let v1 = t1.env_base.get(key).unwrap();
        let v2 = t2.env_base.get(key).unwrap();
        assert_eq!(v1, v2, "baked curve '{key}' must be deterministic across builds");
    }
}

#[test]
fn scene_stats_seamless_loop_pins_endpoints() {
    let source = r#"
config { duration: 4, resolution: (640, 360), seamless_loop: true }

ball: Ellipse, size: (50, 50), at: (200, 180), opacity: 1.0

#0s
ball.at = (200, 180)
#2s
ball.at = (440, 180)
#4s
ball.at = (200, 180)

always {
  let m = scene.stats.motion
}
"#;

    let (timeline, diags) = build_source(source);
    let real_diags = without_content_lints(&diags).collect::<Vec<_>>();
    assert!(
        real_diags.is_empty(),
        "seamless loop should pass without warnings: {real_diags:?}"
    );

    for key in [
        "scene.stats.motion",
        "scene.stats.ink",
        "scene.stats.focus_x",
        "scene.stats.focus_y",
        "scene.stats.spread_x",
        "scene.stats.spread_y",
        "scene.stats.cast",
    ] {
        let val = timeline.env_base.get(key).unwrap();
        let first = crate::timeline::eval_shared::eval_builtin_fn(
            "curve_at",
            &[val.clone(), Value::Num(0.0)],
        )
        .unwrap()
        .as_num();
        let last = crate::timeline::eval_shared::eval_builtin_fn(
            "curve_at",
            &[val.clone(), Value::Num(4.0)],
        )
        .unwrap()
        .as_num();
        assert!(
            (first - last).abs() < 1e-3,
            "seamless loop must pin endpoints for '{key}': first={first}, last={last}"
        );
    }
}

#[test]
fn scene_stats_excludes_full_viewport_background() {
    let source = r#"
config { duration: 2, resolution: (1000, 1000) }

bg: Rect, size: (fill, fill), at: scene.center, opacity: 1.0
subject: Ellipse, size: (100, 100), at: (200, 200), opacity: 1.0

always {
  let fx = scene.stats.focus_x
  let fy = scene.stats.focus_y
}
"#;

    let (timeline, _) = build_source(source);
    let fx = timeline.env_base.get("scene.stats.focus_x").unwrap();
    let fy = timeline.env_base.get("scene.stats.focus_y").unwrap();
    let fx_val =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fx.clone(), Value::Num(1.0)])
            .unwrap()
            .as_num();
    let fy_val =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fy.clone(), Value::Num(1.0)])
            .unwrap()
            .as_num();

    // If bg were included, it would have area 1,000,000 at (500, 500) swamping the subject (area
    // 10,000 at 200, 200)
    assert!(
        (fx_val - 200.0).abs() < 10.0,
        "focus_x must center on subject (200), not full-screen bg (500), got {fx_val}"
    );
    assert!(
        (fy_val - 200.0).abs() < 10.0,
        "focus_y must center on subject (200), not full-screen bg (500), got {fy_val}"
    );
}

#[test]
fn ambience_component_evaluates_at_runtime() {
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/animation/39_ambience.amx");
    let program = graph.load_program(&path).unwrap();
    let mut diagnostics = Vec::new();
    let expanded = program.expand_components(&mut diagnostics);
    assert!(diagnostics.is_empty(), "expansion diagnostics: {diagnostics:?}");
    let report = Timeline::build_with_diagnostics(&expanded, &program.namespaces);
    let timeline = report.output;
    assert!(
        timeline.env_base.contains_key("scene.stats.motion"),
        "scene.stats.motion must be baked"
    );
    assert!(
        timeline.env_base.contains_key("scene.stats.focus_x"),
        "scene.stats.focus_x must be baked"
    );

    let mut sampled_at = Vec::new();
    let mut sampled_opacity = Vec::new();
    let mut sampled_size = Vec::new();

    for t_s in [0.0, 0.75, 1.5, 2.25, 3.0] {
        let time_ms = (t_s * 1000.0) as u64;
        let mut overrides = std::collections::HashMap::new();
        let mut env =
            timeline.build_frame_env_internal(time_ms, SceneDimensions::default(), &overrides);
        for p in &timeline.modifier_programs {
            timeline
                .apply_modifier_program(
                    p,
                    time_ms,
                    SceneDimensions::default(),
                    &mut env,
                    &mut overrides,
                )
                .expect("modifier program evaluation should succeed");
        }
        let wash_props = overrides.get("bg.wash").expect("bg.wash must have modifier overrides");
        if let Some(Value::Vec2(pos)) = wash_props.get("at") {
            sampled_at.push(*pos);
        }
        if let Some(Value::Num(op)) = wash_props.get("opacity") {
            sampled_opacity.push(*op);
        }
        if let Some(Value::Vec2(sz)) = wash_props.get("size") {
            sampled_size.push(*sz);
        }
    }

    assert_eq!(sampled_at.len(), 5);
    // Wash must visibly track actors horizontally (hero swings to 960 at 3s)
    let dx = (sampled_at[4][0] - sampled_at[1][0]).abs();
    assert!(
        dx > 150.0,
        "Ambience wash must visibly track horizontal actor movement: dx={dx}px"
    );

    // Wash must visibly track actors vertically
    let dy = (sampled_at[1][1] - sampled_at[2][1]).abs();
    assert!(dy > 20.0, "Ambience wash must visibly track vertical actor movement: dy={dy}px");

    // Wash opacity must breathe dynamically with motion energy
    let max_op = sampled_opacity.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_op = sampled_opacity.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(
        max_op - min_op > 0.015,
        "Ambience wash opacity must breathe with motion: max={max_op}, min={min_op}"
    );

    // Wash size must dilate and contract as actors separate and converge
    let max_sz = sampled_size.iter().map(|s| s[0]).fold(f64::NEG_INFINITY, f64::max);
    let min_sz = sampled_size.iter().map(|s| s[0]).fold(f64::INFINITY, f64::min);
    assert!(
        max_sz - min_sz > 40.0,
        "Ambience wash size must contract and expand with actor spread: max={max_sz}, min={min_sz}"
    );
}

#[test]
fn scene_stats_preserves_actors_with_non_spatial_modifiers() {
    let source = r#"
config { duration: 2, resolution: (1000, 1000) }

hero: Ellipse, size: (100, 100), at: (200, 500), opacity: 1.0

#0s
hero.at = (200, 500)
#2s
hero.at = (800, 500)

always {
  let m = scene.stats.motion
  let u = sin(t * 3.0)
  hero.color = rgb(1.0, 0.0, 0.0)
}
"#;

    let (timeline, _) = build_source(source);
    let fx = timeline.env_base.get("scene.stats.focus_x").unwrap();
    let fx_start =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fx.clone(), Value::Num(0.0)])
            .unwrap()
            .as_num();
    assert!(
        (fx_start - 200.0).abs() < 10.0,
        "hero must NOT be excluded from stats just because its color is animated in always: got {fx_start}"
    );

    let motion = timeline.env_base.get("scene.stats.motion").unwrap();
    let motion_val = crate::timeline::eval_shared::eval_builtin_fn(
        "curve_at",
        &[motion.clone(), Value::Num(1.0)],
    )
    .unwrap()
    .as_num();
    assert!(
        motion_val > 0.0,
        "hero motion must be captured even when color is animated in always: got {motion_val}"
    );
}

#[test]
fn scene_stats_excludes_containers_and_non_visual_actors() {
    let source = r#"
config { duration: 2, resolution: (1000, 1000) }

group: Group, at: (100, 100)
row: Row, at: (200, 200)
sound: Audio, url: "test.mp3"
hero: Ellipse, size: (100, 100), at: (600, 600), opacity: 1.0

always {
  let fx = scene.stats.focus_x
  let fy = scene.stats.focus_y
  let c = scene.stats.cast
}
"#;

    let (timeline, _) = build_source(source);
    let fx = timeline.env_base.get("scene.stats.focus_x").unwrap();
    let fy = timeline.env_base.get("scene.stats.focus_y").unwrap();
    let cast = timeline.env_base.get("scene.stats.cast").unwrap();

    let fx_val =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fx.clone(), Value::Num(1.0)])
            .unwrap()
            .as_num();
    let fy_val =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[fy.clone(), Value::Num(1.0)])
            .unwrap()
            .as_num();
    let cast_val =
        crate::timeline::eval_shared::eval_builtin_fn("curve_at", &[cast.clone(), Value::Num(1.0)])
            .unwrap()
            .as_num();

    // Only hero should count: cast must be 1.0, and focus must be on hero (600, 600)
    assert_eq!(cast_val, 1.0, "cast count must only include visual leaf actor (hero)");
    assert!(
        (fx_val - 600.0).abs() < 5.0,
        "focus_x must center on hero (600), not containers or audio: got {fx_val}"
    );
    assert!(
        (fy_val - 600.0).abs() < 5.0,
        "focus_y must center on hero (600), not containers or audio: got {fy_val}"
    );
}
