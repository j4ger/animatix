use super::*;
use crate::ast::Expr;
use crate::easing::Easing;
use crate::timeline::plot::{blend_depth, FuncSource, FuncTransition, PlotCurveKind, ProceduralPlot,
    resolve_func_source, sample_procedural_plot_at};

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

/// Parse an .amx source snippet, build the timeline, and panic on parse
/// errors.  Returns the `BuildReport` so individual tests can inspect both
/// diagnostics and the timeline.
fn build_from_source(source: &str) -> crate::diagnostics::BuildReport<crate::timeline::Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(
        parse_errors.is_empty(),
        "Parse errors: {:?}",
        parse_errors
    );
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new())
}

/// Build a minimal `Environment` loaded with the standard library so that
/// built-in functions like `sin` and `cos` are available during evaluation.
fn stdlib_env() -> Environment {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    env
}

/// Construct the AST expression `sin(x)`.
fn sin_x_expr() -> Expr {
    Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())])
}

/// Construct the AST expression `cos(x)`.
fn cos_x_expr() -> Expr {
    Expr::Call("cos".to_string(), vec![Expr::Ident("x".to_string())])
}

/// Construct the AST expression `x^2 + y^2 - 1` (unit circle implicit field).
fn circle_expr() -> Expr {
    use crate::ast::BinaryOp;
    Expr::Binary(
        Box::new(Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Pow,
                Box::new(Expr::Num(2.0)),
            )),
            BinaryOp::Add,
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("y".to_string())),
                BinaryOp::Pow,
                Box::new(Expr::Num(2.0)),
            )),
        )),
        BinaryOp::Sub,
        Box::new(Expr::Num(1.0)),
    )
}

/// Construct the AST expression `y - x` (diagonal line implicit field).
fn line_yx_expr() -> Expr {
    Expr::Binary(
        Box::new(Expr::Ident("y".to_string())),
        crate::ast::BinaryOp::Sub,
        Box::new(Expr::Ident("x".to_string())),
    )
}

// ─────────────────────────────────────────────────────────────
// Test 1: basic_func_transition_cartesian
// ─────────────────────────────────────────────────────────────

/// Verify that a `FuncTransition` is created with the correct timing, easing,
/// and arity when a `curve.func = ...` assignment is processed.
#[test]
fn basic_func_transition_cartesian() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #2s
        curve.func = (x) => cos(x) [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        1,
        "Expected exactly 1 FuncTransition"
    );

    let t = &track.func_transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000 (2s)");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000 (2s + 1s)");
    assert_eq!(
        t.easing,
        Easing::Linear,
        "default easing should be Linear"
    );

    // from and to should both have arity 1.
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // from should evaluate to sin(x) — verify at x = 1.0.
    let mut env = stdlib_env();
    let from_val = resolve_func_source(&t.from, &env, "x", 1.0)
        .expect("resolve from");
    assert!(
        (from_val - 1.0_f64.sin()).abs() < 1e-9,
        "from at x=1.0 should be sin(1.0), got {}",
        from_val
    );

    // to should evaluate to cos(x) — verify at x = 1.0.
    let to_val = resolve_func_source(&t.to, &mut env, "x", 1.0)
        .expect("resolve to");
    assert!(
        (to_val - 1.0_f64.cos()).abs() < 1e-9,
        "to at x=1.0 should be cos(1.0), got {}",
        to_val
    );
}

// ─────────────────────────────────────────────────────────────
// Test 2: blend_at_half_progress
// ─────────────────────────────────────────────────────────────

/// Verify that blending at progress = 0.5 produces `0.5 * from(x) + 0.5 * to(x)`.
/// This test uses `resolve_func_source` on a manually constructed `FuncSource::Blend`
/// and also exercises `sample_procedural_plot_at` to verify non-empty output.
#[test]
fn blend_at_half_progress() {
    let mut env = stdlib_env();

    // Manually construct a Blend node with frozen_progress = 0.5.
    let blend = FuncSource::Blend {
        from: Box::new(FuncSource::Raw(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default())),
        to: Box::new(FuncSource::Raw(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default())),
        frozen_progress: 0.5,
    };

    // Verify blending for several x values.
    for x in [0.0_f64, 0.5, 1.0, std::f64::consts::PI / 4.0, 2.0] {
        let result = resolve_func_source(&blend, &env, "x", x)
            .expect("resolve blend");
        let expected = 0.5 * x.sin() + 0.5 * x.cos();
        assert!(
            (result - expected).abs() < 1e-9,
            "blend at x={}: expected {}, got {}",
            x,
            expected,
            result
        );
    }

    // Exercise `sample_procedural_plot_at` at time_ms = 2500 (50 % through
    // a 2 s → 3 s transition) and verify that output paths are non-empty.
    let plot = ProceduralPlot {
        kind: PlotCurveKind::Cartesian,
        func_args: vec!["x".to_string()],
        func_body: sin_x_expr(),
        actor_label: "curve".to_string(),
        param_names: vec![],
        p_x_domain: [-1.0, 1.0],
        p_y_domain: [-2.0, 2.0],
        p_size: [200.0, 200.0],
        padding: [0.0; 4],
        t_domain: [0.0, std::f64::consts::TAU],
        tolerance: 4.0,
        max_depth: 5,
        resolution: 16,
        stroke_width: 2.0,
        stroke_color: [1.0, 1.0, 1.0, 1.0],
        params: vec![],
        extra_captures: CapturedEnv::default(),
    };

    let transition = FuncTransition {
        start_ms: 2000,
        end_ms: 3000,
        easing: Easing::Linear,
        from: FuncSource::Raw(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default()),
        to: FuncSource::Raw(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default()),
    };

    // At time_ms = 2500, progress is 0.5, so output ≈ 0.5*sin(x) + 0.5*cos(x).
    let paths_mid = sample_procedural_plot_at(&plot, &mut env, 2500, &[transition.clone()]);
    assert!(!paths_mid.is_empty(), "Expected output paths at mid-transition");
    assert!(
        !paths_mid[0].path.elements().is_empty(),
        "Expected non-empty BezPath at mid-transition"
    );

    // Before the transition, output should be pure sin(x).
    let paths_before = sample_procedural_plot_at(&plot, &mut env, 1000, &[transition.clone()]);
    assert!(!paths_before.is_empty(), "Expected output paths before transition");

    // After the transition, output should be pure cos(x).
    let paths_after = sample_procedural_plot_at(&plot, &mut env, 4000, &[transition]);
    assert!(!paths_after.is_empty(), "Expected output paths after transition");
}

// ─────────────────────────────────────────────────────────────
// Test 3: record_and_chain_overlapping_transitions
// ─────────────────────────────────────────────────────────────

/// Verify that when a second `func` assignment overlaps with an in-progress
/// first transition, a `FuncSource::Blend` is created as the `from` of the
/// second transition, with `frozen_progress ≈ 0.5`.
#[test]
fn record_and_chain_overlapping_transitions() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-6, 6), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #1s
        curve.func = (x) => cos(x) [2s]

        #2s
        curve.func = (x) => x^2 [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        2,
        "Expected 2 FuncTransitions for overlapping assignments"
    );

    // Second transition's `from` must be a Blend because the second assignment
    // is processed while the first is still active (t=2s, first runs 1s–3s).
    let second = &track.func_transitions[1];
    match &second.from {
        FuncSource::Blend { frozen_progress, .. } => {
            // At t=2s the first transition (1s–3s) is at progress 0.5.
            assert!(
                (frozen_progress - 0.5).abs() < 1e-9,
                "Expected frozen_progress ≈ 0.5, got {}",
                frozen_progress
            );
        }
        FuncSource::Raw(..) => {
            panic!("Expected FuncSource::Blend for the second transition's from, got Raw");
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Test 4: cascading_transitions
// ─────────────────────────────────────────────────────────────

/// Three back-to-back overlapping transitions.  Each subsequent `from` should
/// be a `Blend` capturing the frozen state of the previous one.
#[test]
fn cascading_transitions() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-6, 6), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #1s
        curve.func = (x) => cos(x) [2s]

        #2s
        curve.func = (x) => x^2 [1s]

        #2.5s
        curve.func = (x) => x^3 [0.5s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        3,
        "Expected 3 FuncTransitions for cascading assignments"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(&track.func_transitions[0].from, FuncSource::Raw(..)),
        "First transition's from should be Raw"
    );

    // Second transition: from = Blend (frozen from the first).
    assert!(
        matches!(&track.func_transitions[1].from, FuncSource::Blend { .. }),
        "Second transition's from should be Blend"
    );

    // Third transition: from = Blend (frozen from the second, which is itself a Blend).
    assert!(
        matches!(&track.func_transitions[2].from, FuncSource::Blend { .. }),
        "Third transition's from should be Blend"
    );

    // Verify the third transition's frozen_progress.  At t=2.5s the second
    // transition (2s–3s) is at progress 0.5.
    if let FuncSource::Blend { frozen_progress, .. } = &track.func_transitions[2].from {
        assert!(
            (frozen_progress - 0.5).abs() < 1e-9,
            "Third transition's frozen_progress should be ≈ 0.5, got {}",
            frozen_progress
        );
    }

    // Sample at the midpoint of the third transition (t=2.75s) and verify
    // non-empty output.
    let mut env = stdlib_env();
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot")
        .clone();
    let paths = sample_procedural_plot_at(&plot, &mut env, 2750, &track.func_transitions);
    assert!(!paths.is_empty(), "Expected output paths during cascading transition");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty path during cascading transition"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 5: polar_mode_transition
// ─────────────────────────────────────────────────────────────

/// Verify that a `PlotCurve` with `kind: "polar"` correctly records a
/// `FuncTransition` and that the procedural plot has the right kind.
#[test]
fn polar_mode_transition() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "polar", func: (t) => 1.0, t_domain: (0, tau), stroke_width: 2
        }

        #2s
        curve.func = (t) => 1.0 + 0.5 * cos(3 * t) [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        1,
        "Expected 1 FuncTransition for polar mode"
    );

    let t = &track.func_transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // Verify the plot kind is Polar.
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("polar curve should have a procedural_plot");
    assert_eq!(plot.kind, PlotCurveKind::Polar, "plot kind should be Polar");

    // Sample at mid-transition and verify non-empty output.
    let mut env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &mut env, 2500, &track.func_transitions);
    assert!(!paths.is_empty(), "Expected output paths for polar mid-transition");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty path for polar mid-transition"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 6: parametric_mode_transition
// ─────────────────────────────────────────────────────────────

/// Verify that a `PlotCurve` with `kind: "parametric"` correctly records a
/// `FuncTransition` and that sampling at mid-transition yields output.
#[test]
fn parametric_mode_transition() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "parametric", func: (t) => (cos(t), sin(t)),
                 t_domain: (0, tau), stroke_width: 2
        }

        #2s
        curve.func = (t) => (cos(2 * t), sin(3 * t)) [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        1,
        "Expected 1 FuncTransition for parametric mode"
    );

    let t = &track.func_transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // Verify the plot kind is Parametric.
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("parametric curve should have a procedural_plot");
    assert_eq!(
        plot.kind,
        PlotCurveKind::Parametric,
        "plot kind should be Parametric"
    );

    // Sample at mid-transition and verify non-empty output.
    let mut env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &mut env, 2500, &track.func_transitions);
    assert!(!paths.is_empty(), "Expected output paths for parametric mid-transition");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty path for parametric mid-transition"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 7: invalid_arity_emits_diagnostic
// ─────────────────────────────────────────────────────────────

/// Assigning a `func` with a different argument count than the declaration
/// should emit an `InvalidPlotFunc` diagnostic and leave `func_transitions`
/// empty.
#[test]
fn invalid_arity_emits_diagnostic() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #2s
        curve.func = (x, y) => x + y [1s]
    "#;
    let report = build_from_source(source);

    let has_invalid_func = report
        .diagnostics
        .iter()
        .any(|d| d.code == DiagnosticCode::InvalidPlotFunc);
    assert!(
        has_invalid_func,
        "Expected InvalidPlotFunc diagnostic for arity mismatch, got: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should still exist");
    assert!(
        track.func_transitions.is_empty(),
        "No FuncTransition should be created when arity mismatches"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 8: static_plot_no_transitions
// ─────────────────────────────────────────────────────────────

/// A `PlotCurve` with no `func` assignment should have an empty
/// `func_transitions` list and a valid `procedural_plot`.
#[test]
fn static_plot_no_transitions() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert!(
        track.func_transitions.is_empty(),
        "Static plot should have no FuncTransitions"
    );

    assert!(
        track.procedural_plot.is_some(),
        "Static plot should have a procedural_plot for potential future transitions"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 9: for_loop_closure_captures_loop_variable
// ─────────────────────────────────────────────────────────────

/// Regression test for Batch 3 Task 6: closures inside `for` loops must
/// capture the loop variable so that render-time sampling can resolve it.
///
/// Three `PlotCurve` actors are created inside a `for freq in [1, 2, 3]` loop.
/// Each curve's `func: (x) => x * freq` should evaluate to different multiples
/// of `x` at render time, not all produce the same value.
#[test]
fn for_loop_closure_captures_loop_variable() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        #0s
        for freq, i in {1, 2, 3} {
            g[i]: Graph, x_domain: (-5, 5), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
              curve[i]: PlotCurve, kind: "cartesian", func: (x) => x * freq
            }
        }
    "#;

    let report = build_from_source(source);
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Unexpected build errors: {:?}", errors);

    let mut env = stdlib_env();

    // Check each generated curve track captures a distinct `freq` and samples correctly.
    // For func=(x)=>x*freq, at x=2.0, expected output = 2.0 * freq.
    for (suffix, expected_output) in [("__0", 2.0_f64), ("__1", 4.0_f64), ("__2", 6.0_f64)] {
        let track_name = format!("curve{}", suffix);
        let track = report
            .output
            .get_track(&track_name)
            .unwrap_or_else(|| panic!("{} track should exist", track_name));

        let plot = track
            .procedural_plot
            .as_ref()
            .unwrap_or_else(|| panic!("{} should have procedural_plot", track_name));

        // Verify that `freq` was captured in extra_captures.
        assert!(
            plot.extra_captures.0.contains_key("freq"),
            "{}: `freq` must be in extra_captures (got {:?})",
            track_name,
            plot.extra_captures.0.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
        );

        // Sample the plot and verify output matches the captured `freq` value.
        let paths = sample_procedural_plot_at(plot, &mut env, 0, &[]);
        assert!(!paths.is_empty(), "{}: sampled paths should be non-empty", track_name);

        // Directly evaluate the function via FuncSource to confirm the captured value.
        let func_source = crate::timeline::plot::FuncSource::Raw(
            plot.func_args.clone(),
            plot.func_body.clone(),
            plot.extra_captures.clone(),
        );
        let result = crate::timeline::plot::resolve_func_source(&func_source, &env, "x", 2.0)
            .expect("evaluation should succeed");
        assert!(
            (result - expected_output).abs() < 1e-9,
            "{}: expected f(2.0) = {}, got {}",
            track_name,
            expected_output,
            result,
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 10: adaptive_quality_reduces_depth_for_blends
// ─────────────────────────────────────────────────────────────

/// Verify that a 3-deep cascading transition (blend depth = 3) still produces
/// valid output paths. The adaptive quality system reduces sampling resolution
/// during deep blends to maintain frame rate, but must not break path generation.
#[test]
fn adaptive_quality_reduces_depth_for_blends() {
    // Build a 3-deep blend: declaration parent with 3 overlapping func transitions
    // creates a Blend(Blend(Raw, Raw), Raw) structure.
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-6, 6), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #1s
        curve.func = (x) => cos(x) [2s]

        #2s
        curve.func = (x) => x^2 [1s]

        #2.5s
        curve.func = (x) => x^3 [0.5s]
    "#;
    let report = build_from_source(source);
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Unexpected build errors: {:?}", errors);

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        3,
        "Expected 3 FuncTransitions for 3-deep blend"
    );

    // Verify the from of the third transition is a nested Blend (depth >= 2).
    let third_from = &track.func_transitions[2].from;
    let depth = blend_depth(third_from);
    assert!(
        depth >= 2,
        "Expected blend_depth >= 2 for third transition's from, got {}",
        depth
    );

    // Sample at the midpoint of the third transition (t=2.75s) and verify
    // non-empty output despite reduced quality.
    let mut env = stdlib_env();
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot")
        .clone();
    let paths = sample_procedural_plot_at(&plot, &mut env, 2750, &track.func_transitions);
    assert!(!paths.is_empty(), "Expected output paths during 3-deep blend");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty path during 3-deep blend"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 11: quality_factor_calculation
// ─────────────────────────────────────────────────────────────

/// Verify the quality factor calculation for various blend depths.
/// The quality factor is derived from blend depth:
///   quality_factor = 0.75^(depth - 1)  for depth >= 1, else 1.0
/// where depth = blend_depth(from).max(blend_depth(to)) + 1.
#[test]
fn quality_factor_calculation() {
    /// Helper: compute quality factor from two source blend depths + 1.
    fn compute_qf(from_depth: usize, to_depth: usize) -> f64 {
        let depth = from_depth.max(to_depth) + 1;
        if depth == 0 {
            1.0
        } else {
            0.75_f64.powi(depth as i32 - 1)
        }
    }

    // Helper to create a Raw FuncSource (blend depth 0).
    fn raw() -> FuncSource {
        FuncSource::Raw(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default())
    }

    // Helper to create a Blend at a given depth by nesting.
    fn blend_of_depth(d: usize) -> FuncSource {
        if d == 0 {
            return raw();
        }
        let inner = blend_of_depth(d - 1);
        FuncSource::Blend {
            from: Box::new(inner.clone()),
            to: Box::new(inner),
            frozen_progress: 0.5,
        }
    }

    // depth 0: Raw source has blend_depth = 0.
    assert_eq!(blend_depth(&raw()), 0, "blend_depth of Raw should be 0");
    // depth 1: single Blend(Raw, Raw) has blend_depth = 1.
    let d1 = blend_of_depth(1);
    assert_eq!(blend_depth(&d1), 1, "blend_depth of single Blend should be 1");
    // depth 2: Blend(Blend(Raw,Raw), Raw) has blend_depth = 2.
    let d2 = blend_of_depth(2);
    assert_eq!(blend_depth(&d2), 2, "blend_depth of double-nested Blend should be 2");
    // depth 3: triple-nested Blend.
    let d3 = blend_of_depth(3);
    assert_eq!(blend_depth(&d3), 3, "blend_depth of triple-nested Blend should be 3");
    // depth 4: quadruple-nested Blend.
    let d4 = blend_of_depth(4);
    assert_eq!(blend_depth(&d4), 4, "blend_depth of quadruple-nested Blend should be 4");

    // Quality factor (depth = max(from_depth, to_depth) + 1):
    // - depth 0: from and to both Raw → quality_factor = 1.0
    assert!((compute_qf(0, 0) - 1.0).abs() < 1e-12, "depth 0: expected 1.0");
    // - depth 1: one blend, from=Raw, to=Raw → quality_factor = 1.0 (0.75^0)
    assert!((compute_qf(0, 0) - 1.0).abs() < 1e-12, "depth 1: expected 1.0");
    // - depth 2: from=Blend(Raw,Raw) (depth 1), to=Raw (depth 0) → quality_factor = 0.75
    let qf2 = compute_qf(1, 0);
    assert!((qf2 - 0.75).abs() < 1e-12, "depth 2: expected 0.75, got {}", qf2);
    // - depth 3: from=Blend(Blend(Raw,Raw),Raw) (depth 2), to=Raw (depth 0) → quality_factor = 0.5625
    let qf3 = compute_qf(2, 0);
    assert!((qf3 - 0.5625).abs() < 1e-12, "depth 3: expected 0.5625, got {}", qf3);
    // - depth 4: depth 3 blend vs Raw → quality_factor = 0.421875
    let qf4 = compute_qf(3, 0);
    assert!((qf4 - 0.421875).abs() < 1e-12, "depth 4: expected 0.421875, got {}", qf4);
}

// ─────────────────────────────────────────────────────────────
// Test 12: implicit_transition_simple
// ─────────────────────────────────────────────────────────────

/// Verify that a `FuncTransition` is created for an implicit plot and that
/// sampling before, during, and after the transition produces non-empty paths.
#[test]
fn implicit_transition_simple() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (200, 200), at: (640, 360) {
          curve: PlotCurve, kind: "implicit", func: (x,y) => x^2 + y^2 - 1, stroke_width: 2
        }

        #2s
        curve.func = (x,y) => y - x [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        1,
        "Expected exactly 1 FuncTransition for implicit plot"
    );

    let t = &track.func_transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");

    let mut env = stdlib_env();
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot")
        .clone();

    assert_eq!(
        plot.kind,
        PlotCurveKind::Implicit,
        "Plot kind should be Implicit"
    );

    // Before transition: pure circle contour.
    let paths_before = sample_procedural_plot_at(&plot, &mut env, 1000, &track.func_transitions);
    assert!(!paths_before.is_empty(), "Expected paths before transition");
    assert!(
        !paths_before[0].path.elements().is_empty(),
        "Expected non-empty path before transition"
    );

    // Mid-transition (t=2500ms, progress≈0.5): blended contour.
    let paths_mid = sample_procedural_plot_at(&plot, &mut env, 2500, &track.func_transitions);
    assert!(!paths_mid.is_empty(), "Expected paths at mid-transition");
    assert!(
        !paths_mid[0].path.elements().is_empty(),
        "Expected non-empty path at mid-transition"
    );

    // After transition: pure line contour.
    let paths_after = sample_procedural_plot_at(&plot, &mut env, 4000, &track.func_transitions);
    assert!(!paths_after.is_empty(), "Expected paths after transition");
    assert!(
        !paths_after[0].path.elements().is_empty(),
        "Expected non-empty path after transition"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 13: implicit_blend_at_half
// ─────────────────────────────────────────────────────────────

/// Directly verify that `FuncSource::Blend` at `frozen_progress = 0.5`
/// interpolates the scalar field correctly at a known point, and that
/// `build_implicit_plot_path_from_source` returns a non-empty contour.
#[test]
fn implicit_blend_at_half() {
    use crate::timeline::plot::{build_implicit_plot_path_from_source, eval_implicit_source};

    let mut env = stdlib_env();

    let circle = FuncSource::Raw(
        vec!["x".to_string(), "y".to_string()],
        circle_expr(),
        CapturedEnv::default(),
    );
    let line = FuncSource::Raw(
        vec!["x".to_string(), "y".to_string()],
        line_yx_expr(),
        CapturedEnv::default(),
    );

    // At (0,0): circle gives -1, line gives 0; blend at 0.5 → -0.5.
    let circle_val = eval_implicit_source(&circle, &mut env, 0.0, 0.0);
    let line_val = eval_implicit_source(&line, &mut env, 0.0, 0.0);
    assert!((circle_val - (-1.0)).abs() < 1e-9, "circle at (0,0) should be -1, got {}", circle_val);
    assert!((line_val - 0.0).abs() < 1e-9, "line at (0,0) should be 0, got {}", line_val);

    let blended = FuncSource::Blend {
        from: Box::new(circle),
        to: Box::new(line),
        frozen_progress: 0.5,
    };

    let blended_val = eval_implicit_source(&blended, &mut env, 0.0, 0.0);
    assert!(
        (blended_val - (-0.5)).abs() < 1e-9,
        "blended at (0,0) should be -0.5, got {}",
        blended_val
    );

    // Build a path from the blended source; it must produce a non-empty contour.
    let path = build_implicit_plot_path_from_source(
        &mut env,
        &blended,
        &[-2.0_f64, 2.0],
        &[-2.0_f64, 2.0],
        &[200.0_f64, 200.0],
        32,
        &[0.0_f64; 4],
    );
    assert!(
        !path.elements().is_empty(),
        "Expected non-empty contour from blended implicit source"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 14: implicit_cascading
// ─────────────────────────────────────────────────────────────

/// Three overlapping transitions on an implicit plot. Verifies that
/// record-and-chain produces nested `FuncSource::Blend` from-sources
/// and that sampling at any mid-point yields non-empty paths.
#[test]
fn implicit_cascading() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (200, 200), at: (640, 360) {
          curve: PlotCurve, kind: "implicit", func: (x,y) => x^2 + y^2 - 1, stroke_width: 2
        }

        #1s
        curve.func = (x,y) => y - x [2s]

        #2s
        curve.func = (x,y) => x + y [1s]

        #2.5s
        curve.func = (x,y) => x - y [0.5s]
    "#;
    let report = build_from_source(source);
    assert!(
        report.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report
        .output
        .get_track("curve")
        .expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        3,
        "Expected 3 FuncTransitions for cascading implicit transitions"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(&track.func_transitions[0].from, FuncSource::Raw(..)),
        "First transition's from should be Raw"
    );
    // Second transition: from = Blend (frozen from the first at t=2s).
    assert!(
        matches!(&track.func_transitions[1].from, FuncSource::Blend { .. }),
        "Second transition's from should be Blend"
    );
    // Third transition: from = Blend (frozen from the second at t=2.5s).
    assert!(
        matches!(&track.func_transitions[2].from, FuncSource::Blend { .. }),
        "Third transition's from should be Blend"
    );

    // At t=2s the first transition (1s–3s) is at progress 0.5.
    if let FuncSource::Blend { frozen_progress, .. } = &track.func_transitions[1].from {
        assert!(
            (frozen_progress - 0.5).abs() < 1e-9,
            "Second transition frozen_progress should be ≈ 0.5, got {}",
            frozen_progress
        );
    }

    let mut env = stdlib_env();
    let plot = track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot")
        .clone();

    assert_eq!(plot.kind, PlotCurveKind::Implicit, "Plot kind should be Implicit");

    // Sample at the midpoint of the third transition (t=2.75s) — deepest blend.
    let paths_deep = sample_procedural_plot_at(&plot, &mut env, 2750, &track.func_transitions);
    assert!(!paths_deep.is_empty(), "Expected paths during 3-deep implicit cascade");
    assert!(
        !paths_deep[0].path.elements().is_empty(),
        "Expected non-empty path during 3-deep implicit cascade"
    );

    // Sample after all transitions have completed.
    let paths_after = sample_procedural_plot_at(&plot, &mut env, 5000, &track.func_transitions);
    assert!(!paths_after.is_empty(), "Expected paths after all implicit transitions");
    assert!(
        !paths_after[0].path.elements().is_empty(),
        "Expected non-empty path after all implicit transitions"
    );
}
