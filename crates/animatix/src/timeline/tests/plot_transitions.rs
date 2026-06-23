use super::*;
use crate::ast::Expr;
use crate::easing::Easing;
use crate::timeline::plot::{FuncSource, FuncTransition, PlotCurveKind, ProceduralPlot,
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
        from: Box::new(FuncSource::Raw(vec!["x".to_string()], sin_x_expr())),
        to: Box::new(FuncSource::Raw(vec!["x".to_string()], cos_x_expr())),
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
        t_domain: [0.0, std::f64::consts::TAU],
        tolerance: 4.0,
        max_depth: 5,
        resolution: 16,
        stroke_width: 2.0,
        stroke_color: [1.0, 1.0, 1.0, 1.0],
        params: vec![],
    };

    let transition = FuncTransition {
        start_ms: 2000,
        end_ms: 3000,
        easing: Easing::Linear,
        from: FuncSource::Raw(vec!["x".to_string()], sin_x_expr()),
        to: FuncSource::Raw(vec!["x".to_string()], cos_x_expr()),
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
