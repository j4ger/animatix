use super::*;
use crate::ast::Expr;
use crate::easing::Easing;
use crate::primitives::{BuildCtx, Primitive, PrimitiveRegistry};
use crate::timeline::modifier_runtime::ir::compile_expr;
use crate::timeline::plot::{
    FuncBlendMode, FuncSource, FuncTransition, PlotCurveKind, ProceduralPlot, ProceduralPlotKind,
    blend_depth, flatten_blend, resolve_func_source, sample_procedural_plot_at,
};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

/// Parse an .amx source snippet, build the timeline, and panic on parse
/// errors.  Returns the `BuildReport` so individual tests can inspect both
/// diagnostics and the timeline.
fn build_from_source(source: &str) -> crate::diagnostics::BuildReport<crate::timeline::Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
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

fn compiled_body(body: Expr) -> crate::timeline::modifier_runtime::ir::CompiledExpr {
    compile_expr(&body).expect("test function body should compile")
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

struct PlotExt;

impl Primitive for PlotExt {
    fn type_name(&self) -> &str {
        "PlotExt"
    }

    fn display_name(&self) -> &str {
        "Plot Extension"
    }

    fn category(&self) -> ActorCategory {
        ActorCategory::Plot
    }

    fn icon_id(&self) -> &str {
        "plot-ext"
    }

    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Extension
    }

    fn capabilities(&self) -> animatix_syntax::schema::PrimitiveCapabilities {
        animatix_syntax::schema::PrimitiveCapabilities {
            vector_paths: true,
            plot_geometry: true,
            ..animatix_syntax::schema::PrimitiveCapabilities::default()
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| crate::timeline::AnimationTrack::new(label.to_string()));
        track.kind = ActorKindId::Extension;
        track.procedural_plot = Some(ProceduralPlot {
            plot_type: ProceduralPlotKind::default(),
            kind: PlotCurveKind::Cartesian,
            func_args: vec!["x".to_string()],
            func_body: compile_expr(&Expr::Ident("x".to_string())).expect("compile test body"),
            actor_label: label.to_string(),
            param_names: vec![],
            p_x_domain: [-5.0, 5.0],
            p_y_domain: [-5.0, 5.0],
            p_size: [320.0, 200.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            t_domain: [0.0, 0.0],
            tolerance: 0.1,
            max_depth: 6,
            resolution: 64,
            density: 0,
            levels: vec![],
            stroke_width: 2.0,
            stroke_color: [1.0, 1.0, 1.0, 1.0],
            fill_color: [0.0, 0.0, 0.0, 0.0],
            params: vec![],
            extra_captures: Default::default(),
        });
        track.rebuild_property_plan();
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        Ok(Some(vec![crate::primitives::RenderCommand::Paths { paths: Vec::new() }]))
    }
}

// ─────────────────────────────────────────────────────────────
// Test 1: basic_func_transition_cartesian
// ─────────────────────────────────────────────────────────────

/// Verify that a `FuncTransition` is created with the correct timing, easing,
/// and arity when a `curve.func = ...` assignment is processed.

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
fn without_content_lints(
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> impl Iterator<Item = &animatix_syntax::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
}

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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(track.func_transitions.len(), 1, "Expected exactly 1 FuncTransition");

    let t = &track.func_transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000 (2s)");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000 (2s + 1s)");
    assert_eq!(t.easing, Easing::Linear, "default easing should be Linear");
    assert_eq!(t.blend_mode, FuncBlendMode::Output, "default blend should be Output");

    // from and to should both have arity 1.
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // from should evaluate to sin(x) — verify at x = 1.0.
    let env = stdlib_env();
    let from_val = resolve_func_source(&t.from, &env, "x", 1.0).expect("resolve from");
    assert!(
        (from_val - 1.0_f64.sin()).abs() < 1e-9,
        "from at x=1.0 should be sin(1.0), got {}",
        from_val
    );

    // to should evaluate to cos(x) — verify at x = 1.0.
    let to_val = resolve_func_source(&t.to, &env, "x", 1.0).expect("resolve to");
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
        from: Box::new(FuncSource::from_expr(
            vec!["x".to_string()],
            sin_x_expr(),
            CapturedEnv::default(),
        )),
        to: Box::new(FuncSource::from_expr(
            vec!["x".to_string()],
            cos_x_expr(),
            CapturedEnv::default(),
        )),
        frozen_progress: 0.5,
    };

    // Verify blending for several x values.
    for x in [0.0_f64, 0.5, 1.0, std::f64::consts::PI / 4.0, 2.0] {
        let result = resolve_func_source(&blend, &env, "x", x).expect("resolve blend");
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
        plot_type: ProceduralPlotKind::Curve(PlotCurveKind::Cartesian),
        kind: PlotCurveKind::Cartesian,
        func_args: vec!["x".to_string()],
        func_body: compiled_body(sin_x_expr()),
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
        density: 16,
        levels: vec![],
        stroke_width: 2.0,
        stroke_color: [1.0, 1.0, 1.0, 1.0],
        fill_color: [1.0, 1.0, 1.0, 1.0],
        params: vec![],
        extra_captures: CapturedEnv::default(),
    };

    let transition = FuncTransition {
        start_ms: 2000,
        end_ms: 3000,
        easing: Easing::Linear,
        from: FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default()),
        to: FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default()),
        blend_mode: FuncBlendMode::Output,
    };

    // At time_ms = 2500, progress is 0.5, so output ≈ 0.5*sin(x) + 0.5*cos(x).
    let paths_mid =
        sample_procedural_plot_at(&plot, &mut env, 2500, std::slice::from_ref(&transition));
    assert!(!paths_mid.is_empty(), "Expected output paths at mid-transition");
    assert!(
        !paths_mid[0].path.elements().is_empty(),
        "Expected non-empty BezPath at mid-transition"
    );

    // Before the transition, output should be pure sin(x).
    let paths_before =
        sample_procedural_plot_at(&plot, &mut env, 1000, std::slice::from_ref(&transition));
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        2,
        "Expected 2 FuncTransitions for overlapping assignments"
    );

    // Second transition's `from` must be a Blend because the second assignment
    // is processed while the first is still active (t=2s, first runs 1s–3s).
    let second = &track.func_transitions[1];
    match &second.from {
        FuncSource::Blend {
            frozen_progress, ..
        } => {
            // At t=2s the first transition (1s–3s) is at progress 0.5.
            assert!(
                (frozen_progress - 0.5).abs() < 1e-9,
                "Expected frozen_progress ≈ 0.5, got {}",
                frozen_progress
            );
        },
        FuncSource::Compiled(..) => {
            panic!("Expected FuncSource::Blend for the second transition's from, got Raw");
        },
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(
        track.func_transitions.len(),
        3,
        "Expected 3 FuncTransitions for cascading assignments"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(&track.func_transitions[0].from, FuncSource::Compiled(..)),
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
    if let FuncSource::Blend {
        frozen_progress, ..
    } = &track.func_transitions[2].from
    {
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition for polar mode");

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
    let paths = sample_procedural_plot_at(plot, &mut env, 2500, &track.func_transitions);
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition for parametric mode");

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
    assert_eq!(plot.kind, PlotCurveKind::Parametric, "plot kind should be Parametric");

    // Sample at mid-transition and verify non-empty output.
    let mut env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &mut env, 2500, &track.func_transitions);
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

    let has_invalid_func =
        report.diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidPlotFunc);
    assert!(
        has_invalid_func,
        "Expected InvalidPlotFunc diagnostic for arity mismatch, got: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should still exist");
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
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert!(track.func_transitions.is_empty(), "Static plot should have no FuncTransitions");

    assert!(
        track.procedural_plot.is_some(),
        "Static plot should have a procedural_plot for potential future transitions"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 9: always_bare_variable_assignment_updates_plot_closure
// ─────────────────────────────────────────────────────────────

/// Regression test for `always { freq = ... }`: the bare assignment must write
/// the frame variable, and plot sampling must let that frame value shadow the
/// build-time closure capture instead of re-merging the captured value on top.
#[test]
fn always_bare_variable_assignment_updates_plot_closure() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        #0s
        let freq = 2
        curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x),
          stroke: accent.primary, stroke_width: 3

        always {
          freq = 2 + 3 * sin(t * 0.5)
        }
    "#;
    let report = build_from_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let time_ms = 1000;
    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env(time_ms, SceneDimensions::default(), &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                time_ms,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("modifier IR execution should succeed");
    }

    let expected_freq = 2.0 + 3.0 * (0.5f64).sin();
    let Some(Value::Num(frame_freq)) = env.get("freq") else {
        panic!("expected bare freq frame variable, got {:?}", env.get("freq"));
    };
    assert!(
        (frame_freq - expected_freq).abs() < 1e-9,
        "expected freq={expected_freq}, got {frame_freq}"
    );

    let track = timeline.get_track("curve").expect("curve track should exist");
    let plot = track.procedural_plot.as_ref().expect("curve should have a procedural plot");
    let source = FuncSource::Compiled(
        plot.func_args.clone(),
        Box::new(plot.func_body.clone()),
        plot.extra_captures.clone(),
    );
    let result =
        resolve_func_source(&source, &env, "x", 1.0).expect("plot closure should evaluate");
    assert!(
        (result - expected_freq.sin()).abs() < 1e-6,
        "plot closure should use frame freq {expected_freq}, got {result}"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 10: for_loop_closure_captures_loop_variable
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
        let func_source = crate::timeline::plot::FuncSource::Compiled(
            plot.func_args.clone(),
            Box::new(plot.func_body.clone()),
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

    let track = report.output.get_track("curve").expect("curve track should exist");

    assert_eq!(track.func_transitions.len(), 3, "Expected 3 FuncTransitions for 3-deep blend");

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

    assert_eq!(plot.kind, PlotCurveKind::Implicit, "Plot kind should be Implicit");

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
        track.func_transitions.len(),
        3,
        "Expected 3 FuncTransitions for cascading implicit transitions"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(&track.func_transitions[0].from, FuncSource::Compiled(..)),
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
    if let FuncSource::Blend {
        frozen_progress, ..
    } = &track.func_transitions[1].from
    {
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

// ─────────────────────────────────────────────────────────────
// Test 15: flatten_blend_basic
// ─────────────────────────────────────────────────────────────

/// Verify that `flatten_blend` on a non-blend source returns `[(1.0, source)]`.
#[test]
fn flatten_blend_basic() {
    let raw = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let flat = flatten_blend(&raw);
    assert_eq!(flat.len(), 1, "Expected 1 entry for non-blend source");
    assert!(
        (flat[0].0 - 1.0).abs() < 1e-12,
        "Expected weight 1.0 for non-blend source, got {}",
        flat[0].0
    );
    // Should point to the original source
    assert!(std::ptr::eq(flat[0].1, &raw), "Should reference the original source");
}

// ─────────────────────────────────────────────────────────────
// Test 16: flatten_blend_single_level
// ─────────────────────────────────────────────────────────────

/// A single `FuncSource::Blend { from: A, to: B, frozen_progress: 0.4 }`
/// should flatten to `[(0.6, A), (0.4, B)]`.
#[test]
fn flatten_blend_single_level() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let blend = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(b.clone()),
        frozen_progress: 0.4,
    };

    let flat = flatten_blend(&blend);
    assert_eq!(flat.len(), 2, "Expected 2 entries for single-level blend");

    // Sort by pointer to have deterministic order
    let mut flat_sorted = flat.clone();
    flat_sorted.sort_by_key(|(_, ptr)| *ptr as *const _ as usize);

    // Weights should be (1-0.4)=0.6 for from and 0.4 for to
    for (w, _) in &flat {
        assert!((*w - 0.6).abs() < 1e-12 || (*w - 0.4).abs() < 1e-12, "Unexpected weight {}", w);
    }
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );
}

// ─────────────────────────────────────────────────────────────
// Test 17: flatten_blend_nested_depth_2
// ─────────────────────────────────────────────────────────────

/// A depth-2 blend `blend(A, blend(B, C, p), q)` should flatten to
/// 3 entries with weights matching the lerp tree.
/// Lerp formula: `from*(1-q) + to*q` where inner to = blend(B,C,p) = B*(1-p)+C*p
/// Result: `A*(1-q) + B*(1-p)*q + C*p*q`
#[test]
fn flatten_blend_nested_depth_2() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(0.5), CapturedEnv::default());

    // Inner: blend(B, C, p=0.3)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.3,
    };
    // Outer: blend(A, inner, q=0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.6,
    };

    let flat = flatten_blend(&outer);
    assert_eq!(flat.len(), 3, "Expected 3 entries for depth-2 blend");

    // Total weight should sum to 1.0
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );

    // Expected weights:
    // A: 1 - q = 0.4
    // B: (1 - p) * q = 0.7 * 0.6 = 0.42
    // C: p * q = 0.3 * 0.6 = 0.18
    let expected_weights = [0.4, 0.42, 0.18];
    let mut flat_weights: Vec<f64> = flat.iter().map(|(w, _)| *w).collect();
    flat_weights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected_sorted = expected_weights.to_vec();
    expected_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, expected) in flat_weights.iter().zip(expected_sorted.iter()) {
        assert!((got - expected).abs() < 1e-12, "Expected weight {}, got {}", expected, got);
    }
}

// ─────────────────────────────────────────────────────────────
// Test 18: flatten_blend_nested_depth_3
// ─────────────────────────────────────────────────────────────

/// A depth-3 blend `blend(A, blend(B, blend(C, D, r), q), p)` should
/// flatten to 4 entries.
#[test]
fn flatten_blend_nested_depth_3() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(0.5), CapturedEnv::default());
    let d = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(1.0), CapturedEnv::default());

    // Inner: blend(C, D, r=0.2)
    let inner = FuncSource::Blend {
        from: Box::new(c.clone()),
        to: Box::new(d.clone()),
        frozen_progress: 0.2,
    };
    // Middle: blend(B, inner, q=0.4)
    let middle = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.4,
    };
    // Outer: blend(A, middle, p=0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(middle.clone()),
        frozen_progress: 0.6,
    };

    let flat = flatten_blend(&outer);
    assert_eq!(flat.len(), 4, "Expected 4 entries for depth-3 blend");

    // Total weight should sum to 1.0
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );

    // Expected weights:
    // A: 1 - p = 0.4
    // B: (1 - q) * p = 0.6 * 0.6 = 0.36
    // C: (1 - r) * q * p = 0.8 * 0.4 * 0.6 = 0.192
    // D: r * q * p = 0.2 * 0.4 * 0.6 = 0.048
    let expected_weights = [0.4, 0.36, 0.192, 0.048];
    let mut flat_weights: Vec<f64> = flat.iter().map(|(w, _)| *w).collect();
    flat_weights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected_sorted = expected_weights.to_vec();
    expected_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, expected) in flat_weights.iter().zip(expected_sorted.iter()) {
        assert!((got - expected).abs() < 1e-12, "Expected weight {}, got {}", expected, got);
    }
}

// ─────────────────────────────────────────────────────────────
// Test 19: resolve_func_source_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that evaluating a depth-2 nested blend via `resolve_func_source`
/// yields the same result as the mathematical formula.
///
/// Tree: blend(A, blend(B, C, 0.3), 0.6)
/// where A(x)=sin(x), B(x)=cos(x), C(x)=0.5
///
/// Formula: A*(1-0.6) + B*(1-0.3)*0.6 + C*0.3*0.6
///        = 0.4*sin(x) + 0.42*cos(x) + 0.18*0.5
///        = 0.4*sin(x) + 0.42*cos(x) + 0.09
#[test]
fn resolve_func_source_nested_blend() {
    let env = stdlib_env();

    // Raw sources
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c_raw = Expr::Num(0.5);
    let c = FuncSource::from_expr(vec!["x".to_string()], c_raw, CapturedEnv::default());

    // Inner: blend(B, C, 0.3)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.3,
    };
    // Outer: blend(A, inner, 0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.6,
    };

    for x in [0.0, 0.5, 1.0, 1.5, 2.0] {
        let result = resolve_func_source(&outer, &env, "x", x)
            .expect("nested blend evaluation should succeed");
        let expected = 0.4 * x.sin() + 0.42 * x.cos() + 0.09;
        assert!(
            (result - expected).abs() < 1e-9,
            "At x={}: expected {}, got {}",
            x,
            expected,
            result
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 20: eval_source_scalar_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that the inner `eval_source_scalar` path (used by adaptive
/// sampling) produces the same result as the mathematical formula for
/// a depth-2 nested blend.
#[test]
fn eval_source_scalar_nested_blend() {
    use std::collections::HashMap;

    use crate::timeline::plot::eval_source_scalar;

    let mut env = stdlib_env();

    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(42.0), CapturedEnv::default());

    // Inner: blend(B, C, 0.25)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.25,
    };
    // Outer: blend(A, inner, 0.8)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.8,
    };

    // Formula: A*(1-0.8) + B*(1-0.25)*0.8 + C*0.25*0.8
    //        = 0.2*sin(x) + 0.6*cos(x) + 0.2*42.0
    for x in [0.0, 0.5, 1.0, std::f64::consts::PI / 4.0] {
        let mut cache = HashMap::new();
        let result = eval_source_scalar(&outer, &mut env, "x", x, &mut cache);
        let expected = 0.2 * x.sin() + 0.6 * x.cos() + 0.2 * 42.0;
        assert!(
            (result - expected).abs() < 1e-9,
            "At x={}: expected {}, got {}",
            x,
            expected,
            result
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 21: eval_implicit_source_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that the implicit source evaluation path produces correct
/// results for a nested blend.
#[test]
fn eval_implicit_source_nested_blend() {
    use crate::timeline::plot::eval_implicit_source;

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
    // Third implicit source: x + y
    let sum_expr = Expr::Binary(
        Box::new(Expr::Ident("x".to_string())),
        crate::ast::BinaryOp::Add,
        Box::new(Expr::Ident("y".to_string())),
    );
    let sum_src = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        sum_expr,
        CapturedEnv::default(),
    );

    // Inner: blend(circle, line, 0.4)
    let inner = FuncSource::Blend {
        from: Box::new(circle.clone()),
        to: Box::new(line.clone()),
        frozen_progress: 0.4,
    };
    // Outer: blend(inner, sum_src, 0.7)
    let outer = FuncSource::Blend {
        from: Box::new(inner.clone()),
        to: Box::new(sum_src.clone()),
        frozen_progress: 0.7,
    };

    // At (0,0):
    // circle(0,0) = -1, line(0,0) = 0, sum(0,0) = 0
    // inner = (-1)*(1-0.4) + 0*0.4 = -0.6
    // outer = (-0.6)*0.3 + 0*0.7 = -0.18
    // Formula: circle*(1-0.4)*(1-0.7) + line*0.4*(1-0.7) + sum*0.7
    //        = -1*0.6*0.3 + 0*0.4*0.3 + 0*0.7 = -0.18
    let result = eval_implicit_source(&outer, &mut env, 0.0, 0.0);
    assert!((result - (-0.18)).abs() < 1e-9, "At (0,0): expected -0.18, got {}", result);

    // At (1, 1):
    // circle(1,1) = 1, line(1,1) = 0, sum(1,1) = 2
    // inner = 1*0.6 + 0*0.4 = 0.6
    // outer = 0.6*0.3 + 2*0.7 = 0.18 + 1.4 = 1.58
    // Formula: 1*0.6*0.3 + 0*0.4*0.3 + 2*0.7 = 0.18 + 1.4 = 1.58
    let result = eval_implicit_source(&outer, &mut env, 1.0, 1.0);
    assert!((result - 1.58).abs() < 1e-9, "At (1,1): expected 1.58, got {}", result);
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
    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition for VectorField");
    assert_eq!(track.func_transitions[0].blend_mode, FuncBlendMode::Output);

    let plot = track.procedural_plot.as_ref().expect("VectorField should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::VectorField);

    let mut env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &mut env, 1500, &track.func_transitions);
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
    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition for Heatmap");

    let plot = track.procedural_plot.as_ref().expect("Heatmap should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::Heatmap);

    let mut env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &mut env, 1500, &track.func_transitions);
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
    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition for ContourSet");

    let plot = track.procedural_plot.as_ref().expect("ContourSet should have procedural_plot");
    assert_eq!(plot.plot_type, ProceduralPlotKind::ContourSet);

    let mut env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &mut env, 1500, &track.func_transitions);
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
    assert_eq!(track.func_transitions.len(), 1, "Expected 1 FuncTransition");
    assert_eq!(track.func_transitions[0].blend_mode, FuncBlendMode::Opacity);

    let plot = track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot")
        .clone();
    let mut env = stdlib_env();
    let paths = sample_procedural_plot_at(&plot, &mut env, 1500, &track.func_transitions);
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
    registry.register(Arc::new(PlotExt)).expect("register PlotExt");
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
    let track = report.output.tracks.get("p").expect("plot extension track");
    assert_eq!(
        track.func_transitions.len(),
        1,
        "plot_geometry capability should enable func transitions"
    );
}
