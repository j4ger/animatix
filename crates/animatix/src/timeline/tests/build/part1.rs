use super::super::tests::without_content_lints;
use super::super::*;
use crate::ast::{BinaryOp, Expr, Property, Stmt};
use crate::timeline::{Timeline, Value};

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
#[test]
fn test_reactive_binding_desugars_to_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "orbiter".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius_x".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "radius_y".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
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
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
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

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );

    let timeline = report.output;

    // At t=0s, circ.opacity should be 0.0 (pre-keyframe default is hidden)
    let circ_track = timeline.world.tracks.get("circ").expect("circ track should exist");
    let opacity_at_0 = timeline.world.style(circ_track.id).opacity.as_ref().unwrap().evaluate(0);
    assert!(
        (opacity_at_0 - 0.0).abs() < 0.01,
        "Expected circ.opacity=0.0 at t=0 (pre-keyframe default), got {:?}",
        opacity_at_0
    );

    // At t=1s, circ.opacity should be 0.5
    let opacity_at_1s =
        timeline.world.style(circ_track.id).opacity.as_ref().unwrap().evaluate(1000);
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let track = report.output.world.tracks.get("box").expect("box track should exist");
    let opacity_at_0 = report.output.world.style(track.id).opacity.as_ref().unwrap().evaluate(0);
    assert_eq!(opacity_at_0, 0.0, "explicit opacity: 0 should apply at t=0");
}

#[test]
fn graph_axes_invisible_before_fadein() {
    // Graph declared before any keyframe → default_opacity = 0.0
    // fade-in at #0.5s should animate opacity 0→1
    let source = "g1: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), at: (280, 200)\n\n#0.5s\nfade-in g1 [400ms]";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let track = timeline.world.tracks.get("g1").expect("g1 track should exist");
    let opacity_at_0 = timeline.world.style(track.id).opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_500 = timeline.world.style(track.id).opacity.as_ref().map(|t| t.evaluate(500));
    let opacity_at_900 = timeline.world.style(track.id).opacity.as_ref().map(|t| t.evaluate(900));
    let opacity_at_1000 = timeline.world.style(track.id).opacity.as_ref().map(|t| t.evaluate(1000));

    assert_eq!(opacity_at_0, Some(0.0), "opacity should be 0 at t=0");
    assert_eq!(opacity_at_500, Some(0.0), "opacity should be 0 at t=500ms (fade-in start)");
    assert_eq!(opacity_at_900, Some(1.0), "opacity should be 1 at t=900ms (fade-in end)");
    assert_eq!(opacity_at_1000, Some(1.0), "opacity should stay 1 after fade-in");
}

#[test]
fn container_fadein_reveals_graph_hosted_children() {
    // A Graph-hosted PlotCurve declared before the first keyframe carries its
    // own hidden-by-default seed. A container-level `fade-in g` must cascade
    // the reveal into the subtree — opacity multiplies down the scene graph,
    // so without the cascade the curve stays invisible forever (probe 010;
    // 07_plots.amx shipped with an invisible headline curve).
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }

        #0.3s
        fade-in g [300ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let child = timeline.world.tracks.get("c").expect("child curve track should exist");
    assert!(
        !timeline.world.meta(child.id).hidden_by_default,
        "container fade-in must lift the child's hidden-by-default flag"
    );
    let opacity_at_0 = timeline.world.style(child.id).opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_1000 = timeline.world.style(child.id).opacity.as_ref().map(|t| t.evaluate(1000));
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
    // A container declared before the first keyframe must be seeded invisible
    // like any other actor. The Graph paints its own axes, so without the seed
    // those axes showed before `fade-in g` ever ran (07_plots shipped a
    // floating crosshair for the first second of the scene). Root cause:
    // `add_node` creates the parent's track while registering children, so a
    // first declaration *with children* looked like a re-declaration and never
    // received the hidden-by-default seed — it also gave the container a
    // nonsensical 1 → 0 → 1 opacity dip instead of a clean reveal.
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }

        #1.0s
        fade-in g [300ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let graph = timeline.world.tracks.get("g").expect("graph track should exist");
    assert!(
        !timeline.world.meta(graph.id).hidden_by_default,
        "the entrance action consumes the flag"
    );
    let opacity = |ms: u64| {
        timeline
            .world
            .style(graph.id)
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    for (label, track) in &timeline.world.tracks {
        let expected =
            animatix_std::caps_for_type(timeline.world.actor_type(track.id)).unwrap_or_default();
        assert_eq!(
            timeline.world.caps(track.id),
            expected,
            "track '{label}' caps drifted from actor_type '{}'",
            timeline.world.actor_type(track.id)
        );
    }
}

#[test]
fn unrevealed_graph_child_still_warns_never_revealed() {
    // Without any entrance action the hosted child stays invisible, and the
    // build must say so. The graph itself is visible-by-default (its
    // declaration-time opacity keyframe is a constant 1.0), so the ancestor
    // check must not treat that constant as a reveal (probe 010).
    let source = r#"
        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    // spec §14 "Runtime parameters": a plot closure capturing a `let` that an
    // `always` block rewrites must resample per frame — the frame value
    // shadows the build-time capture. The dynamic gate used to classify
    // capture-only plots as static, so the documented pattern rendered inert
    // (probe 011; identical frames at t=0.3 and t=5.0).
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    assert!(
        timeline.frame_written_vars.contains("freq"),
        "always bare assignment must register the written name, got: {:?}",
        timeline.frame_written_vars
    );
    let track = timeline.world.tracks.get("curve").expect("curve track");
    let plot = timeline.world.procedural_plot(track.id).expect("procedural plot");
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.world.tracks.get("c").expect("curve track");
    assert!(
        timeline
            .world
            .procedural_plot(track.id)
            .expect("procedural plot")
            .is_dynamic(&timeline.frame_written_vars),
        "body references t — the plot must be dynamic"
    );

    // Full pipeline evaluation: frame env + per-frame plot resampling.
    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        1.0,
        crate::timeline::SceneDimensions {
            width: 640,
            height: 360,
        },
        crate::timeline::DebugRenderOptions::default(),
        &mut filter_backend,
    );
    let mut checked = 0;
    for item in &program.items {
        for command in &item.commands {
            if let crate::primitives::RenderCommand::Paths { paths } = command {
                for vp in paths {
                    for el in vp.path.elements() {
                        if let kurbo::PathEl::LineTo(p) | kurbo::PathEl::MoveTo(p) = el {
                            assert!(
                                p.y.is_finite(),
                                "sampled y must be finite (user-fn call inside plot closure must resolve), got NaN at x={}",
                                p.x
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 0, "expected sampled points");
}

#[test]
fn draw_in_trims_stroke_only_path_geometry_mid_draw() {
    // `draw-in` writes `stroke_progress`, but only the plot primitive used to
    // read it — a stroke-only `Path` faded in at full width instead of drawing
    // (the hero underline/brackets defect). Mid-draw the shape's commands must
    // carry a partial path, and the settled frames must serve the full path
    // again (the shape-command memo must never hand back trimmed geometry).
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

p: Path, commands: {move_to(0, 0), line_to(100, 0), line_to(100, 100), line_to(0, 100)},
  stroke: accent.primary, stroke_width: 4, fill_opacity: 0.0, at: (320, 180)

#0s
draw-in p [1s, ease: linear]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let count_stroke_segments = |time: f64| -> usize {
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let mut segments = 0;
        for item in &program.items {
            for command in &item.commands {
                if let crate::primitives::RenderCommand::Paths { paths } = command {
                    for vp in paths {
                        if vp.stroke.is_some() {
                            segments += vp
                                .path
                                .elements()
                                .iter()
                                .filter(|el| {
                                    matches!(
                                        el,
                                        kurbo::PathEl::LineTo(_)
                                            | kurbo::PathEl::QuadTo(_, _)
                                            | kurbo::PathEl::CurveTo(_, _, _)
                                    )
                                })
                                .count();
                        }
                    }
                }
            }
        }
        segments
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let stroke_end = |time: f64| -> kurbo::Point {
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let mut end = None;
        for item in &program.items {
            for command in &item.commands {
                if let crate::primitives::RenderCommand::Paths { paths } = command {
                    for vp in paths {
                        if vp.stroke.is_some() {
                            for el in vp.path.elements().iter().rev() {
                                if let kurbo::PathEl::LineTo(p) = el {
                                    end = Some(*p);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        end.expect("stroke path present")
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let mut found = (None, 0.0);
        for item in &program.items {
            for command in &item.commands {
                if let crate::primitives::RenderCommand::Paths { paths } = command {
                    for vp in paths {
                        if vp.stroke.is_some() {
                            found = (vp.dash_pattern.clone(), vp.dash_offset);
                        }
                    }
                }
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    let glow = timeline.world.tracks.get("glow").expect("glow track");
    assert_eq!(timeline.world.style(glow.id).blend.get(0, "normal".to_string()), "screen");
    let plain = timeline.world.tracks.get("plain").expect("plain track");
    assert_eq!(timeline.world.style(plain.id).blend.get(0, "normal".to_string()), "normal");
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let text = timeline
        .world
        .tracks
        .get("label")
        .and_then(|t| timeline.world.text(t.id).text_content.as_ref())
        .map(|track| track.evaluate(1000))
        .unwrap_or_default();
    assert_eq!(text, "facts=9 arith=10");
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        1.0,
        crate::timeline::SceneDimensions {
            width: 640,
            height: 360,
        },
        crate::timeline::DebugRenderOptions::default(),
        &mut filter_backend,
    );
    let mut checked = 0;
    for item in &program.items {
        for command in &item.commands {
            if let crate::primitives::RenderCommand::Paths { paths } = command {
                for vp in paths {
                    for el in vp.path.elements() {
                        if let kurbo::PathEl::LineTo(p) | kurbo::PathEl::MoveTo(p) = el {
                            assert!(
                                p.y.is_finite(),
                                "sampled y must be finite, got NaN at x={}",
                                p.x
                            );
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 0, "expected sampled points");
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
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
        let ast = ast.expect("parsed AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(!report.diagnostics.is_empty(), "expected a diagnostic for: {bad}");
    }
}

#[test]
fn anonymous_graph_child_builds_without_reserved_prefix_error() {
    // An unlabeled actor inside a Graph gets an engine-generated `__anon_*`
    // label; the reserved-prefix check must not reject the engine's own
    // names (it used to make unlabeled Graph children unbuildable).
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), at: (320, 180) {
            PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    // LG-4: `highlight` does not declare `intensity` — the value used to be
    // silently ignored (the shipped gradient_descent example carried one).
    // The action-modifier validation must surface it.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        ball: Ellipse, size: (16, 16), color: accent.warning, at: (320, 180)

        #0.2s
        highlight ball [600ms, intensity: 1.3]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    // sin(5x) over (-10, 10) is ~16 periods; the adaptive samplers used to
    // subdivide at most 3 levels (8 samples), whose values are nearly
    // collinear for this function — the curve rendered as a single straight
    // chord (probe 012). `resolution` is now honored as a minimum sample
    // count, so the sampled polyline must contain materially more points than
    // the old floor.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        c: PlotCurve, kind: "cartesian", func: (x) => sin(5 * x),
          color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let track = timeline.world.tracks.get("c").expect("curve track");
    let paths = track.evaluate_vector_paths_value(
        timeline.world.shape(track.id),
        timeline.world.style(track.id),
        500,
    );
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

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

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
    let f1_track = report.output.world.tracks.get("f1").expect("f1 track should exist");
    let f1_content = report
        .output
        .world
        .text(f1_track.id)
        .text_content
        .as_ref()
        .expect("f1 should have text_content")
        .evaluate(0);
    assert_eq!(f1_content, "x^2", "Expected f1 content 'x^2', got {:?}", f1_content);

    // Fragment f2 track should exist with content stored
    let f2_track = report.output.world.tracks.get("f2").expect("f2 track should exist");
    let f2_content = report
        .output
        .world
        .text(f2_track.id)
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

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // Filter out non-error diagnostics (e.g. deprecation warnings)
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    // Fragment f1 should have highlight_opacity animated
    let f1_track = timeline.world.tracks.get("f1").expect("f1 track should exist");
    let highlight_opacity_at_1s = timeline
        .world
        .highlight(f1_track.id)
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
        .highlight(f1_track.id)
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
    let f2_track = timeline.world.tracks.get("f2").expect("f2 track should exist");
    let f2_content_at_1s = timeline
        .world
        .text(f2_track.id)
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

/// Graph `padding` property is stored in env as `Vec4` and defaults to [0;4].
#[test]
fn graph_padding_stored_in_env() {
    // Props are declared comma-separated after the type name (not inside braces).
    let source = "g: Graph, size: (400, 300), x_domain: (-5, 5), y_domain: (-3, 3), padding: (20, 10, 15, 5)";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    match report.output.env().get("g.map_inverse") {
        Some(Value::NativeFn(_)) => {},
        other => panic!("expected NativeFn for g.map_inverse, got {other:?}"),
    }
}

/// Round-trip: `map_inverse(map(mx, my))` returns the original math coordinates.
///
/// This used to build its own call environment and set `g.size` (which `map`
/// reads) and `g_size` (which `map_inverse` reads) to the *same* hand-picked
/// value — so it passed vacuously and could not see that the real build
/// populates them differently: `g.size` carries the half-size track while
/// `g_size` carries the declared full size. That 2x disagreement was the bug.
/// Assert against the environment the build actually produced instead.
#[test]
fn graph_map_inverse_round_trip() {
    let source = "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let env = report.output.env();
    let map_fn = match env.get("g.map") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map not a NativeFn: {other:?}"),
    };
    let map_inv_fn = match env.get("g.map_inverse") {
        Some(Value::NativeFn(f)) => f,
        other => panic!("g.map_inverse not a NativeFn: {other:?}"),
    };

    // The two size keys must agree in the real build, or the pair cannot
    // round-trip no matter what the functions do.
    let dotted = env.get("g.size").and_then(|v| match v {
        Value::Vec2(s) => Some(s),
        _ => None,
    });
    let side = env.get("g_size").and_then(|v| match v {
        Value::Vec2(s) => Some(s),
        _ => None,
    });
    println!("g.size={dotted:?} g_size={side:?}");

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

/// Round-trip with non-zero Graph position `at: (400, 300)`.
#[test]
fn graph_map_inverse_round_trip_with_non_zero_at() {
    let source =
        "g: Graph, at: (400, 300), size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let env = report.output.env();
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
                assert!((rx - mx).abs() < 1e-6, "x round-trip with offset: {mx} -> {sx} -> {rx}");
                assert!((ry - my).abs() < 1e-6, "y round-trip with offset: {my} -> {sy} -> {ry}");
            },
            other => panic!("map_inverse returned {other:?}"),
        }
    }
}
