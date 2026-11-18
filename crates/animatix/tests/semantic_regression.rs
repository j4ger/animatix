//! Black-box semantic regressions for newly composed timeline features.
//!
//! These cases build real `.amx` programs through the syntax/module pipeline,
//! evaluate the resulting timelines, and inspect public scene output. They
//! intentionally avoid copies of the production time mapping or plot logic.

use std::collections::BTreeMap;
use std::path::Path;

use animatix::primitives::RenderCommand;
use animatix::timeline::scene_program::SceneProgram;
use animatix::timeline::{DebugRenderOptions, RuntimeInputs, SceneDimensions, Timeline};
use animatix_syntax::module::{ModuleGraph, SourceAccess};
use kurbo::Shape as _;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1280,
    height: 720,
};

fn build_report(source: &str) -> (Timeline, Vec<String>) {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    let entry = Path::new("virtual:///semantic_regression.amx");
    graph.upsert_source(entry.to_path_buf(), source.to_string());
    let mut program = graph.load_program(entry).expect("fixture should parse and load");
    let type_diagnostics = program.typecheck();
    assert!(type_diagnostics.is_empty(), "type diagnostics: {type_diagnostics:?}");
    let expanded = program.expand_components().expect("component expansion");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        animatix::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    let diagnostics =
        report.diagnostics.iter().map(|diagnostic| format!("{diagnostic:?}")).collect();
    (report.output, diagnostics)
}

fn build(source: &str) -> Timeline {
    let (timeline, diagnostics) = build_report(source);
    assert!(diagnostics.is_empty(), "build diagnostics: {diagnostics:?}");
    timeline
}

fn frame(timeline: &Timeline, time_s: f64) -> SceneProgram {
    let mut backend = None;
    timeline.evaluate_program_with_debug(time_s, DIMS, DebugRenderOptions::default(), &mut backend)
}

fn frame_with_inputs(timeline: &Timeline, time_s: f64, inputs: &RuntimeInputs) -> SceneProgram {
    let mut backend = None;
    timeline
        .evaluate_program_with_inputs(
            time_s,
            DIMS,
            DebugRenderOptions::default(),
            &mut backend,
            inputs,
        )
        .expect("input snapshot should match this timeline")
}

fn bounds(program: &SceneProgram, actor: &str) -> kurbo::Rect {
    *program.precise_bounds.get(actor).unwrap_or_else(|| {
        panic!(
            "expected world-space bounds for {actor}; items were {:?}",
            program.items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>()
        )
    })
}

fn center(rect: kurbo::Rect) -> (f64, f64) {
    ((rect.x0 + rect.x1) * 0.5, (rect.y0 + rect.y1) * 0.5)
}

fn close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() < 1.0,
        "{context}: expected {expected:.2}, got {actual:.2}"
    );
}

fn assert_marker_at_decay_endpoint(program: &SceneProgram, lane: &str) {
    let curve = bounds(program, &format!("{lane}.loss_curve"));
    let marker = center(bounds(program, &format!("{lane}.marker")));
    // The plotted function is monotone decreasing over x ∈ [0, 6], so its
    // right/lower world-space corner is the x=6 endpoint. Allow the authored
    // 4px curve stroke to expand the recorded bounds by two pixels.
    assert!(
        (marker.0 - curve.x1).abs() < 3.0,
        "marker x must meet the plotted endpoint: marker={marker:?}, curve={curve:?}"
    );
    assert!(
        (marker.1 - curve.y1).abs() < 3.0,
        "marker y must meet the plotted endpoint: marker={marker:?}, curve={curve:?}"
    );
}

fn curve_bounds(program: &SceneProgram, actor: &str) -> kurbo::Rect {
    let item = program.items.iter().find(|item| item.label == actor).unwrap_or_else(|| {
        panic!(
            "missing plot actor {actor}; observed {:?}",
            program.items.iter().map(|item| item.label.as_str()).collect::<Vec<_>>()
        )
    });
    item.commands
        .iter()
        .flat_map(|command| match command {
            RenderCommand::Paths { paths } => {
                paths.iter().map(|path| path.path.bounding_box()).collect()
            },
            _ => Vec::new(),
        })
        .reduce(|a, b| a.union(b))
        .unwrap_or_else(|| panic!("plot actor {actor} emitted no path geometry"))
}

fn text_glyph_count(program: &SceneProgram, actor: &str) -> usize {
    let item = program
        .items
        .iter()
        .find(|item| item.label == actor)
        .unwrap_or_else(|| panic!("missing text actor {actor}"));
    item.commands
        .iter()
        .map(|command| match command {
            RenderCommand::Text { paths, .. } => paths.len(),
            _ => 0,
        })
        .sum()
}

fn text_outline_signature(program: &SceneProgram, actor: &str) -> Vec<String> {
    let item = program
        .items
        .iter()
        .find(|item| item.label == actor)
        .unwrap_or_else(|| panic!("missing text actor {actor}"));
    item.commands
        .iter()
        .flat_map(|command| match command {
            RenderCommand::Text { paths, .. } => paths
                .iter()
                .flat_map(|path| path.path.elements().iter())
                .map(|element| format!("{element:?}"))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn local_ticker_example_is_seekable_scoped_and_hands_off_at_clip_end() {
    let timeline = build(include_str!("../../../examples/animation/local_tickers.amx"));

    // One authored 500ms ticker is called twice for the same actor at different
    // starts and rates; a second actor runs concurrently at a different rate.
    assert!((timeline.duration_seconds() - 4.9).abs() < 1e-6);
    assert!(timeline.has_keyframe_at("needle", "position", 2000));
    assert!(timeline.has_keyframe_at("satellite", "position", 1900));
    assert!(timeline.has_keyframe_at("needle", "position", 4900));

    let before_start = frame(&timeline, 0.98);
    let at_start = frame(&timeline, 1.0);
    close(center(bounds(&before_start, "needle")).0, 140.0, "pre-call value");
    close(center(bounds(&at_start, "needle")).0, 140.0, "local zero value");

    let first_mid = frame(&timeline, 1.5);
    close(center(bounds(&first_mid, "needle")).0, 380.0, "stretch midpoint");
    close(center(bounds(&first_mid, "needle")).1, 300.0, "first instance lane");
    let satellite_mid = frame(&timeline, 1.45);
    close(center(bounds(&satellite_mid, "satellite")).0, 380.0, "fit_duration midpoint");
    close(center(bounds(&satellite_mid, "satellite")).1, 480.0, "second instance lane");

    let first_end = frame(&timeline, 2.0);
    close(center(bounds(&first_end, "needle")).0, 620.0, "first call end handoff");
    let after_first_end = frame(&timeline, 2.02);
    close(
        center(bounds(&after_first_end, "needle")).0,
        615.2,
        "global position track takes over after scoped always ends",
    );
    close(center(bounds(&frame(&timeline, 1.9), "satellite")).0, 620.0, "short clip end");
    close(
        center(bounds(&frame(&timeline, 1.92), "satellite")).0,
        620.0,
        "short clip end handoff",
    );

    close(center(bounds(&frame(&timeline, 3.98), "needle")).1, 300.0, "before second call");
    close(center(bounds(&frame(&timeline, 4.0), "needle")).1, 300.0, "second call start");
    let second_mid = frame(&timeline, 4.45);
    close(center(bounds(&second_mid, "needle")).0, 380.0, "second call midpoint");
    close(
        center(bounds(&second_mid, "needle")).1,
        300.0,
        "second call stays in the original lane",
    );
    let second_end = frame(&timeline, 4.9);
    close(center(bounds(&second_end, "needle")).0, 620.0, "fit_duration end handoff");
    let after_second_end = frame(&timeline, 4.92);
    close(
        center(bounds(&after_second_end, "needle")).0,
        620.0,
        "second scope stopped after end",
    );

    // Deliberately seek out of order through a reused Timeline. Each sample
    // must match a fresh-order result rather than leaking the previous local t.
    let late_then_early = frame(&timeline, 4.45);
    let early_again = frame(&timeline, 1.5);
    close(
        center(bounds(&late_then_early, "needle")).0,
        center(bounds(&second_mid, "needle")).0,
        "repeat second midpoint",
    );
    close(
        center(bounds(&early_again, "needle")).0,
        center(bounds(&first_mid, "needle")).0,
        "seek back to first midpoint",
    );
}

#[test]
fn nested_local_time_scopes_compose_and_preserve_relative_handoffs() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (900, 500), duration: 4 }
dot: Ellipse, at: (100, 300), size: (24, 24), color: accent.info

fn inner(dot, lane_y: Num) { return #{
  #0ms
  dot.at = (140, lane_y)
  always { dot.at = (140 + 960 * t, lane_y) }
  #500ms
  dot.at = (620, lane_y)

} }

fn outer(dot, lane_y: Num) { return #{
  #0ms
  stretch(inner(dot, lane_y), 2.0)
  #+1200ms
  dot.at = (900, lane_y)

} }

#0s
fade_in(target: dot, duration: 100ms)
#1s
stretch(outer(dot, 300), 2.0)
"#,
    );

    // Inner's 500ms source becomes 1s at its own 2x rate, then the outer
    // 2x mapping stretches that interval to 2s globally. The enclosing
    // relative marker at local +1200ms maps to scene time 3.4s.
    assert!(timeline.has_keyframe_at("dot", "position", 1000));
    assert!(timeline.has_keyframe_at("dot", "position", 3000));
    assert!(timeline.has_keyframe_at("dot", "position", 3400));
    assert!((timeline.duration_seconds() - 3.4).abs() < 1e-6);

    let before = frame(&timeline, 0.98);
    let start = frame(&timeline, 1.0);
    let middle = frame(&timeline, 2.0);
    let inner_end = frame(&timeline, 3.0);
    let after_inner = frame(&timeline, 3.1);
    let outer_relative = frame(&timeline, 3.4);
    let after_outer = frame(&timeline, 3.42);
    close(
        center(bounds(&before, "dot")).0,
        139.2,
        "global position track interpolates up to nested call start",
    );
    close(center(bounds(&start, "dot")).0, 140.0, "nested call local zero");
    close(center(bounds(&middle, "dot")).0, 380.0, "composed local midpoint");
    close(center(bounds(&inner_end, "dot")).0, 620.0, "nested inner end");
    close(
        center(bounds(&after_inner, "dot")).0,
        690.0,
        "global position track takes over between inner and outer handoffs",
    );
    close(center(bounds(&outer_relative, "dot")).0, 900.0, "outer relative handoff");
    close(center(bounds(&after_outer, "dot")).0, 900.0, "outer scope end hold");

    let late_then_early = frame(&timeline, 2.0);
    let early_again = frame(&timeline, 1.0);
    assert_eq!(bounds(&late_then_early, "dot"), bounds(&middle, "dot"));
    assert_eq!(bounds(&early_again, "dot"), bounds(&start, "dot"));
}

#[test]
fn scoped_plot_func_uses_local_t_then_holds_and_yields_to_later_func_keyframe() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (900, 500), duration: 4 }
#0s
graph: Graph, at: (450, 250), size: (520, 320),
  x_domain: (0, 2), y_domain: (-1, 1),
  grid: false, ticks: false, tick_labels: "none" {
  curve: PlotCurve, kind: "cartesian", func: (x) => sin(x),
    stroke: accent.info, stroke_width: 5
}

fn sweep(plot) { return #{
  #0ms
  plot.func = (x) => sin(x + t)
  #500ms
  plot.func = (x) => sin(x + 0.5)

} }

#1s
stretch(sweep(graph.curve), 2.0)

#3s
graph.curve.func = (x) => sin(x + 1.0)
"#,
    );

    // This samples real SceneProgram PlotCurve paths after build/evaluation;
    // it proves the rewritten closure reaches the plot backend, not just AST.
    let start = curve_bounds(&frame(&timeline, 1.0), "curve");
    let middle = curve_bounds(&frame(&timeline, 1.5), "curve");
    let end = curve_bounds(&frame(&timeline, 2.0), "curve");
    let after_end = curve_bounds(&frame(&timeline, 2.2), "curve");
    let later_after_end = curve_bounds(&frame(&timeline, 2.8), "curve");
    let later_func = curve_bounds(&frame(&timeline, 3.0), "curve");

    assert_ne!(start, middle, "local free `t` must change sampled PlotCurve geometry");
    assert_ne!(middle, end, "the explicit local end closure should take over");
    assert_eq!(end, after_end, "the clip end closure must hold after its local scope");
    assert_eq!(end, later_after_end, "the end value must remain stable across later frames");
    assert_ne!(end, later_func, "a later func keyframe must take over after the local clip");

    // The same timeline must give the same geometry when sought backwards.
    assert_eq!(middle, curve_bounds(&frame(&timeline, 1.5), "curve"));
    assert_eq!(after_end, curve_bounds(&frame(&timeline, 2.2), "curve"));
}

#[test]
fn local_plan_intervals_do_not_depend_on_authored_t_bindings() {
    for (name, function, invocation, expected_x) in [
        (
            "function parameter",
            r#"
fn bad(dot, t: Num) { return #{
  #0ms
  always { dot.at = (100 + 960 * t, 300) }
  #500ms
  dot.at = (620, 300)

} }
"#,
            "stretch(bad(dot, 0.25), 2.0)",
            340.0,
        ),
        (
            "loop binding",
            r#"
fn bad(dot) { return #{
  #0ms
  for t in {0, 1} {
    always { dot.at = (100 + 10 * t, 300) }
  }
  #500ms
  dot.at = (620, 300)

} }
"#,
            "stretch(bad(dot), 2.0)",
            110.0,
        ),
    ] {
        let source = format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (640, 360), duration: 2 }}
{function}
#0s
dot: Ellipse, at: (100, 300), size: (20, 20), color: accent.info
#1s
{invocation}
"#
        );
        let timeline = build(&source);
        close(
            center(bounds(&frame(&timeline, 0.98), "dot")).0,
            100.0,
            &format!("{name} before plan"),
        );
        close(
            center(bounds(&frame(&timeline, 1.5), "dot")).0,
            expected_x,
            &format!("{name} lexical t"),
        );
        close(
            center(bounds(&frame(&timeline, 2.02), "dot")).0,
            620.0,
            &format!("{name} finite plan end"),
        );
    }
}

#[test]
fn compare_swap_example_keeps_the_two_cards_as_one_moving_unit() {
    let timeline = build(include_str!("../../../examples/projects/compare_swap.amx"));
    let order = |time_ms| timeline.get_child_order("pair", time_ms);
    assert_eq!(order(0), vec!["card8", "card3"]);
    assert_eq!(order(1350), vec!["card3", "card8"]);
    assert_eq!(order(1700), vec!["card3", "card8"]);

    // Repeated seeks should preserve the computed order and both component
    // instances should remain visible as complete cards after the swap.
    let before = frame(&timeline, 0.5);
    let mid = frame(&timeline, 1.35);
    let settled = frame(&timeline, 1.8);
    assert!(mid.items.iter().any(|item| item.label == "number8"));
    assert!(mid.items.iter().any(|item| item.label == "number3"));
    let before8 = center(bounds(&before, "tile8")).0;
    let before3 = center(bounds(&before, "tile3")).0;
    let settled8 = center(bounds(&settled, "tile8")).0;
    let settled3 = center(bounds(&settled, "tile3")).0;
    assert!(
        settled8 > before8 + 100.0,
        "8-card should move right: before={before8}, after={settled8}"
    );
    assert!(
        settled3 < before3 - 100.0,
        "3-card should move left: before={before3}, after={settled3}"
    );
    for (frame_name, program) in [("before swap", &before), ("after swap", &settled)] {
        for (number, tile) in [("number8", "tile8"), ("number3", "tile3")] {
            let number_center = center(bounds(program, number));
            let tile_center = center(bounds(program, tile));
            assert!(
                (number_center.0 - tile_center.0).abs() < 12.0
                    && (number_center.1 - tile_center.1).abs() < 30.0,
                "{number} must stay centered inside {tile} at {frame_name}: number={number_center:?}, tile={tile_center:?}"
            );
        }
    }
    let back = frame(&timeline, 0.5);
    let forward = frame(&timeline, 1.35);
    assert_eq!(order(1350), vec!["card3", "card8"]);
    assert_eq!(bounds(&back, "tile8"), bounds(&before, "tile8"));
    assert!(forward.items.iter().any(|item| item.label == "number3"));
}

#[test]
fn runtime_progress_updates_two_component_plots_markers_and_text_at_same_time() {
    let timeline = build(include_str!("../../../examples/data/shared_progress_math.amx"));
    assert_eq!(
        timeline.runtime_input_names().iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["progress"]
    );

    let inputs_at_low = timeline
        .validate_runtime_inputs(&BTreeMap::from([("progress".to_string(), 0.25)]))
        .expect("whitelisted progress should validate");
    let low = frame_with_inputs(&timeline, 3.0, &inputs_at_low);

    let inputs_at_high = timeline
        .validate_runtime_inputs(&BTreeMap::from([("progress".to_string(), 0.75)]))
        .expect("whitelisted progress should validate");
    let high = frame_with_inputs(&timeline, 3.0, &inputs_at_high);
    let repeated_high = timeline
        .validate_runtime_inputs(&BTreeMap::from([("progress".to_string(), 0.75)]))
        .expect("the same complete snapshot should validate identically");
    assert_eq!(inputs_at_high.cache_key(), repeated_high.cache_key());
    let high_cache_hit = frame_with_inputs(&timeline, 3.0, &repeated_high);

    // At the same paused scene time, both independent component instances
    // must use the same newly supplied progress for their procedural curve,
    // point marker, and text state.
    assert_ne!(curve_bounds(&low, "slow.loss_curve"), curve_bounds(&high, "slow.loss_curve"));
    assert_ne!(curve_bounds(&low, "fast.loss_curve"), curve_bounds(&high, "fast.loss_curve"));
    assert_ne!(bounds(&low, "slow.marker"), bounds(&high, "slow.marker"));
    assert_ne!(bounds(&low, "fast.marker"), bounds(&high, "fast.marker"));
    assert_ne!(
        text_outline_signature(&low, "slow.loss_readout"),
        text_outline_signature(&high, "slow.loss_readout")
    );
    assert_ne!(
        text_outline_signature(&low, "fast.loss_readout"),
        text_outline_signature(&high, "fast.loss_readout")
    );
    assert_ne!(
        text_outline_signature(&low, "progress_readout"),
        text_outline_signature(&high, "progress_readout")
    );
    assert_ne!(text_glyph_count(&low, "slow.state"), text_glyph_count(&high, "slow.state"));
    assert_ne!(text_glyph_count(&low, "fast.state"), text_glyph_count(&high, "fast.state"));
    assert_marker_at_decay_endpoint(&low, "slow");
    assert_marker_at_decay_endpoint(&low, "fast");
    assert_marker_at_decay_endpoint(&high, "slow");
    assert_marker_at_decay_endpoint(&high, "fast");
    assert_eq!(
        curve_bounds(&high, "slow.loss_curve"),
        curve_bounds(&high_cache_hit, "slow.loss_curve")
    );
    assert_eq!(bounds(&high, "fast.marker"), bounds(&high_cache_hit, "fast.marker"));

    let automatic = timeline
        .validate_runtime_inputs(&BTreeMap::new())
        .expect("empty snapshot clears runtime inputs");
    let after_clear = frame_with_inputs(&timeline, 3.0, &automatic);
    let automatic_frame = frame(&timeline, 3.0);
    let explicit_midpoint = timeline
        .validate_runtime_inputs(&BTreeMap::from([("progress".to_string(), 0.5)]))
        .expect("midpoint progress should validate");
    let midpoint_frame = frame_with_inputs(&timeline, 3.0, &explicit_midpoint);
    assert_ne!(
        curve_bounds(&high, "slow.loss_curve"),
        curve_bounds(&after_clear, "slow.loss_curve")
    );
    assert_ne!(bounds(&high, "fast.marker"), bounds(&after_clear, "fast.marker"));
    assert_eq!(
        curve_bounds(&automatic_frame, "slow.loss_curve"),
        curve_bounds(&after_clear, "slow.loss_curve")
    );
    assert_eq!(bounds(&automatic_frame, "fast.marker"), bounds(&after_clear, "fast.marker"));
    assert_marker_at_decay_endpoint(&automatic_frame, "slow");
    assert_marker_at_decay_endpoint(&automatic_frame, "fast");
    for lane in ["slow", "fast"] {
        assert_eq!(
            curve_bounds(&automatic_frame, &format!("{lane}.loss_curve")),
            curve_bounds(&midpoint_frame, &format!("{lane}.loss_curve")),
            "automatic driver at t=3s should produce progress=0.5 for {lane}"
        );
        assert_eq!(
            bounds(&automatic_frame, &format!("{lane}.marker")),
            bounds(&midpoint_frame, &format!("{lane}.marker")),
            "automatic marker at t=3s should match explicit midpoint progress for {lane}"
        );
        assert_eq!(
            text_outline_signature(&automatic_frame, &format!("{lane}.loss_readout")),
            text_outline_signature(&midpoint_frame, &format!("{lane}.loss_readout")),
            "automatic numeric readout should match explicit midpoint progress for {lane}"
        );
    }
    assert_eq!(
        text_outline_signature(&automatic_frame, "progress_readout"),
        text_outline_signature(&midpoint_frame, "progress_readout"),
        "automatic progress readout should match explicit midpoint progress"
    );
    assert_ne!(
        text_outline_signature(&automatic_frame, "slow.state"),
        text_outline_signature(&low, "slow.state"),
        "automatic progress=0.5 must update state text to LATE rather than leave base EARLY"
    );
    assert_eq!(
        text_outline_signature(&automatic_frame, "progress_readout"),
        text_outline_signature(&after_clear, "progress_readout")
    );

    // Rejected structure/unknown values must not mutate the accepted snapshot.
    let rejected = timeline.validate_runtime_inputs(&BTreeMap::from([
        ("progress".to_string(), 0.9),
        ("undeclared".to_string(), 0.1),
    ]));
    assert!(rejected.is_err(), "unknown inputs must be rejected atomically");
    let unchanged = frame_with_inputs(&timeline, 3.0, &inputs_at_high);
    assert_eq!(
        curve_bounds(&high, "slow.loss_curve"),
        curve_bounds(&unchanged, "slow.loss_curve")
    );
    assert_eq!(bounds(&high, "fast.marker"), bounds(&unchanged, "fast.marker"));
    let non_finite =
        timeline.validate_runtime_inputs(&BTreeMap::from([("progress".to_string(), f64::NAN)]));
    assert!(non_finite.is_err(), "non-finite values must be rejected");

    // Inputs are immutable caller-owned snapshots, not hidden mutable state
    // inside a Timeline. A clone keeps the whitelist and evaluates a supplied
    // snapshot with its own disposable caches.
    let clone = timeline.clone();
    assert_eq!(clone.runtime_input_names(), timeline.runtime_input_names());
    let cloned = frame_with_inputs(&clone, 3.0, &inputs_at_high);
    assert_eq!(curve_bounds(&high, "fast.loss_curve"), curve_bounds(&cloned, "fast.loss_curve"));
    assert_eq!(bounds(&high, "slow.marker"), bounds(&cloned, "slow.marker"));

    let incompatible = build(
        r#"
config { runtime_inputs: {"other"} }
"#,
    );
    let mut backend = None;
    assert!(
        incompatible
            .evaluate_program_with_inputs(
                3.0,
                DIMS,
                DebugRenderOptions::default(),
                &mut backend,
                &inputs_at_high,
            )
            .is_err(),
        "a snapshot for a different input whitelist must be rejected"
    );
}

#[test]
fn runtime_input_whitelist_rejects_structural_names_and_builtin_functions() {
    let (non_numeric, non_numeric_diagnostics) = build_report(
        r#"
config { runtime_inputs: {"gain"} }
let gain = {1, 2}
"#,
    );
    assert!(
        non_numeric_diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("InvalidConfigValue") && diagnostic.contains("gain")
        }),
        "a structural list variable cannot be a runtime input: {non_numeric_diagnostics:?}"
    );
    assert!(
        !non_numeric.runtime_input_names().iter().any(|name| name == "gain"),
        "invalid declarations must be removed from the accepted whitelist"
    );

    let (structural, structural_diagnostics) = build_report(
        r#"
config { runtime_inputs: {"count"} }
let count = 2
#0s
if count > 1 {
  item: Rect, size: (20, 20), at: (40, 40)
}
"#,
    );
    assert!(
        structural_diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("InvalidConfigValue") && diagnostic.contains("count")
        }),
        "a runtime input cannot decide whether an actor is structurally built: {structural_diagnostics:?}"
    );
    assert!(
        !structural.runtime_input_names().iter().any(|name| name == "count"),
        "structural input must be removed from the accepted whitelist"
    );

    let (builtin, builtin_diagnostics) = build_report(
        r#"
config { runtime_inputs: {"sin"} }
"#,
    );
    assert!(
        builtin_diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("InvalidConfigValue") && diagnostic.contains("sin")
        }),
        "a built-in function cannot be shadowed by an input: {builtin_diagnostics:?}"
    );
    assert!(
        !builtin.runtime_input_names().iter().any(|name| name == "sin"),
        "built-in function names must be excluded from the accepted whitelist"
    );
}

#[test]
fn keyframed_captured_variable_resamples_a_plot_without_continuous_let_interpolation() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), duration: 2 }
#0s
let slope = 1.0
graph: Graph, at: (320, 180), size: (400, 240), x_domain: (0, 1), y_domain: (0, 2.5),
  grid: false, ticks: false, tick_labels: "none" {
  line: PlotCurve, kind: "cartesian", func: (x) => slope * x, stroke_width: 4
}
#2s
let slope = 2.0
"#,
    );

    let start = frame(&timeline, 0.0);
    let middle = frame(&timeline, 1.0);
    let stamped = frame(&timeline, 2.0);
    let after = frame(&timeline, 2.0);
    let start_bounds = curve_bounds(&start, "line");
    let middle_bounds = curve_bounds(&middle, "line");
    let stamped_bounds = curve_bounds(&stamped, "line");
    assert!((start_bounds.height() - middle_bounds.height()).abs() < 0.1);
    assert!(
        stamped_bounds.height() > middle_bounds.height() * 1.7,
        "the keyframe stamp must resample the captured slope: middle={middle_bounds:?}, stamp={stamped_bounds:?}"
    );
    assert_eq!(stamped_bounds, curve_bounds(&after, "line"));
}

#[test]
fn independent_component_parameters_follow_distinct_variable_tracks() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (900, 420), duration: 2 }

pub component Lane(p: Num, y: Num, tint: Color) {
  graph: Graph, at: (225, y), size: (360, 300),
    x_domain: (0, 1), y_domain: (0, 1),
    grid: false, ticks: false, tick_labels: "none" {
    line: PlotCurve, kind: "cartesian", func: (x) => p * x,
      stroke: tint, stroke_width: 4
    marker: Ellipse, at: (0, 0), size: (14, 14), color: tint
  }
  always { marker.at = graph.map(1, p) }
}

#0s
let p_a = 0.2
let p_b = 0.8
lane_a: Lane, p: p_a, y: 210, tint: accent.info
lane_b: Lane, p: p_b, y: 210, tint: accent.warning

#2s
let p_a = 0.4
let p_b = 0.6
"#,
    );

    // No runtime_inputs whitelist is declared: these are ordinary component
    // arguments captured by a procedural curve and an always modifier. Each
    // expansion must retain its own keyframed variable rather than capturing
    // one shared/mutated value.
    assert!(timeline.runtime_input_names().is_empty());
    let start = frame(&timeline, 0.0);
    let keyframe = frame(&timeline, 2.0);
    let a_start = curve_bounds(&start, "lane_a.line");
    let b_start = curve_bounds(&start, "lane_b.line");
    let a_end = curve_bounds(&keyframe, "lane_a.line");
    let b_end = curve_bounds(&keyframe, "lane_b.line");
    let a_marker_start = center(bounds(&start, "lane_a.marker")).1;
    let b_marker_start = center(bounds(&start, "lane_b.marker")).1;
    let a_marker_end = center(bounds(&keyframe, "lane_a.marker")).1;
    let b_marker_end = center(bounds(&keyframe, "lane_b.marker")).1;

    assert!(
        a_end.height() > a_start.height() * 1.7,
        "lane_a should follow p_a from 0.2 to 0.4: start={a_start:?}, end={a_end:?}"
    );
    assert!(
        b_start.height() > b_end.height() * 1.2,
        "lane_b should independently follow p_b from 0.8 to 0.6: start={b_start:?}, end={b_end:?}"
    );
    assert!(
        a_marker_end < a_marker_start - 40.0,
        "lane_a marker should move up with its own parameter: start={a_marker_start}, end={a_marker_end}"
    );
    assert!(
        b_marker_end > b_marker_start + 40.0,
        "lane_b marker should move down with its own parameter: start={b_marker_start}, end={b_marker_end}"
    );
}

#[test]
fn probability_bar_keeps_its_zero_and_formats_the_visible_fraction() {
    let timeline = build(&format!(
        r#"
        {}
        let progress = 0
        always {{ progress = clamp(t / 2, 0, 1) }}
        row: ProbabilityBar, label: "Cat", value: 0.7864, progress: progress, x: 300, y: 250, width: 500
        zero: Text, text: "0.00%", at: (820, 250), font_family: "mono", font_size: 23, text_anchor: "left", opacity: 1
        half: Text, text: "39.32%", at: (820, 250), font_family: "mono", font_size: 23, text_anchor: "left", opacity: 1
        full: Text, text: "78.64%", at: (820, 250), font_family: "mono", font_size: 23, text_anchor: "left", opacity: 1
        "#,
        include_str!("../../../examples/lib/probability.amx"),
    ));
    for (time, end_x, reference) in [
        (0.0, 300.0, "zero"),
        (1.0, 496.6, "half"),
        (2.0, 693.2, "full"),
    ] {
        let program = frame(&timeline, time);
        let segment = curve_bounds(&program, "row.bar");
        close(segment.x0, 300.0, "fixed baseline");
        close(segment.x1, end_x, "probability endpoint");
        let percentage = text_outline_signature(&program, "row.percent");
        assert!(!percentage.is_empty());
        assert_eq!(percentage, text_outline_signature(&program, reference));
    }
}
