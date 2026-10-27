use super::*;
use crate::ast::Expr;
use crate::easing::Easing;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::modifier_runtime::ir::compile_expr;
use crate::timeline::plot::{
    FuncBlendMode, FuncSource, FuncTransition, PlotCurveKind, ProceduralPlot, ProceduralPlotKind,
    blend_depth, resolve_func_source, sample_procedural_plot_at,
};

// ─────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────

/// Parse an .amx source snippet, build the timeline, and panic on parse
/// errors.  Returns the `BuildReport` so individual tests can inspect both
/// diagnostics and the timeline.
pub(super) fn build_from_source(
    source: &str,
) -> crate::diagnostics::BuildReport<crate::timeline::Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new())
}

/// Build a minimal `Environment` loaded with the standard library so that
/// built-in functions like `sin` and `cos` are available during evaluation.
pub(super) fn stdlib_env() -> Environment {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    env
}

/// Construct the AST expression `sin(x)`.
pub(super) fn sin_x_expr() -> Expr {
    Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())])
}

/// Construct the AST expression `cos(x)`.
pub(super) fn cos_x_expr() -> Expr {
    Expr::Call("cos".to_string(), vec![Expr::Ident("x".to_string())])
}

fn compiled_body(body: Expr) -> crate::timeline::modifier_runtime::ir::CompiledExpr {
    compile_expr(&body).expect("test function body should compile")
}

/// Construct the AST expression `x^2 + y^2 - 1` (unit circle implicit field).
pub(super) fn circle_expr() -> Expr {
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
pub(super) fn line_yx_expr() -> Expr {
    Expr::Binary(
        Box::new(Expr::Ident("y".to_string())),
        crate::ast::BinaryOp::Sub,
        Box::new(Expr::Ident("x".to_string())),
    )
}

pub(super) struct PlotExt;

impl Primitive for PlotExt {
    fn type_name(&self) -> &str {
        "PlotExt"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let track_id = ctx.timeline.world.spawn(label, "Rect");
        ctx.timeline.world.ensure_len();
        ctx.timeline.world.ensure_plot_payload(track_id).procedural_plot = Some(ProceduralPlot {
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
        let caps = ctx.timeline.world.caps(track_id);
        let actor_type = ctx.timeline.world.actor_type(track_id).to_string();
        ctx.timeline
            .world
            .property_plan
            .get_mut(track_id)
            .rebuild_for(&caps, &actor_type);
        Ok(())
    }

    fn handle_assignment(
        &self,
        property: &str,
        value: &crate::ast::Expr,
        ctx: &mut crate::primitives::AssignmentCtx,
        env: &crate::timeline::Environment,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        subject: &str,
    ) -> bool {
        crate::primitives::plot::handle_plot_assignment(
            property,
            value,
            ctx,
            env,
            diagnostics,
            subject,
        )
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        1,
        "Expected exactly 1 FuncTransition"
    );

    let t = &report.output.world.func_transitions(track)[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000 (2s)");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000 (2s + 1s)");
    // Assignments keep the linear default on purpose: an action that expands
    // into them hands them its timing modifiers, so easing the assignment path
    // would double-ease effects that pre-bake their curve. See
    // `default_easing_for`.
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
// Tessellation fineness follows `resolution`
// ─────────────────────────────────────────────────────────────

use crate::ast::BinaryOp;

fn bin(op: BinaryOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(Box::new(a), op, Box::new(b))
}

/// The §03 wave: the first two terms of the square-wave series, on the real
/// plot box (660 x 351.9 px, x 0..4pi, y -1.75..1.75).
fn epicycles_plot(resolution: usize) -> ProceduralPlot {
    let four_over_pi = bin(BinaryOp::Div, Expr::Num(4.0), Expr::Ident("pi".to_string()));
    let third = bin(
        BinaryOp::Div,
        Expr::Call(
            "sin".to_string(),
            vec![bin(
                BinaryOp::Mul,
                Expr::Num(3.0),
                Expr::Ident("x".to_string()),
            )],
        ),
        Expr::Num(3.0),
    );
    let body = bin(
        BinaryOp::Mul,
        four_over_pi,
        bin(
            BinaryOp::Add,
            Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())]),
            third,
        ),
    );
    ProceduralPlot {
        plot_type: ProceduralPlotKind::Curve(PlotCurveKind::Cartesian),
        kind: PlotCurveKind::Cartesian,
        func_args: vec!["x".to_string()],
        func_body: compiled_body(body),
        actor_label: "hero_wave".to_string(),
        param_names: vec![],
        p_x_domain: [0.0, 4.0 * std::f64::consts::PI],
        p_y_domain: [-1.75, 1.75],
        p_size: [660.0, 351.858],
        padding: [0.0, 0.0, 0.0, 0.0],
        t_domain: [0.0, 0.0],
        tolerance: 0.5,
        max_depth: 10,
        resolution,
        density: 0,
        levels: vec![],
        stroke_width: 2.0,
        stroke_color: [1.0, 1.0, 1.0, 1.0],
        fill_color: [0.0, 0.0, 0.0, 0.0],
        params: vec![],
        extra_captures: Default::default(),
    }
}

/// Widest horizontal gap between consecutive vertices of the sampled polyline.
///
/// This is the observable that matters, and segment *count* is not it: on the
/// §03 wave a 16-segment base grid and a 64-segment one both end up emitting
/// ~104 segments, because `tolerance` adds refinement wherever the curve is
/// steep. What differs is *where* the vertices sit — a coarse base grid leaves
/// long spans unrefined, and an unrefined span is drawn as one straight chord.
fn widest_gap(plot: &ProceduralPlot) -> f64 {
    let env = stdlib_env();
    let mut xs: Vec<f64> = Vec::new();
    for vp in sample_procedural_plot_at(plot, &env, 0, &[]) {
        for el in vp.path.elements() {
            let pt = match el {
                kurbo::PathEl::MoveTo(pt) | kurbo::PathEl::LineTo(pt) => *pt,
                _ => continue,
            };
            xs.push(pt.x);
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.windows(2).map(|w| w[1] - w[0]).fold(0.0_f64, f64::max)
}

/// A coarse base grid must leave visibly longer unrefined spans than a fine
/// one — and the knob has to be what changes them.
///
/// Regression guard for the clamp that made `resolution` dead: the sample count
/// was clamped by `max_depth` *before* the log2, so every request above 10
/// collapsed onto the same 16-segment grid and rendered byte-identically. On
/// `web/demos/epicycles` §03 that left the shallow 1/k ripple spanning whole
/// base cells as straight chords: measured against the analytic curve, p95
/// deviation on frame 443 was 3.96px against 1.69px on its neighbours — a
/// 2.27px swing that is the "the curve flattens, then the flat bits jump"
/// flicker. The fix takes the swing to 0.07px.
#[test]
fn coarser_resolution_leaves_longer_straight_spans() {
    let coarse = widest_gap(&epicycles_plot(16));
    let fine = widest_gap(&epicycles_plot(256));
    assert!(coarse > 0.0, "coarse plot produced no spans at all");
    assert!(
        coarse > fine * 1.5,
        "`resolution` did not change span length: coarse gap {coarse:.1}px vs \
         fine gap {fine:.1}px — the clamp is back"
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
    let env = stdlib_env();

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
    let paths_mid = sample_procedural_plot_at(&plot, &env, 2500, std::slice::from_ref(&transition));
    assert!(!paths_mid.is_empty(), "Expected output paths at mid-transition");
    assert!(
        !paths_mid[0].path.elements().is_empty(),
        "Expected non-empty BezPath at mid-transition"
    );

    // Before the transition, output should be pure sin(x).
    let paths_before =
        sample_procedural_plot_at(&plot, &env, 1000, std::slice::from_ref(&transition));
    assert!(!paths_before.is_empty(), "Expected output paths before transition");

    // After the transition, output should be pure cos(x).
    let paths_after = sample_procedural_plot_at(&plot, &env, 4000, &[transition]);
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        2,
        "Expected 2 FuncTransitions for overlapping assignments"
    );

    // Second transition's `from` must be a Blend because the second assignment
    // is processed while the first is still active (t=2s, first runs 1s–3s).
    let second = &report.output.world.func_transitions(track)[1];
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        3,
        "Expected 3 FuncTransitions for cascading assignments"
    );

    // First transition: from = Raw (declaration func).
    assert!(
        matches!(&report.output.world.func_transitions(track)[0].from, FuncSource::Compiled(..)),
        "First transition's from should be Raw"
    );

    // Second transition: from = Blend (frozen from the first).
    assert!(
        matches!(&report.output.world.func_transitions(track)[1].from, FuncSource::Blend { .. }),
        "Second transition's from should be Blend"
    );

    // Third transition: from = Blend (frozen from the second, which is itself a Blend).
    assert!(
        matches!(&report.output.world.func_transitions(track)[2].from, FuncSource::Blend { .. }),
        "Third transition's from should be Blend"
    );

    // Verify the third transition's frozen_progress.  At t=2.5s the second
    // transition (2s–3s) is at progress 0.5.
    if let FuncSource::Blend {
        frozen_progress, ..
    } = &report.output.world.func_transitions(track)[2].from
    {
        assert!(
            (frozen_progress - 0.5).abs() < 1e-9,
            "Third transition's frozen_progress should be ≈ 0.5, got {}",
            frozen_progress
        );
    }

    // Sample at the midpoint of the third transition (t=2.75s) and verify
    // non-empty output.
    let env = stdlib_env();
    let plot = report
        .output
        .world
        .procedural_plot(track)
        .expect("curve should have a procedural_plot")
        .clone();
    let paths =
        sample_procedural_plot_at(&plot, &env, 2750, report.output.world.func_transitions(track));
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        1,
        "Expected 1 FuncTransition for polar mode"
    );

    let t = &report.output.world.func_transitions(track)[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // Verify the plot kind is Polar.
    let plot = report
        .output
        .world
        .procedural_plot(track)
        .expect("polar curve should have a procedural_plot");
    assert_eq!(plot.kind, PlotCurveKind::Polar, "plot kind should be Polar");

    // Sample at mid-transition and verify non-empty output.
    let env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &env, 2500, report.output.world.func_transitions(track));
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        1,
        "Expected 1 FuncTransition for parametric mode"
    );

    let t = &report.output.world.func_transitions(track)[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    // Verify the plot kind is Parametric.
    let plot = report
        .output
        .world
        .procedural_plot(track)
        .expect("parametric curve should have a procedural_plot");
    assert_eq!(plot.kind, PlotCurveKind::Parametric, "plot kind should be Parametric");

    // Sample at mid-transition and verify non-empty output.
    let env = stdlib_env();
    let paths =
        sample_procedural_plot_at(plot, &env, 2500, report.output.world.func_transitions(track));
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

    let track = report.output.get_entity_id("curve").expect("curve track should still exist");
    assert!(
        report.output.world.func_transitions(track).is_empty(),
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert!(
        report.output.world.func_transitions(track).is_empty(),
        "Static plot should have no FuncTransitions"
    );

    assert!(
        report.output.world.procedural_plot(track).is_some(),
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

    let track = timeline.get_entity_id("curve").expect("curve track should exist");
    let plot = timeline
        .world
        .procedural_plot(track)
        .expect("curve should have a procedural plot");
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

    let env = stdlib_env();

    // Check each generated curve track captures a distinct `freq` and samples correctly.
    // For func=(x)=>x*freq, at x=2.0, expected output = 2.0 * freq.
    for (suffix, expected_output) in [("__0", 2.0_f64), ("__1", 4.0_f64), ("__2", 6.0_f64)] {
        let track_name = format!("curve{}", suffix);
        let track = report
            .output
            .get_entity_id(&track_name)
            .unwrap_or_else(|| panic!("{} track should exist", track_name));

        let plot = report
            .output
            .world
            .procedural_plot(track)
            .unwrap_or_else(|| panic!("{} should have procedural_plot", track_name));

        // Verify that `freq` was captured in extra_captures.
        assert!(
            plot.extra_captures.0.contains_key("freq"),
            "{}: `freq` must be in extra_captures (got {:?})",
            track_name,
            plot.extra_captures.0.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
        );

        // Sample the plot and verify output matches the captured `freq` value.
        let paths = sample_procedural_plot_at(plot, &env, 0, &[]);
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

    let track = report.output.get_entity_id("curve").expect("curve track should exist");

    assert_eq!(
        report.output.world.func_transitions(track).len(),
        3,
        "Expected 3 FuncTransitions for 3-deep blend"
    );

    // Verify the from of the third transition is a nested Blend (depth >= 2).
    let third_from = &report.output.world.func_transitions(track)[2].from;
    let depth = blend_depth(third_from);
    assert!(
        depth >= 2,
        "Expected blend_depth >= 2 for third transition's from, got {}",
        depth
    );

    // Sample at the midpoint of the third transition (t=2.75s) and verify
    // non-empty output despite reduced quality.
    let env = stdlib_env();
    let plot = report
        .output
        .world
        .procedural_plot(track)
        .expect("curve should have a procedural_plot")
        .clone();
    let paths =
        sample_procedural_plot_at(&plot, &env, 2750, report.output.world.func_transitions(track));
    assert!(!paths.is_empty(), "Expected output paths during 3-deep blend");
    assert!(
        !paths[0].path.elements().is_empty(),
        "Expected non-empty path during 3-deep blend"
    );
}

// ─────────────────────────────────────────────────────────────
// Test 11: quality_factor_calculation
// ─────────────────────────────────────────────────────────────
