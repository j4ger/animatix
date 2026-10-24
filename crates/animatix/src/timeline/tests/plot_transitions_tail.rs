//! Plot-transition tail tests (implicit/staged transitions, vector/heatmap/contour).

use super::plot_transitions::*;
use super::*;
use crate::primitives::PrimitiveRegistry;
use crate::timeline::CapturedEnv;
use crate::timeline::plot::{
    FuncBlendMode, FuncSource, PlotCurveKind, ProceduralPlotKind, blend_depth, resolve_func_source,
    sample_procedural_plot_at,
};
use std::sync::Arc;

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
        FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default())
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
    // - depth 3: from=Blend(Blend(Raw,Raw),Raw) (depth 2), to=Raw (depth 0) → quality_factor =
    //   0.5625
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "Expected exactly 1 FuncTransition for implicit plot"
    );

    let t = &report.output.world.func_transitions(track.id)[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");

    let env = stdlib_env();
    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("curve should have a procedural_plot")
        .clone();

    assert_eq!(plot.kind, PlotCurveKind::Implicit, "Plot kind should be Implicit");

    // Before transition: pure circle contour.
    let paths_before = sample_procedural_plot_at(
        &plot,
        &env,
        1000,
        report.output.world.func_transitions(track.id),
    );
    assert!(!paths_before.is_empty(), "Expected paths before transition");
    assert!(
        !paths_before[0].path.elements().is_empty(),
        "Expected non-empty path before transition"
    );

    // Mid-transition (t=2500ms, progress≈0.5): blended contour.
    let paths_mid = sample_procedural_plot_at(
        &plot,
        &env,
        2500,
        report.output.world.func_transitions(track.id),
    );
    assert!(!paths_mid.is_empty(), "Expected paths at mid-transition");
    assert!(
        !paths_mid[0].path.elements().is_empty(),
        "Expected non-empty path at mid-transition"
    );

    // After transition: pure line contour.
    let paths_after = sample_procedural_plot_at(
        &plot,
        &env,
        4000,
        report.output.world.func_transitions(track.id),
    );
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

    let circle = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        circle_expr(),
        CapturedEnv::default(),
    );
    let line = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        line_yx_expr(),
        CapturedEnv::default(),
    );

    // At (0,0): circle gives -1, line gives 0; blend at 0.5 → -0.5.
    let circle_val = eval_implicit_source(&circle, &mut env, 0.0, 0.0);
    let line_val = eval_implicit_source(&line, &mut env, 0.0, 0.0);
    assert!(
        (circle_val - (-1.0)).abs() < 1e-9,
        "circle at (0,0) should be -1, got {}",
        circle_val
    );
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        3,
        "Expected 3 FuncTransitions for cascading implicit transitions"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(
            &report.output.world.func_transitions(track.id)[0].from,
            FuncSource::Compiled(..)
        ),
        "First transition's from should be Raw"
    );
    // Second transition: from = Blend (frozen from the first at t=2s).
    assert!(
        matches!(
            &report.output.world.func_transitions(track.id)[1].from,
            FuncSource::Blend { .. }
        ),
        "Second transition's from should be Blend"
    );
    // Third transition: from = Blend (frozen from the second at t=2.5s).
    assert!(
        matches!(
            &report.output.world.func_transitions(track.id)[2].from,
            FuncSource::Blend { .. }
        ),
        "Third transition's from should be Blend"
    );

    // At t=2s the first transition (1s–3s) is at progress 0.5.
    if let FuncSource::Blend {
        frozen_progress, ..
    } = &report.output.world.func_transitions(track.id)[1].from
    {
        assert!(
            (frozen_progress - 0.5).abs() < 1e-9,
            "Second transition frozen_progress should be ≈ 0.5, got {}",
            frozen_progress
        );
    }

    let env = stdlib_env();
    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("curve should have a procedural_plot")
        .clone();

    assert_eq!(plot.kind, PlotCurveKind::Implicit, "Plot kind should be Implicit");

    // Sample at the midpoint of the third transition (t=2.75s) — deepest blend.
    let paths_deep = sample_procedural_plot_at(
        &plot,
        &env,
        2750,
        report.output.world.func_transitions(track.id),
    );
    assert!(!paths_deep.is_empty(), "Expected paths during 3-deep implicit cascade");
    assert!(
        !paths_deep[0].path.elements().is_empty(),
        "Expected non-empty path during 3-deep implicit cascade"
    );

    // Sample after all transitions have completed.
    let paths_after = sample_procedural_plot_at(
        &plot,
        &env,
        5000,
        report.output.world.func_transitions(track.id),
    );
    assert!(!paths_after.is_empty(), "Expected paths after all implicit transitions");
    assert!(
        !paths_after[0].path.elements().is_empty(),
        "Expected non-empty path after all implicit transitions"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 23: vector_field_func_transition
// ─────────────────────────────────────────────────────────────

/// Verify that VectorField accepts `func` transitions, keeps a procedural
/// plot, and samples both endpoints through the existing side-channel.
#[test]
fn vector_field_func_transition() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        field: VectorField, func: (x, y) => (y, -x), density: 6,
          x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        field.func = (x, y) => (-y, x) [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("field").expect("field track should exist");
    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "Expected 1 FuncTransition for VectorField"
    );
    assert_eq!(
        report.output.world.func_transitions(track.id)[0].blend_mode,
        FuncBlendMode::Output
    );

    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("VectorField should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::VectorField);

    let env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &env, 1500, report.output.world.func_transitions(track.id));
    assert!(!paths.is_empty(), "Expected output paths during VectorField transition");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty VectorField path at mid-transition"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 24: heatmap_func_transition
// ─────────────────────────────────────────────────────────────

/// Verify that Heatmap records and samples `func` transitions.
#[test]
fn heatmap_func_transition() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        heat: Heatmap, func: (x, y) => sin(x) * cos(y), resolution: 8,
          color: red, x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        heat.func = (x, y) => x + y [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("heat").expect("heat track should exist");
    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "Expected 1 FuncTransition for Heatmap"
    );

    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("Heatmap should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::Heatmap);

    let env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &env, 1500, report.output.world.func_transitions(track.id));
    assert!(!paths.is_empty(), "Expected output paths during Heatmap transition");
    assert!(paths.iter().any(|p| p.fill.is_some()), "Expected filled heat cells");
}

// ─────────────────────────────────────────────────────────────
// Test 25: contour_set_func_transition
// ─────────────────────────────────────────────────────────────

/// Verify that ContourSet records and samples `func` transitions.
#[test]
fn contour_set_func_transition() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        cont: ContourSet, func: (x, y) => x^2 + y^2, levels: {1, 2},
          resolution: 16, x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        cont.func = (x, y) => x * y [1s]
    "#;
    let report = build_from_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("cont").expect("cont track should exist");
    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "Expected 1 FuncTransition for ContourSet"
    );

    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("ContourSet should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::ContourSet);

    let env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &env, 1500, report.output.world.func_transitions(track.id));
    assert!(!paths.is_empty(), "Expected output paths during ContourSet transition");
}

// ─────────────────────────────────────────────────────────────
// Test 26: opacity_crossfade_blend_mode
// ─────────────────────────────────────────────────────────────

/// `blend: opacity` records a `FuncBlendMode::Opacity` transition and
/// `sample_procedural_plot_at` renders both endpoint paths at partial opacity.
#[test]
fn opacity_crossfade_blend_mode() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }

        #1s
        curve.func = (x) => cos(x) [1s, blend: opacity]
    "#;
    let report = build_from_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");
    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "Expected 1 FuncTransition"
    );
    assert_eq!(
        report.output.world.func_transitions(track.id)[0].blend_mode,
        FuncBlendMode::Opacity
    );

    let plot = report
        .output
        .world
        .procedural_plot(track.id)
        .expect("curve should have a procedural_plot")
        .clone();
    let env = stdlib_env();
    let paths = sample_procedural_plot_at(
        &plot,
        &env,
        1500,
        report.output.world.func_transitions(track.id),
    );
    assert_eq!(paths.len(), 2, "Opacity cross-fade should render both endpoint paths");
    for path in &paths {
        let (color, _) = path.stroke.expect("curves should be stroked");
        assert!(color.to_rgba8().a > 0 && color.to_rgba8().a < 255, "Expected partial alpha");
    }
}
// ─────────────────────────────────────────────────────────────
// Test 22: depth_4_nested_blend_parity
// ─────────────────────────────────────────────────────────────

/// Deep nesting: 4 levels of `FuncSource::Blend`. Verify evaluation
/// matches the mathematical formula within epsilon.
#[test]
fn depth_4_nested_blend_parity() {
    let env = stdlib_env();

    fn raw_const(val: f64) -> FuncSource {
        FuncSource::from_expr(vec!["x".to_string()], Expr::Num(val), CapturedEnv::default())
    }

    // Build a depth-4 blend tree: blend(blend(blend(blend(A,B,0.9),C,0.7),D,0.5),E,0.3)
    // where A=sin(x), B=cos(x), C=1.0, D=2.0, E=-1.5
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = raw_const(1.0);
    let d = raw_const(2.0);
    let e = raw_const(-1.5);

    // Build bottom-up
    // l1 = blend(A, B, 0.9)
    let l1 = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(b.clone()),
        frozen_progress: 0.9,
    };
    // l2 = blend(l1, C, 0.7)
    let l2 = FuncSource::Blend {
        from: Box::new(l1),
        to: Box::new(c.clone()),
        frozen_progress: 0.7,
    };
    // l3 = blend(l2, D, 0.5)
    let l3 = FuncSource::Blend {
        from: Box::new(l2),
        to: Box::new(d.clone()),
        frozen_progress: 0.5,
    };
    // l4 = blend(l3, E, 0.3)
    let l4 = FuncSource::Blend {
        from: Box::new(l3),
        to: Box::new(e.clone()),
        frozen_progress: 0.3,
    };

    // Compute expected weights:
    // Each level: result = from*(1-p) + to*p
    // Level 1: A*(1-0.9) + B*0.9 = 0.1*A + 0.9*B
    // Level 2: l1*(1-0.7) + C*0.7 = 0.3*l1 + 0.7*C
    //        = 0.3*(0.1*A + 0.9*B) + 0.7*C = 0.03*A + 0.27*B + 0.7*C
    // Level 3: l2*(1-0.5) + D*0.5 = 0.5*l2 + 0.5*D
    //        = 0.5*(0.03*A + 0.27*B + 0.7*C) + 0.5*D
    //        = 0.015*A + 0.135*B + 0.35*C + 0.5*D
    // Level 4: l3*(1-0.3) + E*0.3 = 0.7*l3 + 0.3*E
    //        = 0.7*(0.015*A + 0.135*B + 0.35*C + 0.5*D) + 0.3*E
    //        = 0.0105*A + 0.0945*B + 0.245*C + 0.35*D + 0.3*E
    // At x=1.0:
    // A(1)=sin(1), B(1)=cos(1), C=1.0, D=2.0, E=-1.5
    // expected = 0.0105*sin(1) + 0.0945*cos(1) + 0.245*1.0 + 0.35*2.0 + 0.3*(-1.5)
    //          = 0.0105*sin(1) + 0.0945*cos(1) + 0.245 + 0.7 - 0.45
    //          = 0.0105*sin(1) + 0.0945*cos(1) + 0.495
    let x = 1.0;
    let result = resolve_func_source(&l4, &env, "x", x).expect("depth-4 blend should resolve");
    let expected = 0.0105 * x.sin() + 0.0945 * x.cos() + 0.495;
    assert!(
        (result - expected).abs() < 1e-9,
        "At x={}: expected {}, got {}",
        x,
        expected,
        result
    );

    // Also verify via eval_source_scalar (the inner evaluation path)
    use std::collections::HashMap;
    let mut local_env = stdlib_env();
    let mut cache = HashMap::new();
    let result2 =
        crate::timeline::plot::eval_source_scalar(&l4, &mut local_env, "x", x, &mut cache);
    assert!(
        (result2 - expected).abs() < 1e-9,
        "eval_source_scalar at x={}: expected {}, got {}",
        x,
        expected,
        result2
    );
}

#[test]
fn extension_plot_capability_enables_func_assignments() {
    let (ast, errors) = animatix_syntax::parser::parse_source("p: PlotExt\np.func = (x) => x [1s]");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut registry = PrimitiveRegistry::new();
    let mut plot_info = animatix_std::PrimitiveInfo::extension("PlotExt", ActorCategory::Plot);
    plot_info.capabilities.plot_geometry = true;
    plot_info.capabilities.vector_paths = true;
    registry.register(Arc::new(PlotExt), plot_info).expect("register PlotExt");
    let report = Timeline::build_with_primitive_registry(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(registry),
    );
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let track = report.output.world.tracks.get("p").expect("plot extension track");
    assert_eq!(
        report.output.world.func_transitions(track.id).len(),
        1,
        "plot_geometry capability should enable func transitions"
    );
}
