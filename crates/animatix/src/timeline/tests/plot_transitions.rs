use std::sync::Arc;

use super::*;
use crate::ast::{BinaryOp, Expr};
use crate::easing::Easing;
use crate::primitives::{BuildCtx, Primitive, PrimitiveRegistry};
use crate::timeline::CapturedEnv;
use crate::timeline::modifier_runtime::ir::{CompiledExpr, compile_expr};
use crate::timeline::plot::{
    FuncBlendMode, FuncSource, FuncTransition, PlotCurveKind, ProceduralPlot, ProceduralPlotKind,
    blend_depth, build_implicit_plot_path_from_source, eval_implicit_source, eval_source_scalar,
    flatten_blend, resolve_func_source, sample_procedural_plot_at,
};

// Helpers

fn build_from_source(source: &str) -> crate::diagnostics::BuildReport<crate::timeline::Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new())
}

fn assert_clean(report: &crate::diagnostics::BuildReport<crate::timeline::Timeline>) {
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );
}

fn assert_sampled(paths: &[crate::timeline::VelloPath], context: &str) {
    assert!(!paths.is_empty(), "Expected output paths {context}");
    assert!(!paths[0].path.elements().is_empty(), "Expected non-empty path {context}");
}

fn stdlib_env() -> Environment {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    env
}

fn sin_x_expr() -> Expr {
    Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())])
}

fn cos_x_expr() -> Expr {
    Expr::Call("cos".to_string(), vec![Expr::Ident("x".to_string())])
}

fn compiled_body(body: Expr) -> CompiledExpr {
    compile_expr(&body).expect("test function body should compile")
}

fn circle_expr() -> Expr {
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

fn line_yx_expr() -> Expr {
    Expr::Binary(
        Box::new(Expr::Ident("y".to_string())),
        BinaryOp::Sub,
        Box::new(Expr::Ident("x".to_string())),
    )
}

fn bin(op: BinaryOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(Box::new(a), op, Box::new(b))
}

fn dummy_procedural_plot(label: &str, body: CompiledExpr) -> ProceduralPlot {
    ProceduralPlot {
        plot_type: ProceduralPlotKind::Curve(PlotCurveKind::Cartesian),
        kind: PlotCurveKind::Cartesian,
        func_args: vec!["x".to_string()],
        func_body: body,
        actor_label: label.to_string(),
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
    }
}

fn assert_flatten_weights(source: &FuncSource, expected_weights: &[f64]) {
    let flat = flatten_blend(source);
    assert_eq!(flat.len(), expected_weights.len(), "flattened entry count mismatch");
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Total weight should sum to 1.0, got {total_weight}"
    );
    let mut got: Vec<f64> = flat.iter().map(|(w, _)| *w).collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected = expected_weights.to_vec();
    expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (g, e) in got.iter().zip(&expected) {
        assert!((g - e).abs() < 1e-12, "Expected weight {e}, got {g}");
    }
}

fn assert_mode_transition(source: &str, kind: PlotCurveKind, label: &str) {
    let report = build_from_source(source);
    assert_clean(&report);
    let timeline = report.output;
    let track = timeline.get_entity_id("curve").expect("curve track should exist");
    let transitions = timeline.world.func_transitions(track);
    assert_eq!(transitions.len(), 1, "Expected 1 FuncTransition for {label} mode");
    let t = &transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    let plot = timeline
        .world
        .procedural_plot(track)
        .unwrap_or_else(|| panic!("{label} curve should have a procedural_plot"));
    assert_eq!(plot.kind, kind, "plot kind should be {label}");
    let env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &env, 2500, transitions);
    assert_sampled(&paths, &format!("for {label} mid-transition"));
}

fn func_source_1d(expr: Expr) -> FuncSource {
    FuncSource::from_expr(vec!["x".to_string()], expr, CapturedEnv::default())
}

fn func_source_2d(expr: Expr) -> FuncSource {
    FuncSource::from_expr(vec!["x".to_string(), "y".to_string()], expr, CapturedEnv::default())
}

fn func_source_num(val: f64) -> FuncSource {
    func_source_1d(Expr::Num(val))
}

fn blend_source(from: FuncSource, to: FuncSource, frozen_progress: f64) -> FuncSource {
    FuncSource::Blend {
        from: Box::new(from),
        to: Box::new(to),
        frozen_progress,
    }
}

fn assert_2d_plot_transition(
    source: &str,
    actor_label: &str,
    expected_kind: ProceduralPlotKind,
    check_paths: impl FnOnce(&[crate::timeline::VelloPath]),
) {
    let report = build_from_source(source);
    assert_clean(&report);

    let track = report
        .output
        .get_entity_id(actor_label)
        .unwrap_or_else(|| panic!("{actor_label} track should exist"));
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].blend_mode, FuncBlendMode::Output);

    let plot = report
        .output
        .world
        .procedural_plot(track)
        .unwrap_or_else(|| panic!("{actor_label} procedural_plot"));
    assert_eq!(plot.plot_type, expected_kind);

    let env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &env, 1500, transitions);
    check_paths(&paths);
}

const CASCADING_SOURCE: &str = r#"
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

struct PlotExt;

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
        let mut plot = dummy_procedural_plot(
            label,
            compile_expr(&Expr::Ident("x".to_string())).expect("compile test body"),
        );
        plot.plot_type = ProceduralPlotKind::default();
        plot.p_x_domain = [-5.0, 5.0];
        plot.p_y_domain = [-5.0, 5.0];
        plot.p_size = [320.0, 200.0];
        plot.t_domain = [0.0, 0.0];
        plot.tolerance = 0.1;
        plot.max_depth = 6;
        plot.resolution = 64;
        plot.density = 0;
        plot.fill_color = [0.0, 0.0, 0.0, 0.0];
        ctx.timeline.world.ensure_plot_payload(track_id).procedural_plot = Some(plot);
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

// Tests: Transitions & Mode Handling

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
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 1, "Expected exactly 1 FuncTransition");

    let t = &transitions[0];
    assert_eq!(t.start_ms, 2000, "start_ms should be 2000 (2s)");
    assert_eq!(t.end_ms, 3000, "end_ms should be 3000 (2s + 1s)");
    assert_eq!(t.easing, Easing::Linear, "default easing should be Linear");
    assert_eq!(t.blend_mode, FuncBlendMode::Output, "default blend should be Output");
    assert_eq!(t.from.arity(), 1, "from arity should be 1");
    assert_eq!(t.to.arity(), 1, "to arity should be 1");

    let env = stdlib_env();
    let from_val = resolve_func_source(&t.from, &env, "x", 1.0).expect("resolve from");
    assert!((from_val - 1.0_f64.sin()).abs() < 1e-9, "from at x=1.0 should be sin(1.0)");

    let to_val = resolve_func_source(&t.to, &env, "x", 1.0).expect("resolve to");
    assert!((to_val - 1.0_f64.cos()).abs() < 1e-9, "to at x=1.0 should be cos(1.0)");
}

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
    let mut plot = dummy_procedural_plot("hero_wave", compiled_body(body));
    plot.p_x_domain = [0.0, 4.0 * std::f64::consts::PI];
    plot.p_y_domain = [-1.75, 1.75];
    plot.p_size = [660.0, 351.858];
    plot.tolerance = 0.5;
    plot.max_depth = 10;
    plot.resolution = resolution;
    plot.density = 0;
    plot.fill_color = [0.0, 0.0, 0.0, 0.0];
    plot
}

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

#[test]
fn coarser_resolution_leaves_longer_straight_spans() {
    let coarse = widest_gap(&epicycles_plot(16));
    let fine = widest_gap(&epicycles_plot(256));
    assert!(coarse > 0.0, "coarse plot produced no spans at all");
    assert!(
        coarse > fine * 1.5,
        "`resolution` did not change span length: coarse gap {coarse:.1}px vs fine gap {fine:.1}px"
    );
}

#[test]
fn blend_at_half_progress() {
    let env = stdlib_env();
    let blend = blend_source(func_source_1d(sin_x_expr()), func_source_1d(cos_x_expr()), 0.5);

    for x in [0.0_f64, 0.5, 1.0, std::f64::consts::PI / 4.0, 2.0] {
        let result = resolve_func_source(&blend, &env, "x", x).expect("resolve blend");
        let expected = 0.5 * x.sin() + 0.5 * x.cos();
        assert!(
            (result - expected).abs() < 1e-9,
            "blend at x={x}: expected {expected}, got {result}"
        );
    }

    let plot = dummy_procedural_plot("curve", compiled_body(sin_x_expr()));
    let transition = FuncTransition {
        start_ms: 2000,
        end_ms: 3000,
        easing: Easing::Linear,
        from: func_source_1d(sin_x_expr()),
        to: func_source_1d(cos_x_expr()),
        blend_mode: FuncBlendMode::Output,
    };

    let paths_mid = sample_procedural_plot_at(&plot, &env, 2500, std::slice::from_ref(&transition));
    assert_sampled(&paths_mid, "at mid-transition");

    let paths_before =
        sample_procedural_plot_at(&plot, &env, 1000, std::slice::from_ref(&transition));
    assert_sampled(&paths_before, "before transition");

    let paths_after = sample_procedural_plot_at(&plot, &env, 4000, &[transition]);
    assert_sampled(&paths_after, "after transition");
}

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
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    assert_eq!(report.output.world.func_transitions(track).len(), 2);

    let second = &report.output.world.func_transitions(track)[1];
    match &second.from {
        FuncSource::Blend {
            frozen_progress, ..
        } => {
            assert!((frozen_progress - 0.5).abs() < 1e-9, "Expected frozen_progress ≈ 0.5");
        },
        FuncSource::Compiled(..) => panic!("Expected FuncSource::Blend for second transition"),
    }
}

#[test]
fn cascading_transitions() {
    let report = build_from_source(CASCADING_SOURCE);
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 3, "Expected 3 FuncTransitions");

    assert!(matches!(&transitions[0].from, FuncSource::Compiled(..)));
    assert!(matches!(&transitions[1].from, FuncSource::Blend { .. }));
    assert!(matches!(&transitions[2].from, FuncSource::Blend { .. }));

    if let FuncSource::Blend {
        frozen_progress, ..
    } = &transitions[2].from
    {
        assert!((frozen_progress - 0.5).abs() < 1e-9, "Third transition frozen_progress ≈ 0.5");
    }

    let env = stdlib_env();
    let plot = report.output.world.procedural_plot(track).expect("curve procedural_plot");
    let paths = sample_procedural_plot_at(plot, &env, 2750, transitions);
    assert_sampled(&paths, "during cascading transition");
}

#[test]
fn polar_mode_transition() {
    assert_mode_transition(
        r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "polar", func: (t) => 1.0, t_domain: (0, tau), stroke_width: 2
        }

        #2s
        curve.func = (t) => 1.0 + 0.5 * cos(3 * t) [1s]
        "#,
        PlotCurveKind::Polar,
        "polar",
    );
}

#[test]
fn parametric_mode_transition() {
    assert_mode_transition(
        r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "parametric", func: (t) => (cos(t), sin(t)),
                 t_domain: (0, tau), stroke_width: 2
        }

        #2s
        curve.func = (t) => (cos(2 * t), sin(3 * t)) [1s]
        "#,
        PlotCurveKind::Parametric,
        "parametric",
    );
}

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
    assert!(
        report.diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidPlotFunc),
        "Expected InvalidPlotFunc diagnostic for arity mismatch"
    );

    let track = report.output.get_entity_id("curve").expect("curve track should still exist");
    assert!(
        report.output.world.func_transitions(track).is_empty(),
        "No FuncTransition should be created when arity mismatches"
    );
}

#[test]
fn static_plot_no_transitions() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        g: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (640, 360) {
          curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 2
        }
    "#;
    let report = build_from_source(source);
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    assert!(report.output.world.func_transitions(track).is_empty());
    assert!(report.output.world.procedural_plot(track).is_some());
}

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
    assert_clean(&report);
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
    assert!((frame_freq - expected_freq).abs() < 1e-9);

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
    assert!((result - expected_freq.sin()).abs() < 1e-6);
}

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
    for (suffix, expected_output) in [("__0", 2.0_f64), ("__1", 4.0_f64), ("__2", 6.0_f64)] {
        let track_name = format!("curve{}", suffix);
        let track = report.output.get_entity_id(&track_name).unwrap();
        let plot = report.output.world.procedural_plot(track).unwrap();

        assert!(plot.extra_captures.0.contains_key("freq"));
        let paths = sample_procedural_plot_at(plot, &env, 0, &[]);
        assert!(!paths.is_empty());

        let func_source = FuncSource::Compiled(
            plot.func_args.clone(),
            Box::new(plot.func_body.clone()),
            plot.extra_captures.clone(),
        );
        let result =
            resolve_func_source(&func_source, &env, "x", 2.0).expect("evaluation should succeed");
        assert!((result - expected_output).abs() < 1e-9);
    }
}

#[test]
fn adaptive_quality_reduces_depth_for_blends() {
    let report = build_from_source(CASCADING_SOURCE);
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 3);

    let depth = blend_depth(&transitions[2].from);
    assert!(depth >= 2, "Expected blend_depth >= 2, got {depth}");

    let env = stdlib_env();
    let plot = report.output.world.procedural_plot(track).expect("curve procedural_plot");
    let paths = sample_procedural_plot_at(plot, &env, 2750, transitions);
    assert_sampled(&paths, "during 3-deep blend");
}

#[test]
fn quality_factor_calculation() {
    fn compute_qf(from_depth: usize, to_depth: usize) -> f64 {
        let depth = from_depth.max(to_depth) + 1;
        if depth == 0 {
            1.0
        } else {
            0.75_f64.powi(depth as i32 - 1)
        }
    }

    fn raw() -> FuncSource {
        func_source_1d(sin_x_expr())
    }

    fn blend_of_depth(d: usize) -> FuncSource {
        if d == 0 {
            return raw();
        }
        let inner = blend_of_depth(d - 1);
        blend_source(inner.clone(), inner, 0.5)
    }

    for d in 0..=4 {
        assert_eq!(blend_depth(&blend_of_depth(d)), d, "blend_depth mismatch at {d}");
    }

    for (from, to, expected) in [(0, 0, 1.0), (1, 0, 0.75), (2, 0, 0.5625), (3, 0, 0.421875)] {
        let qf = compute_qf(from, to);
        assert!(
            (qf - expected).abs() < 1e-12,
            "depth {}: expected {}, got {}",
            from + 1,
            expected,
            qf
        );
    }
}

// Tests: Implicit & 2D Plot Procedural Transitions

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
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 1, "Expected exactly 1 FuncTransition for implicit plot");
    assert_eq!(transitions[0].start_ms, 2000);
    assert_eq!(transitions[0].end_ms, 3000);

    let env = stdlib_env();
    let plot = report.output.world.procedural_plot(track).expect("curve procedural_plot");
    assert_eq!(plot.kind, PlotCurveKind::Implicit);

    for (t, ctx) in [
        (1000, "before transition"),
        (2500, "at mid-transition"),
        (4000, "after transition"),
    ] {
        let paths = sample_procedural_plot_at(plot, &env, t, transitions);
        assert_sampled(&paths, ctx);
    }
}

#[test]
fn implicit_blend_at_half() {
    let mut env = stdlib_env();
    let circle = func_source_2d(circle_expr());
    let line = func_source_2d(line_yx_expr());

    assert!((eval_implicit_source(&circle, &mut env, 0.0, 0.0) - (-1.0)).abs() < 1e-9);
    assert!((eval_implicit_source(&line, &mut env, 0.0, 0.0) - 0.0).abs() < 1e-9);

    let blended = blend_source(circle, line, 0.5);
    assert!((eval_implicit_source(&blended, &mut env, 0.0, 0.0) - (-0.5)).abs() < 1e-9);

    let path = build_implicit_plot_path_from_source(
        &mut env,
        &blended,
        &[-2.0, 2.0],
        &[-2.0, 2.0],
        &[200.0, 200.0],
        32,
        &[0.0; 4],
    );
    assert!(
        !path.elements().is_empty(),
        "Expected non-empty contour from blended implicit source"
    );
}

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
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 3, "Expected 3 FuncTransitions");

    assert!(matches!(&transitions[0].from, FuncSource::Compiled(..)));
    assert!(matches!(&transitions[1].from, FuncSource::Blend { .. }));
    assert!(matches!(&transitions[2].from, FuncSource::Blend { .. }));

    if let FuncSource::Blend {
        frozen_progress, ..
    } = &transitions[1].from
    {
        assert!((frozen_progress - 0.5).abs() < 1e-9);
    }

    let env = stdlib_env();
    let plot = report.output.world.procedural_plot(track).expect("curve procedural_plot");
    assert_eq!(plot.kind, PlotCurveKind::Implicit);

    let paths_deep = sample_procedural_plot_at(plot, &env, 2750, transitions);
    assert_sampled(&paths_deep, "during 3-deep implicit cascade");

    let paths_after = sample_procedural_plot_at(plot, &env, 5000, transitions);
    assert_sampled(&paths_after, "after all implicit transitions");
}

#[test]
fn vector_field_func_transition() {
    assert_2d_plot_transition(
        r#"
        config { colorscheme: "editorial-dark" }

        field: VectorField, func: (x, y) => (y, -x), density: 6,
          x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        field.func = (x, y) => (-y, x) [1s]
        "#,
        "field",
        ProceduralPlotKind::VectorField,
        |paths| assert_sampled(paths, "at mid-transition"),
    );
}

#[test]
fn heatmap_func_transition() {
    assert_2d_plot_transition(
        r#"
        config { colorscheme: "editorial-dark" }

        heat: Heatmap, func: (x, y) => sin(x) * cos(y), resolution: 8,
          color: red, x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        heat.func = (x, y) => x + y [1s]
        "#,
        "heat",
        ProceduralPlotKind::Heatmap,
        |paths| {
            assert!(!paths.is_empty(), "Expected output paths during Heatmap transition");
            assert!(paths.iter().any(|p| p.fill.is_some()), "Expected filled heat cells");
        },
    );
}

#[test]
fn contour_set_func_transition() {
    assert_2d_plot_transition(
        r#"
        config { colorscheme: "editorial-dark" }

        cont: ContourSet, func: (x, y) => x^2 + y^2, levels: {1, 2},
          resolution: 16, x_domain: (-3, 3), y_domain: (-3, 3), size: (160, 160)

        #1s
        cont.func = (x, y) => x * y [1s]
        "#,
        "cont",
        ProceduralPlotKind::ContourSet,
        |paths| assert_sampled(paths, "during ContourSet transition"),
    );
}

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
    assert_clean(&report);

    let track = report.output.get_entity_id("curve").expect("curve track should exist");
    let transitions = report.output.world.func_transitions(track);
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].blend_mode, FuncBlendMode::Opacity);

    let plot = report.output.world.procedural_plot(track).expect("curve procedural_plot");
    let env = stdlib_env();
    let paths = sample_procedural_plot_at(plot, &env, 1500, transitions);
    assert_eq!(paths.len(), 2, "Opacity cross-fade should render both endpoint paths");
    for path in &paths {
        let (color, _) = path.stroke.expect("curves should be stroked");
        assert!(color.to_rgba8().a > 0 && color.to_rgba8().a < 255, "Expected partial alpha");
    }
}

// Tests: Blend Flattening & Multi-depth Evaluation

#[test]
fn test_flatten_blend_nested_depths() {
    // Depth 0: non-blend source references original with weight 1.0
    let raw = func_source_1d(sin_x_expr());
    assert_eq!(blend_depth(&raw), 0);
    let flat = flatten_blend(&raw);
    assert_eq!(flat.len(), 1, "Expected 1 entry for non-blend source");
    assert!((flat[0].0 - 1.0).abs() < 1e-12);
    assert!(std::ptr::eq(flat[0].1, &raw), "Should reference the original source");

    let x = 1.0_f64;
    let sin_val = x.sin();
    let cos_val = x.cos();

    struct DepthCase {
        depth: usize,
        source: FuncSource,
        expected_weights: Vec<f64>,
        expected_val: f64,
    }

    let cases = [
        // Depth 1: single-level blend
        DepthCase {
            depth: 1,
            source: blend_source(func_source_1d(sin_x_expr()), func_source_1d(cos_x_expr()), 0.4),
            expected_weights: vec![0.6, 0.4],
            expected_val: 0.6 * sin_val + 0.4 * cos_val,
        },
        // Depth 2: nested right-leaning blend
        DepthCase {
            depth: 2,
            source: blend_source(
                func_source_1d(sin_x_expr()),
                blend_source(func_source_1d(cos_x_expr()), func_source_num(0.5), 0.3),
                0.6,
            ),
            expected_weights: vec![0.4, 0.42, 0.18],
            expected_val: 0.4 * sin_val + 0.42 * cos_val + 0.18 * 0.5,
        },
        // Depth 3: nested right-leaning blend
        DepthCase {
            depth: 3,
            source: blend_source(
                func_source_1d(sin_x_expr()),
                blend_source(
                    func_source_1d(cos_x_expr()),
                    blend_source(func_source_num(0.5), func_source_num(1.0), 0.2),
                    0.4,
                ),
                0.6,
            ),
            expected_weights: vec![0.4, 0.36, 0.192, 0.048],
            expected_val: 0.4 * sin_val + 0.36 * cos_val + 0.192 * 0.5 + 0.048 * 1.0,
        },
        // Depth 4: bottom-up nested left-leaning blend (parity case)
        DepthCase {
            depth: 4,
            source: [
                (func_source_1d(cos_x_expr()), 0.9),
                (func_source_num(1.0), 0.7),
                (func_source_num(2.0), 0.5),
                (func_source_num(-1.5), 0.3),
            ]
            .into_iter()
            .fold(func_source_1d(sin_x_expr()), |acc, (to, progress)| {
                blend_source(acc, to, progress)
            }),
            expected_weights: vec![0.0105, 0.0945, 0.245, 0.35, 0.3],
            expected_val: 0.0105 * sin_val + 0.0945 * cos_val + 0.495,
        },
    ];

    let mut env = stdlib_env();
    for case in cases {
        assert_eq!(blend_depth(&case.source), case.depth, "depth mismatch at depth {}", case.depth);
        assert_flatten_weights(&case.source, &case.expected_weights);

        let resolved = resolve_func_source(&case.source, &env, "x", x)
            .unwrap_or_else(|e| panic!("resolve_func_source failed at depth {}: {e}", case.depth));
        assert!(
            (resolved - case.expected_val).abs() < 1e-9,
            "resolve_func_source at depth {}: expected {}, got {}",
            case.depth,
            case.expected_val,
            resolved
        );

        let mut cache = std::collections::HashMap::new();
        let eval_scalar = eval_source_scalar(&case.source, &mut env, "x", x, &mut cache);
        assert!(
            (eval_scalar - case.expected_val).abs() < 1e-9,
            "eval_source_scalar at depth {}: expected {}, got {}",
            case.depth,
            case.expected_val,
            eval_scalar
        );
    }
}

#[test]
fn resolve_func_source_nested_blend() {
    let env = stdlib_env();
    let outer = blend_source(
        func_source_1d(sin_x_expr()),
        blend_source(func_source_1d(cos_x_expr()), func_source_num(0.5), 0.3),
        0.6,
    );

    for x in [0.0, 0.5, 1.0, 1.5, 2.0] {
        let result = resolve_func_source(&outer, &env, "x", x)
            .expect("nested blend evaluation should succeed");
        let expected = 0.4 * x.sin() + 0.42 * x.cos() + 0.09;
        assert!((result - expected).abs() < 1e-9, "At x={x}: expected {expected}, got {result}");
    }
}

#[test]
fn eval_source_scalar_nested_blend() {
    let mut env = stdlib_env();
    let outer = blend_source(
        func_source_1d(sin_x_expr()),
        blend_source(func_source_1d(cos_x_expr()), func_source_num(42.0), 0.25),
        0.8,
    );

    for x in [0.0, 0.5, 1.0, std::f64::consts::PI / 4.0] {
        let mut cache = std::collections::HashMap::new();
        let result = eval_source_scalar(&outer, &mut env, "x", x, &mut cache);
        let expected = 0.2 * x.sin() + 0.6 * x.cos() + 0.2 * 42.0;
        assert!((result - expected).abs() < 1e-9, "At x={x}: expected {expected}, got {result}");
    }
}

#[test]
fn eval_implicit_source_nested_blend() {
    let mut env = stdlib_env();
    let circle = func_source_2d(circle_expr());
    let line = func_source_2d(line_yx_expr());
    let sum_src = func_source_2d(bin(
        BinaryOp::Add,
        Expr::Ident("x".to_string()),
        Expr::Ident("y".to_string()),
    ));

    let outer = blend_source(blend_source(circle, line, 0.4), sum_src, 0.7);

    let result = eval_implicit_source(&outer, &mut env, 0.0, 0.0);
    assert!((result - (-0.18)).abs() < 1e-9, "At (0,0): expected -0.18, got {result}");

    let result = eval_implicit_source(&outer, &mut env, 1.0, 1.0);
    assert!((result - 1.58).abs() < 1e-9, "At (1,1): expected 1.58, got {result}");
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
    let report = Timeline::build_with_options(
        &ast,
        &std::collections::HashMap::new(),
        crate::timeline::BuildOptions::default().with_primitive_registry(Arc::new(registry)),
    );
    assert_clean(&report);
    let track = report.output.world.tracks.get_id("p").expect("plot extension track");
    assert_eq!(
        report.output.world.func_transitions(track).len(),
        1,
        "plot_geometry capability should enable func transitions"
    );
}

#[test]
fn test_flip_transition_shared_id() {
    let source_a = r#"
#0s
card: Rect, at: (100, 100), size: (50, 50), color: (1, 0, 0, 1), shared_id: "hero"
"#;
    let source_b = r#"
#0s
card_large: Rect, at: (400, 300), size: (150, 150), color: (1, 0, 0, 1), shared_id: "hero"
"#;

    let (ast_a, _) = animatix_syntax::parser::parse_source(source_a);
    let (ast_b, _) = animatix_syntax::parser::parse_source(source_b);

    let report_a = Timeline::build_with_diagnostics(&ast_a.unwrap(), &Default::default());
    let report_b = Timeline::build_with_diagnostics(&ast_b.unwrap(), &Default::default());

    let timeline_a = report_a.output;
    let timeline_b = report_b.output;

    let shared_a = timeline_a.shared_id_actors(0);
    let shared_b = timeline_b.shared_id_actors(0);

    assert_eq!(shared_a.get("hero"), Some(&"card".to_string()));
    assert_eq!(shared_b.get("hero"), Some(&"card_large".to_string()));

    let dims = SceneDimensions {
        width: 800,
        height: 600,
    };
    let debug_options = Default::default();

    // Verify unsuppressed evaluation has the item
    let unsuppressed = timeline_a.evaluate_program_with_debug(0.0, dims, debug_options, &mut None);
    assert_eq!(unsuppressed.items.len(), 1);
    assert_eq!(unsuppressed.items[0].label, "card");

    // Verify suppressed evaluation suppresses the actor
    let suppressed = timeline_a.evaluate_program_with_suppressed(
        0.0,
        dims,
        debug_options,
        &mut None,
        &["card".to_string()],
    );
    // Suppressed actor has opacity 0 so its draw commands evaluate to opacity 0 and are omitted from scene
    assert_eq!(suppressed.items.len(), 1);
    assert_eq!(suppressed.items[0].opacity, 0.0);
}
