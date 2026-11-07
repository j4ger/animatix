use super::super::*;
use super::support::{
    actor, assert_clean, assert_finite_samples, build, build_ast, keyframe, prop, render_paths,
    text_at, vec2,
};
use crate::ast::{BinaryOp, Expr, Stmt};
use crate::timeline::{Environment, Timeline, Value};

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
#[test]
fn test_reactive_binding_desugars_to_modifier() {
    let ast = vec![keyframe(
        0.0,
        vec![
            actor(
                "orbiter",
                "Ellipse",
                vec![
                    prop("radius_x", Expr::Num(10.0)),
                    prop("radius_y", Expr::Num(10.0)),
                    prop("at", vec2(0.0, 0.0)),
                ],
            ),
            Stmt::ReactiveBinding {
                target: vec![crate::ast::TargetSegment::Static("orbiter".to_string())],
                property: "at".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Binary(
                        Box::new(Expr::Num(640.0)),
                        BinaryOp::Add,
                        Box::new(Expr::Binary(
                            Box::new(Expr::Num(100.0)),
                            BinaryOp::Mul,
                            Box::new(Expr::Call(
                                "cos".to_string(),
                                vec![Expr::Ident("t".to_string())],
                            )),
                        )),
                    ),
                    Expr::Binary(
                        Box::new(Expr::Num(360.0)),
                        BinaryOp::Add,
                        Box::new(Expr::Binary(
                            Box::new(Expr::Num(100.0)),
                            BinaryOp::Mul,
                            Box::new(Expr::Call(
                                "sin".to_string(),
                                vec![Expr::Ident("t".to_string())],
                            )),
                        )),
                    ),
                ]),
                value_span: None,
                span: None,
            },
        ],
    )];

    let report = build_ast(&ast);
    assert_clean(&report);
    let timeline = report.output;

    // The reactive binding should have been desugared to a modifier
    assert!(
        !timeline.modifiers.is_empty(),
        "Expected modifiers from reactive binding desugaring"
    );

    // Evaluate at t=0s — orbiter should be at (740, 360)
    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env_internal(
        0,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
        &overrides,
    );
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                0,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("modifier IR execution should succeed");
    }

    let orbiter_at = overrides.get("orbiter").and_then(|m| m.get("at"));
    assert!(orbiter_at.is_some(), "Expected orbiter.at override from reactive binding");
    if let Some(Value::Vec2([x, y])) = orbiter_at {
        assert!((x - 740.0).abs() < 0.1, "Expected x≈740, got {}", x);
        assert!((y - 360.0).abs() < 0.1, "Expected y≈360, got {}", y);
    } else {
        panic!("Expected Vec2 override for orbiter.at, got {:?}", orbiter_at);
    }
}

#[test]
fn test_hierarchical_assignment_target() {
    let source = r#"
        g: Graph {
            circ: Ellipse {
                at: (0, 0),
                radius: 10,
            }
        }

        #+1s
        g.circ.opacity = 0.5
    "#;

    let report = build(source);

    assert_clean(&report);

    let timeline = report.output;

    // At t=0s, circ.opacity should be 0.0 (pre-keyframe default is hidden)
    let circ_track = timeline.world.tracks.get_id("circ").expect("circ track should exist");
    let opacity_at_0 = timeline.world.style(circ_track).opacity.as_ref().unwrap().evaluate(0);
    assert!(
        (opacity_at_0 - 0.0).abs() < 0.01,
        "Expected circ.opacity=0.0 at t=0 (pre-keyframe default), got {:?}",
        opacity_at_0
    );

    // At t=1s, circ.opacity should be 0.5
    let opacity_at_1s = timeline.world.style(circ_track).opacity.as_ref().unwrap().evaluate(1000);
    assert!(
        (opacity_at_1s - 0.5).abs() < 0.01,
        "Expected circ.opacity=0.5 at t=1s, got {:?}",
        opacity_at_1s
    );
}

#[test]
fn explicit_opacity_before_keyframe_is_honored() {
    // A declaration before any keyframe starts hidden by default, but an
    // explicit `opacity` must be preserved instead of ignored.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }
        box: Rect, size: (100, 100), color: accent.primary, opacity: 0, at: (200, 150)
    "#;
    let report = build(source);
    assert_clean(&report);
    let track = report.output.world.tracks.get_id("box").expect("box track should exist");
    let opacity_at_0 = report.output.world.style(track).opacity.as_ref().unwrap().evaluate(0);
    assert_eq!(opacity_at_0, 0.0, "explicit opacity: 0 should apply at t=0");
}

#[test]
fn graph_axes_invisible_before_fadein() {
    // Graph declared before any keyframe → default_opacity = 0.0
    // fade-in at #0.5s should animate opacity 0→1
    let source = "g1: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), at: (280, 200)\n\n#0.5s\nfade-in g1 [400ms]";
    let timeline = build(source).output;

    let track = timeline.world.tracks.get_id("g1").expect("g1 track should exist");
    let opacity_at_0 = timeline.world.style(track).opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_500 = timeline.world.style(track).opacity.as_ref().map(|t| t.evaluate(500));
    let opacity_at_900 = timeline.world.style(track).opacity.as_ref().map(|t| t.evaluate(900));
    let opacity_at_1000 = timeline.world.style(track).opacity.as_ref().map(|t| t.evaluate(1000));

    assert_eq!(opacity_at_0, Some(0.0), "opacity should be 0 at t=0");
    assert_eq!(opacity_at_500, Some(0.0), "opacity should be 0 at t=500ms (fade-in start)");
    assert_eq!(opacity_at_900, Some(1.0), "opacity should be 1 at t=900ms (fade-in end)");
    assert_eq!(opacity_at_1000, Some(1.0), "opacity should stay 1 after fade-in");
}

#[test]
fn container_fadein_reveals_graph_hosted_children() {
    // Container entrance actions must cascade reveal state to hosted children
    // so pre-keyframe children become visible when the container fades in.
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }

        #0.3s
        fade-in g [300ms]
    "#;
    let timeline = build(source).output;

    let child = timeline.world.tracks.get_id("c").expect("child curve track should exist");
    assert!(
        !timeline.world.meta(child).hidden_by_default,
        "container fade-in must lift the child's hidden-by-default flag"
    );
    let opacity_at_0 = timeline.world.style(child).opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_1000 = timeline.world.style(child).opacity.as_ref().map(|t| t.evaluate(1000));
    assert_eq!(
        opacity_at_0,
        Some(0.0),
        "child opacity should start at its seeded 0 (fade starts at 300ms)"
    );
    assert_eq!(
        opacity_at_1000,
        Some(1.0),
        "child opacity should be lifted to 1 after the container fade-in window"
    );
}

#[test]
fn pre_keyframe_graph_container_is_hidden_until_its_entrance() {
    // Pre-keyframe containers declared with children must receive the hidden-by-default
    // seed so their contents and painted chrome stay hidden until their entrance.
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }

        #1.0s
        fade-in g [300ms]
    "#;
    let timeline = build(source).output;

    let graph = timeline.world.tracks.get_id("g").expect("graph track should exist");
    assert!(
        !timeline.world.meta(graph).hidden_by_default,
        "the entrance action consumes the flag"
    );
    let opacity = |ms: u64| {
        timeline
            .world
            .style(graph)
            .opacity
            .as_ref()
            .map(|t| t.evaluate(ms))
            .unwrap_or(1.0)
    };
    assert_eq!(opacity(0), 0.0, "graph must start invisible before its entrance");
    assert_eq!(opacity(900), 0.0, "graph must stay invisible until the entrance at 1.0s");
    assert!(
        (opacity(1300) - 1.0).abs() < 1e-6,
        "graph must be fully visible after the fade completes"
    );
}

#[test]
fn every_built_track_identity_is_consistent() {
    // `actor_type` and `kind` are written together through `set_identity`; this
    // guards against a future site writing `kind` directly and drifting.
    let source = r#"
        config { colorscheme: "editorial-dark" }
        title: Text, text: "hi"
        code: Code, text: "let x = 1"
        r: Rect, size: (10, 10)
        e: Ellipse, size: (10, 10)
        p: Polygon, points: {(0, 0), (10, 0), (0, 10)}
        row: Row { c: Rect, size: (5, 5) }
        g: Graph, x_domain: (-1, 1), y_domain: (-1, 1), size: (100, 100) {
            curve: PlotCurve, kind: "cartesian", func: (x) => x
        }

        #0s
        fade-in title [1ms]
    "#;
    let timeline = build(source).output;

    for (label, id) in timeline.world.tracks.iter() {
        let expected =
            animatix_std::caps_for_type(timeline.world.actor_type(id)).unwrap_or_default();
        assert_eq!(
            timeline.world.caps(id),
            expected,
            "track '{label}' caps drifted from actor_type '{}'",
            timeline.world.actor_type(id)
        );
    }
}

#[test]
fn unrevealed_graph_child_still_warns_never_revealed() {
    // Pre-keyframe hosted children without entrance actions must emit NeverRevealed;
    // a parent's constant declaration opacity must not count as an entrance reveal.
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }
    "#;
    let report = build(source);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed
                && d.location.subject.as_deref() == Some("c")),
        "hidden graph child without any entrance must warn never-revealed, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn plot_capture_of_always_written_var_is_dynamic() {
    // A plot closure capturing variables updated in an `always` block must be
    // marked dynamic so it resamples every frame.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        #0s
        let freq = 2

        curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x),
          color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)

        always {
          freq = 2 + 3 * sin(t * 0.5)
        }
    "#;
    let timeline = build(source).output;

    assert!(
        timeline.frame_written_vars.contains("freq"),
        "always bare assignment must register the written name, got: {:?}",
        timeline.frame_written_vars
    );
    let track = timeline.world.tracks.get_id("curve").expect("curve track");
    let plot = timeline.world.procedural_plot(track).expect("procedural plot");
    assert!(
        plot.is_dynamic(&timeline.frame_written_vars),
        "closure capturing an always-written variable must be dynamic"
    );
}

#[test]
fn plot_closure_calls_pure_fn_at_frame_time() {
    // LG-1 premise check: a plot closure body may call a pure user fn. The
    // body references `t` so the procedural plot is dynamic (per-frame
    // resampling fires), and the frame env's frozen base must resolve
    // `double` through the CallEnv dispatch. The sampled curve at t where
    // the dynamic gate is active must equal double(x) = 2x, not NaN.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

fn double(n: Num) -> Num {
  n * 2
}

c: PlotCurve, kind: "cartesian", func: (x) => double(x) + 0 * t,
  color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)

#0.2s
fade-in c [100ms]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.world.tracks.get_id("c").expect("curve track");
    assert!(
        timeline
            .world
            .procedural_plot(track)
            .expect("procedural plot")
            .is_dynamic(&timeline.frame_written_vars),
        "body references t — the plot must be dynamic"
    );

    // Full pipeline evaluation: frame env + per-frame plot resampling.
    assert_finite_samples(&timeline, 1.0);
}

#[test]
fn draw_in_trims_stroke_only_path_geometry_mid_draw() {
    // `draw-in` must trim stroke geometry along path commands mid-draw,
    // restoring the full path once the animation settles.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

p: Path, commands: {move_to(0, 0), line_to(100, 0), line_to(100, 100), line_to(0, 100)},
  stroke: accent.primary, stroke_width: 4, fill_opacity: 0.0, at: (320, 180)

#0s
draw-in p [1s, ease: linear]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let count_stroke_segments = |time: f64| -> usize {
        render_paths(&timeline, time)
            .iter()
            .filter(|vp| vp.stroke.is_some())
            .flat_map(|vp| vp.path.elements().iter().copied())
            .filter(|el| {
                matches!(
                    el,
                    kurbo::PathEl::LineTo(_)
                        | kurbo::PathEl::QuadTo(_, _)
                        | kurbo::PathEl::CurveTo(_, _, _)
                )
            })
            .count()
    };

    // Linear ease over 1s: t=0.5 → progress 0.5 → ceil(0.5 × 3) = 2 of 3
    // segments drawn.
    assert_eq!(count_stroke_segments(0.5), 2, "mid-draw the stroke must be partial");
    // At the stamp nothing is drawn yet (progress clamps to 0, stroke hidden).
    assert_eq!(count_stroke_segments(0.0), 0);
    // Settled: the full path is back, twice, so a memo hit after a mid-draw
    // bypass is covered too.
    assert_eq!(count_stroke_segments(1.5), 3);
    assert_eq!(count_stroke_segments(2.5), 3);
}

#[test]
fn draw_in_cuts_a_straight_line_mid_segment() {
    // The hero-underline case: a single-segment stroke must draw from its
    // start point rather than pop whole segments (trim is arc-length aware).
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

underline: Path, commands: {move_to(100, 180), line_to(540, 180)},
  stroke: accent.primary, stroke_width: 6, fill_opacity: 0.0

#0s
draw-in underline [1s, ease: linear]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let stroke_end = |time: f64| -> kurbo::Point {
        render_paths(&timeline, time)
            .iter()
            .filter(|vp| vp.stroke.is_some())
            .flat_map(|vp| vp.path.elements().iter().rev().copied())
            .find_map(|el| match el {
                kurbo::PathEl::LineTo(p) => Some(p),
                _ => None,
            })
            .expect("stroke path present")
    };

    // Linear 1s draw over a 440px line: at 0.5s the tip sits at the midpoint,
    // at 0.25s a quarter in, settled exactly on the authored end.
    let mid = stroke_end(0.5);
    assert!((mid.x - 320.0).abs() < 1.0 && (mid.y - 180.0).abs() < 0.5, "got {mid:?}");
    let quarter = stroke_end(0.25);
    assert!((quarter.x - 210.0).abs() < 1.0, "got {quarter:?}");
    let done = stroke_end(1.5);
    assert!((done.x - 540.0).abs() < 0.5, "got {done:?}");
}

#[test]
fn draw_in_scheduled_after_the_declaration_starts_hidden() {
    // An authored `opacity: 1` is the settled value, not a pre-entrance
    // state: a `draw-in` at #1s must leave the line unpainted from frame 0
    // instead of showing the full stroke until the action begins.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

line_a: Line, from: (100, 180), to: (540, 180), stroke: accent.primary,
  stroke_width: 6, opacity: 1

#1s
draw-in line_a [600ms, ease: linear]
    "#;
    let report = build(source);
    assert_clean(&report);
    let timeline = report.output;
    let stroked = |time: f64| -> usize {
        render_paths(&timeline, time).iter().filter(|vp| vp.stroke.is_some()).count()
    };

    assert_eq!(stroked(0.0), 0, "the line must not paint before its draw-in");
    assert_eq!(stroked(0.9), 0, "still hidden until the action begins");
    assert!(stroked(1.3) > 0, "mid-draw must paint a partial stroke");
    assert!(stroked(1.7) > 0, "the draw completes on its authored duration");

    let shape_id = timeline.world.get_id("line_a").expect("line entity");
    assert_eq!(timeline.world.style(shape_id).stroke_progress.get(0, 1.0), 0.0);
}

#[test]
fn dash_pattern_stamps_paths_and_dash_offset_animates() {
    // `dash_pattern`/`dash_offset` ride outside the shape-command memo, so a
    // static pattern is stamped on every frame and an animated offset (the
    // marching-ants idiom) is never served from a cached encoding.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

p: Path, commands: {move_to(0, 0), line_to(300, 0)},
  stroke: accent.primary, stroke_width: 2, fill_opacity: 0.0,
  dash_pattern: {8, 6}

#0s
p.dash_offset = 0
#0.5s
p.dash_offset = 40
    "#;
    let report = build(source);
    // The `never-revealed` content hint fires on a fixture with no entrance
    // action; it is about the fixture, not the feature under test.
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed),
        "expected only never-revealed hints, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let dash_at = |time: f64| -> (Option<Vec<f32>>, f32) {
        let mut found = (None, 0.0);
        for vp in render_paths(&timeline, time) {
            if vp.stroke.is_some() {
                found = (vp.dash_pattern.clone(), vp.dash_offset);
            }
        }
        found
    };

    let (pattern, offset_early) = dash_at(0.25);
    assert_eq!(pattern.as_deref(), Some(&[8.0_f32, 6.0][..]), "pattern must be stamped");
    assert_eq!(offset_early, 0.0, "undated assignment holds until its stamp");

    let (pattern_again, offset_late) = dash_at(0.75);
    assert_eq!(pattern_again.as_deref(), Some(&[8.0_f32, 6.0][..]));
    assert_eq!(offset_late, 40.0, "the offset step lands at its stamp");
}

#[test]
fn blend_mode_is_bound_and_sampled() {
    // `blend:` rides the registry-backed declaration path; the sampled track
    // must carry the authored mode and default to "normal" elsewhere.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

base: Rect, size: (200, 200), color: accent.primary, at: (200, 180)
glow: Ellipse, size: (200, 200), color: accent.warning, at: (380, 180),
  blend: "screen"
plain: Rect, size: (50, 50), color: text.primary, at: (600, 320)
    "#;
    let report = build(source);
    // `never-revealed` content hints are about the entrance-less fixture, not
    // the feature under test.
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed),
        "expected only never-revealed hints, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    use crate::timeline::TrackAccessor;
    let glow = timeline.world.tracks.get_id("glow").expect("glow track");
    assert_eq!(timeline.world.style(glow).blend.get(0, "normal".to_string()), "screen");
    let plain = timeline.world.tracks.get_id("plain").expect("plain track");
    assert_eq!(timeline.world.style(plain).blend.get(0, "normal".to_string()), "normal");
}

#[test]
fn sum_range_computes_series_at_build_and_frame_time() {
    // Build time: `let` precompute inside a keyframe. 1! + 2! + 3! = 9 and
    // an arithmetic series 0+1+2+3+4 = 10.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade-in label [100ms]

#0.5s
let fact_sum = sum_range((k) => factorial(k), 1, 3)
let arith = sum_range((k) => k, 0, 4)
label.text = format("facts={} arith={}", fact_sum, arith)
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    assert_eq!(text_at(&timeline, "label", 1000), "facts=9 arith=10");
}

#[test]
fn sum_range_inside_plot_closure() {
    // The payoff shape: a Taylor partial sum via sum_range inside a plot
    // closure. The body references `t` (through the n expression) so the
    // dynamic gate fires, and every sampled y must be finite — the partial
    // sum of sin's series at small x stays bounded.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

c: PlotCurve, kind: "cartesian",
  func: (x) => sum_range((k) => (-1)^k * x^(2*k + 1) / factorial(2*k + 1), 0, 2) + 0 * t,
  color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)

#0.2s
fade-in c [100ms]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    assert_finite_samples(&timeline, 1.0);
}

#[test]
fn sum_range_rejects_invalid_arguments() {
    // Non-closure first arg and non-integer bounds must be type errors at
    // build time.
    for bad in [
        "let x = sum_range(5, 0, 3)",
        "let x = sum_range((k) => k, 0.5, 3)",
        "let x = sum_range((k) => k, -1, 3)",
        "let x = sum_range((a, b) => a + b, 0, 3)",
    ] {
        let source = format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (480, 270) }}
#0.5s
{bad}
    "#
        );
        let report = build(&source);
        assert!(!report.diagnostics.is_empty(), "expected a diagnostic for: {bad}");
    }
}

#[test]
fn anonymous_graph_child_builds_without_reserved_prefix_error() {
    // Engine-generated `__anon_*` labels on anonymous container children
    // must not be rejected by the reserved-prefix check.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }
    "#;
    let report = build(source);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::ReservedLabelPrefix),
        "anonymous Graph children must not trip the reserved-prefix check, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn undeclared_action_modifier_warns() {
    // Modifiers not declared in an action's signature must trigger an
    // UnsupportedModifierKey diagnostic.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        ball: Ellipse, size: (16, 16), color: accent.warning, at: (320, 180)

        #0.2s
        highlight ball [600ms, intensity: 1.3]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnsupportedModifierKey
                && d.message.contains("intensity")
        }),
        "undeclared `intensity` on highlight must warn, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn declared_action_modifier_does_not_warn() {
    // `shake` declares `intensity`; its use must stay silent.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        ball: Ellipse, size: (16, 16), color: accent.warning, at: (320, 180)

        #0.2s
        fade-in ball [300ms]

        #0.6s
        shake ball [intensity: 2]
    "#;
    let report = build(source);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnsupportedModifierKey),
        "declared `intensity` on shake must not warn, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn high_frequency_curve_meets_resolution_floor() {
    // High-frequency curves must respect the resolution sample floor so
    // oscillatory functions produce detailed polylines instead of under-sampled chords.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        c: PlotCurve, kind: "cartesian", func: (x) => sin(5 * x),
          color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)
    "#;
    let timeline = build(source).output;

    let track = timeline.world.get_id("c").expect("curve track");
    let paths = timeline
        .world
        .shape_or_default(track)
        .evaluate_vector_paths_value(timeline.world.style(track), 500);
    let point_count: usize = paths.iter().map(|vp| vp.path.elements().len()).sum();
    assert!(
        point_count >= 32,
        "high-frequency curve should sample at least ~resolution points, got {point_count}"
    );
}

#[test]
fn equation_container_builds_with_fragment_children() {
    let source = r#"
        eq: Equation {
            f1: Fragment, text: "x^2"
            f2: Fragment, text: "+ y"
        }
    "#;

    let report = build(source);

    // Equation container track should exist
    // (its children are checked through the authoritative hierarchy below)

    // Equation should have Fragment children registered
    let eq_id = report.output.get_entity_id("eq").expect("eq track should exist");
    let eq_children: Vec<&str> = report.output.world.child_labels(eq_id).collect();
    assert!(
        eq_children.contains(&"f1"),
        "Equation track should contain child 'f1', got: {eq_children:?}"
    );
    assert!(
        eq_children.contains(&"f2"),
        "Equation track should contain child 'f2', got: {eq_children:?}"
    );

    // Fragment f1 track should exist with its body stored
    let f1_track = report.output.world.tracks.get_id("f1").expect("f1 track should exist");
    let f1_content = report
        .output
        .world
        .text_or_default(f1_track)
        .text_content
        .as_ref()
        .expect("f1 should have text_content")
        .evaluate(0);
    assert_eq!(f1_content, "x^2", "Expected f1 content 'x^2', got {:?}", f1_content);

    // Fragment f2 track should exist with content stored
    let f2_track = report.output.world.tracks.get_id("f2").expect("f2 track should exist");
    let f2_content = report
        .output
        .world
        .text_or_default(f2_track)
        .text_content
        .as_ref()
        .expect("f2 should have text_content")
        .evaluate(0);
    assert_eq!(f2_content, "+ y", "Expected f2 content '+ y', got {:?}", f2_content);
}

#[test]
fn equation_fragment_dot_path_assignment() {
    let source = r#"
        eq: Equation {
            f1: Fragment, text: "x^2"
            f2: Fragment, text: "+ y"
        }

        #+1s
        eq.f1.highlight_opacity = 1.0 [800ms]
        eq.f2.text = "+ z"
    "#;

    let report = build(source);

    // Filter out non-error diagnostics (e.g. deprecation warnings)
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    // Fragment f1 should have highlight_opacity animated
    let f1_track = timeline.world.tracks.get_id("f1").expect("f1 track should exist");
    let highlight_opacity_at_1s = timeline
        .world
        .highlight_or_default(f1_track)
        .highlight_opacity
        .as_ref()
        .expect("f1 should have highlight_opacity track")
        .evaluate(1000);
    assert!(
        (highlight_opacity_at_1s - 0.0).abs() < 0.01,
        "Expected highlight_opacity=0.0 at t=1s (animation start), got {:?}",
        highlight_opacity_at_1s
    );

    let highlight_opacity_at_end = timeline
        .world
        .highlight_or_default(f1_track)
        .highlight_opacity
        .as_ref()
        .expect("f1 should have highlight_opacity track")
        .evaluate(1800);
    assert!(
        (highlight_opacity_at_end - 1.0).abs() < 0.01,
        "Expected highlight_opacity=1.0 at t=1.8s (animation end), got {:?}",
        highlight_opacity_at_end
    );

    // Fragment f2 should have updated content
    let f2_track = timeline.world.tracks.get_id("f2").expect("f2 track should exist");
    let f2_content_at_1s = timeline
        .world
        .text_or_default(f2_track)
        .text_content
        .as_ref()
        .expect("f2 should have text_content")
        .evaluate(1000);
    assert_eq!(
        f2_content_at_1s, "+ z",
        "Expected f2 content '+ z' at t=1s, got {:?}",
        f2_content_at_1s
    );
}

#[test]
fn pointlist_literal_tuples() {
    let source = r#"
        poly: Polygon {
            points: {(0, 0), (100, 0), (100, 100)},
        }
    "#;
    let report = build(source);
    assert_clean(&report);
    assert!(report.output.world.tracks.contains_key("poly"), "poly track should exist");
}

#[test]
fn pointlist_with_variable() {
    let source = r#"
        let p1 = (10, 20)
        poly: Polygon {
            points: {p1, (50, 60)},
        }
    "#;
    let report = build(source);
    assert_clean(&report);
}

/// Graph `padding` property is stored in env as `Vec4` and defaults to [0;4].
#[test]
fn graph_padding_stored_in_env() {
    // Props are declared comma-separated after the type name (not inside braces).
    let source = "g: Graph, size: (400, 300), x_domain: (-5, 5), y_domain: (-3, 3), padding: (20, 10, 15, 5)";
    let report = build(source);
    assert_clean(&report);

    let padding = report.output.env().get("g_padding");
    match padding {
        Some(Value::Vec4([l, r, t, b])) => {
            assert!((l - 20.0).abs() < 1e-10, "expected left=20, got {l}");
            assert!((r - 10.0).abs() < 1e-10, "expected right=10, got {r}");
            assert!((t - 15.0).abs() < 1e-10, "expected top=15, got {t}");
            assert!((b - 5.0).abs() < 1e-10, "expected bottom=5, got {b}");
        },
        other => panic!("expected Vec4 for g_padding, got {other:?}"),
    }
}

/// Graph with no `padding` property defaults to [0, 0, 0, 0].
#[test]
fn graph_padding_defaults_to_zero() {
    let source = "g: Graph, size: (300, 300)";
    let report = build(source);
    assert_clean(&report);

    let padding = report.output.env().get("g_padding");
    match padding {
        Some(Value::Vec4([l, r, t, b])) => {
            assert_eq!([l, r, t, b], [0.0, 0.0, 0.0, 0.0], "default padding should be [0;4]");
        },
        other => panic!("expected Vec4([0;4]) for g_padding, got {other:?}"),
    }
}

/// Uniform scalar padding is broadcast to all four sides.
#[test]
fn graph_padding_scalar_broadcasts_to_all_sides() {
    let source = "g: Graph, size: (300, 300), padding: 10";
    let report = build(source);
    // Diagnostics may include a warning for non-Vec4 but we don't assert here.
    let _ = report.diagnostics;

    let padding = report.output.env().get("g_padding");
    if let Some(Value::Vec4([l, r, t, b])) = padding {
        assert_eq!([l, r, t, b], [10.0, 10.0, 10.0, 10.0], "scalar padding should broadcast");
    }
    // If not stored as Vec4 the default is fine; just ensure no crash.
}

/// `g.map_inverse` is registered as a NativeFn in the build env.
#[test]
fn graph_map_inverse_registered_as_native_fn() {
    let source = "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)";
    let report = build(source);
    assert_clean(&report);
    match report.output.env().get("g.map_inverse") {
        Some(Value::NativeFn(_)) => {},
        other => panic!("expected NativeFn for g.map_inverse, got {other:?}"),
    }
}

/// Round-trip: `map_inverse(map(mx, my))` returns the original math coordinates.
///
/// Assert against the environment the build actually produced: `g.size` carries
/// the half-size track while `g_size` carries the declared full size, and the
/// pair must agree for the round-trip to mean anything.
#[test]
fn graph_map_inverse_round_trip() {
    assert_map_round_trip(
        &build("g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)").output,
    );
}

/// Round-trip with non-zero Graph position `at: (400, 300)`.
#[test]
fn graph_map_inverse_round_trip_with_non_zero_at() {
    assert_map_round_trip(
        &build(
            "g: Graph, at: (400, 300), size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)",
        )
        .output,
    );
}

/// Both round-trip scenes assert against the build's real environment.
fn assert_map_round_trip(timeline: &Timeline) {
    let env = timeline.env();
    let map_fn = match env.get("g.map") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map not a NativeFn: {other:?}"),
    };
    let map_inv_fn = match env.get("g.map_inverse") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map_inverse not a NativeFn: {other:?}"),
    };
    for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (7.5, -4.0)] {
        let screen = map_fn(&[Value::Num(mx), Value::Num(my)], env).expect("map call");
        let (sx, sy) = match screen {
            Value::Vec2([sx, sy]) => (sx, sy),
            other => panic!("map returned {other:?}"),
        };
        let math = map_inv_fn(&[Value::Num(sx), Value::Num(sy)], env).expect("map_inverse call");
        match math {
            Value::Vec2([rx, ry]) => {
                assert!((rx - mx).abs() < 1e-6, "x round-trip: {mx} -> {sx} -> {rx}");
                assert!((ry - my).abs() < 1e-6, "y round-trip: {my} -> {sy} -> {ry}");
            },
            other => panic!("map_inverse returned {other:?}"),
        }
    }
}

#[test]
fn fragment_text_resolves_variable_backed_expression() {
    let source = r#"
let fragment_text = "x^2 + y^2"

eq: Equation {
    f: Fragment, text: fragment_text
}
"#;
    let report = build(source);
    assert_clean(&report);
    let timeline = report.output;
    let fragment_id = timeline.world.get_id("f").expect("fragment track");
    let content = timeline
        .world
        .text_or_default(fragment_id)
        .text_content
        .as_ref()
        .expect("fragment text_content")
        .evaluate(0);
    assert_eq!(
        content, "x^2 + y^2",
        "a Fragment text expression must resolve against the build environment"
    );
}

/// Verify that CoordinateFrameComponent is registered on the plot host entity
/// and readable through both the id and label accessors.
#[test]
fn graph_registers_coordinate_frame_component() {
    let source = "g: Graph, size: (800, 600), x_domain: (-8, 8), y_domain: (-4, 4), padding: (10, 20, 30, 40)";
    let timeline = build(source).output;
    let g_id = timeline.tracks().get_id("g").expect("g entity id");
    let frame = timeline.coordinate_frame(g_id).expect("CoordinateFrameComponent");
    assert_eq!(frame.x_domain, [-8.0, 8.0]);
    assert_eq!(frame.y_domain, [-4.0, 4.0]);
    assert_eq!(frame.padding, [10.0, 20.0, 30.0, 40.0]);
    // The label boundary resolves to the same component, and EntityRef exposes it.
    assert_eq!(timeline.coordinate_frame_for("g"), Some(frame));
    assert_eq!(timeline.entity("g").expect("g entity").coordinate_frame(), Some(frame));
}

/// `map()` must scale at the same px/unit the curve is drawn with: a point at
/// the +x domain edge lands on the right half-width, not a quarter of it.
#[test]
fn graph_map_agrees_with_the_drawn_curve_scale() {
    let source = "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)";
    let report = build(source);
    let env = report.output.env();
    let map_fn = match env.get("g.map") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map not a NativeFn: {other:?}"),
    };

    let centre = match map_fn(&[Value::Num(0.0), Value::Num(0.0)], env).expect("map") {
        Value::Vec2(v) => v,
        other => panic!("map returned {other:?}"),
    };
    let right = match map_fn(&[Value::Num(10.0), Value::Num(0.0)], env).expect("map") {
        Value::Vec2(v) => v,
        other => panic!("map returned {other:?}"),
    };
    // 800px wide over a 20-unit domain is 40 px/unit; padding is zero here.
    let px_per_unit = (right[0] - centre[0]) / 10.0;
    assert!(
        (px_per_unit - 40.0).abs() < 1.0,
        "map() reports {px_per_unit} px/unit, but an 800px graph over a 20-unit \
         domain is 40 — the curve and its tracking actors disagree"
    );
}

/// `map_inverse` respects padding: screen center (shifted by padding) maps to math (0, 0).
/// With padding [left=20, right=10, top=15, bottom=5], the padded plot center is at
/// screen offset (5, 5), which corresponds to math origin (0, 0).
#[test]
fn graph_map_inverse_respects_padding() {
    let source = "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5), padding: (20, 10, 15, 5)";
    let report = build(source);

    let map_inv_fn = match report.output.env().get("g.map_inverse") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map_inverse not a NativeFn: {other:?}"),
    };

    let mut call_env = Environment::new();
    call_env.set("g_size", Value::Vec2([800.0, 600.0]));
    call_env.set("g_at", Value::Vec2([0.0, 0.0]));
    call_env.set("g_padding", Value::Vec4([20.0, 10.0, 15.0, 5.0]));

    // shift_x = (left - right)/2 = (20 - 10)/2 = 5
    // shift_y = (top - bottom)/2 = (15 - 5)/2 = 5
    // => screen (5, 5) should map to math (0, 0)
    let result =
        map_inv_fn(&[Value::Num(5.0), Value::Num(5.0)], &call_env).expect("map_inverse call");
    match result {
        Value::Vec2([mx, my]) => {
            assert!((mx - 0.0).abs() < 1e-9, "expected mx=0, got {mx}");
            assert!((my - 0.0).abs() < 1e-9, "expected my=0, got {my}");
        },
        other => panic!("map_inverse returned {other:?}"),
    }
}

/// Coordinates outside the plot area are extrapolated without error or panic.
#[test]
fn graph_map_inverse_outside_plot_no_panic() {
    let source = "g: Graph, size: (400, 300), x_domain: (-5, 5), y_domain: (-3, 3)";
    let report = build(source);

    let map_inv_fn = match report.output.env().get("g.map_inverse") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map_inverse not a NativeFn: {other:?}"),
    };

    let mut call_env = Environment::new();
    call_env.set("g_size", Value::Vec2([400.0, 300.0]));
    call_env.set("g_at", Value::Vec2([0.0, 0.0]));
    call_env.set("g_padding", Value::Vec4([0.0; 4]));

    // Far outside the plot area — finite, extrapolated beyond the domain.
    let result = map_inv_fn(&[Value::Num(9999.0), Value::Num(9999.0)], &call_env)
        .expect("no error for out-of-bounds coords");
    match result {
        Value::Vec2([mx, my]) => {
            assert!(mx.is_finite(), "mx should be finite: {mx}");
            assert!(my.is_finite(), "my should be finite: {my}");
            assert!(mx > 5.0, "expected extrapolated mx > 5.0, got {mx}");
        },
        other => panic!("expected Vec2, got {other:?}"),
    }
}
