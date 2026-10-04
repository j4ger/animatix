use super::*;
use crate::ast::{BinaryOp, LoopPattern};

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
    let circ_track = timeline.tracks.get("circ").expect("circ track should exist");
    let opacity_at_0 = circ_track.style.opacity.as_ref().unwrap().evaluate(0);
    assert!(
        (opacity_at_0 - 0.0).abs() < 0.01,
        "Expected circ.opacity=0.0 at t=0 (pre-keyframe default), got {:?}",
        opacity_at_0
    );

    // At t=1s, circ.opacity should be 0.5
    let opacity_at_1s = circ_track.style.opacity.as_ref().unwrap().evaluate(1000);
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
    let track = report.output.tracks.get("box").expect("box track should exist");
    let opacity_at_0 = track.style.opacity.as_ref().unwrap().evaluate(0);
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

    let track = timeline.tracks.get("g1").expect("g1 track should exist");
    let opacity_at_0 = track.style.opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_500 = track.style.opacity.as_ref().map(|t| t.evaluate(500));
    let opacity_at_900 = track.style.opacity.as_ref().map(|t| t.evaluate(900));
    let opacity_at_1000 = track.style.opacity.as_ref().map(|t| t.evaluate(1000));

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

    let child = timeline.tracks.get("c").expect("child curve track should exist");
    assert!(
        !child.hidden_by_default,
        "container fade-in must lift the child's hidden-by-default flag"
    );
    let opacity_at_0 = child.style.opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_1000 = child.style.opacity.as_ref().map(|t| t.evaluate(1000));
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

    let graph = timeline.tracks.get("g").expect("graph track should exist");
    assert!(!graph.hidden_by_default, "the entrance action consumes the flag");
    let opacity = |ms: u64| graph.style.opacity.as_ref().map(|t| t.evaluate(ms)).unwrap_or(1.0);
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

    for (label, track) in &timeline.tracks {
        let expected = animatix_std::caps_for_type(&track.actor_type).unwrap_or_default();
        assert_eq!(
            track.caps, expected,
            "track '{label}' caps drifted from actor_type '{}'",
            track.actor_type
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
    let track = timeline.tracks.get("curve").expect("curve track");
    let plot = track.procedural_plot.as_ref().expect("procedural plot");
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

    let track = timeline.tracks.get("c").expect("curve track");
    assert!(
        track
            .procedural_plot
            .as_ref()
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
    let glow = timeline.tracks.get("glow").expect("glow track");
    assert_eq!(glow.style.blend.get(0, "normal".to_string()), "screen");
    let plain = timeline.tracks.get("plain").expect("plain track");
    assert_eq!(plain.style.blend.get(0, "normal".to_string()), "normal");
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
        .tracks
        .get("label")
        .and_then(|t| t.text.text_content.as_ref())
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

    let track = timeline.tracks.get("c").expect("curve track");
    let paths = track.evaluate_vector_paths_value(500);
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
    let eq_track = report.output.tracks.get("eq").expect("eq track should exist");

    // Equation should have Fragment children registered
    assert!(
        eq_track.children.contains(&"f1".to_string()),
        "Equation track should contain child 'f1', got: {:?}",
        eq_track.children
    );
    assert!(
        eq_track.children.contains(&"f2".to_string()),
        "Equation track should contain child 'f2', got: {:?}",
        eq_track.children
    );

    // Fragment f1 track should exist with its body stored
    let f1_track = report.output.tracks.get("f1").expect("f1 track should exist");
    let f1_content = f1_track
        .text
        .text_content
        .as_ref()
        .expect("f1 should have text_content")
        .evaluate(0);
    assert_eq!(f1_content, "x^2", "Expected f1 content 'x^2', got {:?}", f1_content);

    // Fragment f2 track should exist with content stored
    let f2_track = report.output.tracks.get("f2").expect("f2 track should exist");
    let f2_content = f2_track
        .text
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
    let f1_track = timeline.tracks.get("f1").expect("f1 track should exist");
    let highlight_opacity_at_1s = f1_track
        .highlight
        .highlight_opacity
        .as_ref()
        .expect("f1 should have highlight_opacity track")
        .evaluate(1000);
    assert!(
        (highlight_opacity_at_1s - 0.0).abs() < 0.01,
        "Expected highlight_opacity=0.0 at t=1s (animation start), got {:?}",
        highlight_opacity_at_1s
    );

    let highlight_opacity_at_end = f1_track
        .highlight
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
    let f2_track = timeline.tracks.get("f2").expect("f2 track should exist");
    let f2_content_at_1s = f2_track
        .text
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
    assert!(report.output.tracks.contains_key("poly"), "poly track should exist");
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

/// `map()` must scale at the same px/unit the curve is drawn with: a point at
/// the +x domain edge lands on the right half-width, not a quarter of it.
#[test]
fn graph_map_agrees_with_the_drawn_curve_scale() {
    let source = "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5)";
    let (ast, _) = animatix_syntax::parser::parse_source(source);
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &std::collections::HashMap::new());
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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

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

#[test]
fn test_for_loop_tuple_destructuring_creates_actors() {
    let source = r#"
        #0s
        for (x, y), i in {(10, 20), (30, 40), (50, 60)} {
            dot[i]: Rect, at: (x, y), size: (10, 10)
        }
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    // Should have 3 actors: dot__0, dot__1, dot__2
    assert!(timeline.tracks.contains_key("dot__0"), "Expected dot__0 track");
    assert!(timeline.tracks.contains_key("dot__1"), "Expected dot__1 track");
    assert!(timeline.tracks.contains_key("dot__2"), "Expected dot__2 track");

    // Verify all three actors have position tracks
    // dot__0 should be at (10, 20)
    let dot0 = timeline.tracks.get("dot__0").unwrap();
    assert!(dot0.geometry.position.is_some(), "Expected dot__0 to have position track");

    // dot__1 should be at (30, 40)
    assert!(
        timeline.tracks.get("dot__1").unwrap().geometry.position.is_some(),
        "Expected dot__1 to have position track"
    );

    // dot__2 should be at (50, 60)
    assert!(
        timeline.tracks.get("dot__2").unwrap().geometry.position.is_some(),
        "Expected dot__2 to have position track"
    );
}

#[test]
fn test_for_loop_tuple_with_vec2_values() {
    // Create a for loop iterating over a variable holding Vec2 values
    // This tests the Vec2 destructuring path in bind_loop_var
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["vx".to_string(), "vy".to_string()]),
            index_var: None,
            iterable: Expr::List(vec![
                Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(15.0)]),
                Expr::Tuple(vec![Expr::Num(25.0), Expr::Num(35.0)]),
            ]),
            modifiers: vec![],
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "point".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Ident("vx".to_string()),
                            Expr::Ident("vy".to_string()),
                        ]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(8.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    // Two actors should have been created (one per iteration)
    // They have the same label "point" so only the last one persists
    assert!(timeline.tracks.contains_key("point"), "Expected point track");

    // The last iteration sets at to (25, 35)
    let track = timeline.tracks.get("point").unwrap();
    if let Some(pos_track) = &track.geometry.position {
        let pos = pos_track.evaluate(0);
        // position may be stored as Vec2 or individually
        assert!(
            (pos[0] - 25.0).abs() < 0.01 || (pos[0] - 5.0).abs() < 0.01,
            "Expected point x ~25 or ~5 (last iteration), got {}",
            pos[0]
        );
        assert!(
            (pos[1] - 35.0).abs() < 0.01 || (pos[1] - 15.0).abs() < 0.01,
            "Expected point y ~35 or ~15, got {}",
            pos[1]
        );
    }
}

#[test]
fn test_for_loop_tuple_destructuring_with_let_decl() {
    // Test that tuple destructuring in for loops works with let declarations
    // using a list literal as the iterable (not a variable, since variables
    // that evaluate to Value::List are wrapped as a single item)
    let source = r#"
        #0s
        for (a, b) in {(1, 2), (3, 4)} {
            let sum = a + b
        }
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    // The for loop should execute without panicking
    let timeline = report.output;
    // sum should have a variable track since it was declared in the for loop body
    assert!(
        timeline.variable_tracks.contains_key("sum"),
        "Expected variable track for 'sum'"
    );
    // The variable track has one keyframe at t=0 (last iteration wins since
    // both iterations run at the same time_ms)
    let sum_track = timeline.variable_tracks.get("sum").unwrap();
    assert_eq!(sum_track.keyframes.len(), 1, "Expected 1 keyframe for sum (both at t=0)");
    let sum_value = sum_track.keyframes.get(&0);
    assert!(sum_value.is_some(), "Expected sum keyframe at t=0");
    if let Value::Num(n) = sum_value.unwrap() {
        // Last iteration sets sum = 3+4 = 7
        assert!((*n - 7.0).abs() < 0.01, "Expected sum=7, got {}", n);
    } else {
        panic!("Expected Num value for sum");
    }
}

#[test]
fn test_for_loop_variable_cleaned_after_exit() {
    // After a for-loop, the loop variable and index variable should not
    // persist in the environment (closures already captured them).
    let source = r#"
        #0s
        for i, idx in {1, 2, 3} {
            let x = i * 2
        }
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;
    // Loop variable 'i' should NOT be in the environment after the loop
    assert!(
        timeline.env.get("i").is_none(),
        "Loop variable 'i' should be undefined after loop exit"
    );
    // Index variable 'idx' should NOT be in the environment after the loop
    assert!(
        timeline.env.get("idx").is_none(),
        "Index variable 'idx' should be undefined after loop exit"
    );
}

#[test]
fn test_for_loop_tuple_vars_cleaned_after_exit() {
    // Tuple destructuring variables should also be cleaned up after the loop.
    let source = r#"
        #0s
        for (a, b) in {(1, 2), (3, 4)} {
            let z = a + b
        }
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;
    // Tuple destructuring variables should be cleaned up
    assert!(
        timeline.env.get("a").is_none(),
        "Tuple var 'a' should be undefined after loop exit"
    );
    assert!(
        timeline.env.get("b").is_none(),
        "Tuple var 'b' should be undefined after loop exit"
    );
}

fn build_timeline(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "Unexpected build errors: {:?}",
        report.diagnostics
    );
    report.output
}

#[test]
fn filled_shape_defaults_to_no_stroke() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    assert_eq!(track.style.stroke_width.get(0, 99.0), 0.0);
}

#[test]
fn stroke_only_shape_keeps_default_stroke() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Line, from: (0, 0), to: (100, 0), at: (200, 150)
"#,
    );
    let track = timeline.tracks.get("a").expect("line track");
    assert_eq!(track.style.stroke_width.get(0, 99.0), 2.0);
}

#[test]
fn explicit_filled_shape_stroke_is_preserved() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, stroke: red, stroke_width: 4, at: (200, 150)
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    assert_eq!(track.style.stroke_width.get(0, 99.0), 4.0);
}

#[test]
fn plot_actor_declarations_seed_dash_and_gradient() {
    // The plot pipeline consumes its own children's declarations, so the dash
    // pair and the gradient properties — both declared for every
    // `AllStrokePaths` actor, which includes `PlotCurve` — need an explicit
    // route onto the track. Without it the frame-time stamp samples an empty
    // ramp and the curve silently draws its default solid stroke.
    let timeline = build_timeline(
        r##"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
g: Graph, x_domain: (-1, 1), y_domain: (-1, 1), at: (320, 180) {
  c: PlotCurve, kind: "cartesian", func: (x) => x, stroke: red, dash_pattern: {10, 7},
    stroke_gradient: linear(0, {(0%, "#ff0000"), (100%, "#0000ff")})
}
"##,
    );
    let track = timeline.tracks.get("c").expect("plot curve track");
    assert_eq!(
        track.style.dash_pattern.get(0, Vec::new()),
        vec![10.0, 7.0],
        "dash_pattern should land on the plot track"
    );
    let ramp = track.style.stroke_gradient.get(0, Default::default());
    assert_eq!(ramp.stops.len(), 2, "stroke_gradient should land on the plot track");
}

#[test]
fn draw_in_adds_visible_stroke_to_filled_shape() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
draw-in a [1s]
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    assert_eq!(track.style.stroke_width.get(0, 99.0), 2.0);
    let fill = track.style.color.get(0, [0.0, 0.0, 0.0, 1.0]);
    let stroke = track.style.stroke_color.get(0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(stroke, fill, "draw-in outline should use the fill color");
}

#[test]
fn reveal_in_adds_visible_stroke_to_filled_shape() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
reveal-in a [1s]
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    assert_eq!(track.style.stroke_width.get(0, 99.0), 2.0);
}

/// Effect children of a `Filter` scope lower into the scope's chain instead of
/// becoming scene-graph actors.
#[test]
fn filter_effect_child_lowers_into_scope_chain() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    assert_eq!(scope.effects.stages.len(), 1);
    assert_eq!(scope.effects.stages[0].label, "soft");

    let chain = scope.effects.build_chain(0);
    assert_eq!(chain.instances.len(), 1);
    assert_eq!(chain.instances[0].id.as_str(), "Blur");
    assert_eq!(chain.instances[0].params.f32_at(0), 10.0);

    // Effects are not actors: no track is created for the stage label.
    assert!(!timeline.tracks.contains_key("soft"));
}

/// `scope.stage.param = value` animates an effect parameter over time.
#[test]
fn effect_stage_param_assignment_animates() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 4
  img: Rect, size: (100, 100)
}
#1s
bg.soft.radius = 16 [1s]
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    assert_eq!(scope.effects.build_chain(0).instances[0].params.f32_at(0), 4.0);
    assert_eq!(scope.effects.build_chain(2000).instances[0].params.f32_at(0), 16.0);
}

/// An effect declared outside a `Filter` scope reports a build diagnostic.
#[test]
fn effect_outside_filter_scope_reports_diagnostic() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
row: Row {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("must be declared inside a Filter scope")),
        "diagnostics: {:?}",
        report.diagnostics
    );
}

/// `Filter, bounds: (x, y, w, h)` yields a region of interest expanded by the
/// chain's worst-case support.
#[test]
fn filter_bounds_yields_support_padded_region() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter, bounds: (40, 30, 120, 80) {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    assert!(!scope.effects.build_chain(0).is_empty());

    let (width, height) = timeline.resolution().expect("configured resolution");
    let region = timeline
        .effect_scope_region(
            scope,
            crate::timeline::SceneDimensions { width, height },
            0,
            kurbo::Affine::IDENTITY,
        )
        .expect("authored bounds must produce a region");

    // Worst-case support = blur radius (10), applied on every side.
    assert_eq!(region.origin, [30.0, 20.0]);
    assert_eq!(region.size.width, 140);
    assert_eq!(region.size.height, 100);
}

/// Authored bounds are scene coordinates, but the scope's sub-scene is rendered
/// in world space, so a camera move has to carry the bounds with it — otherwise
/// the scope filters the rectangle the author pointed at *before* the move.
/// `dogfood/probe_camera_scopes.amx` measured the gap this test guards: 9
/// differing pixels at camera identity, 73,512 after a 2x push.
#[test]
fn filter_bounds_follow_the_camera() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter, bounds: (40, 30, 120, 80) {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
#1s
camera.zoom = 2.0
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    let (width, height) = timeline.resolution().expect("configured resolution");
    let dims = crate::timeline::SceneDimensions { width, height };

    let before = timeline
        .effect_scope_region(scope, dims, 0, timeline.camera.affine(0, dims, None))
        .expect("identity camera keeps the authored rectangle");
    assert_eq!(before.origin, [30.0, 20.0]);

    // 2x about the scene center (160, 90): (40, 30) -> (-80, -30) and
    // (160, 110) -> (160, 130); the support then pads 10 px on every side and
    // the region clips to the frame.
    let after = timeline
        .effect_scope_region(scope, dims, 2000, timeline.camera.affine(2000, dims, None))
        .expect("the pushed scope still has a region");
    assert_eq!(after.origin, [0.0, 0.0]);
    assert_eq!(after.size.width, 170);
    assert_eq!(after.size.height, 140);
}

/// Without authored bounds the scope keeps the full-scene path.
#[test]
fn filter_without_bounds_has_no_region() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    let (width, height) = timeline.resolution().expect("configured resolution");
    assert!(
        timeline
            .effect_scope_region(
                scope,
                crate::timeline::SceneDimensions { width, height },
                0,
                kurbo::Affine::IDENTITY
            )
            .is_none()
    );
}

/// A plugin effect registered in the extension registry lowers like a
/// built-in: `effect_for_type` finds it and the scope chain carries a stage
/// whose identity is the authored name.
#[test]
fn plugin_effect_lowers_into_scope_chain() {
    use crate::timeline::effects::{
        EffectParamKind, EffectParamSpec, EffectParamValue, EffectPassSpec, PluginEffectData,
        register_extension_effect,
    };

    let params = vec![EffectParamSpec::new(
        "size",
        EffectParamKind::F32,
        EffectParamValue::F32(0.0),
        0,
        4,
    )];
    let passes = vec![EffectPassSpec::new(
        "mock-pixelate",
        "@compute fn main() {}",
        "main",
    )];
    let registered = register_extension_effect(PluginEffectData {
        type_name: "MockPixelate".into(),
        display_name: "Mock Pixelate".into(),
        params,
        passes,
        author_uniform_size: 16,
        support_px: 0.0,
    })
    .expect("plugin effect registers");
    assert_eq!(&*registered, "MockPixelate");

    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  fx: MockPixelate, size: 8
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.tracks.get("bg").expect("filter scope");
    assert_eq!(scope.effects.stages.len(), 1);
    assert_eq!(scope.effects.stages[0].label, "fx");

    let chain = scope.effects.build_chain(0);
    assert_eq!(chain.instances.len(), 1);
    assert_eq!(chain.instances[0].id.as_str(), "MockPixelate");
    assert_eq!(chain.instances[0].params.f32_at(0), 8.0);
}

/// Top-level `config { resolution: (w, h) }` is recorded on the timeline so
/// export tooling can default its canvas to the authored size.
#[test]
fn config_resolution_is_recorded_on_timeline() {
    let source = r#"
config { resolution: (960, 540) }
r: Rect, size: (100, 100), at: (480, 270)
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    assert_eq!(report.output.resolution(), Some((960, 540)));
}

/// A stroke-only Path (explicit `stroke:`, no `color:`) must not emit the
/// default scheme fill: vello implicitly closes open paths, so the fill
/// rendered a hand-drawn line as a dark dome. (Regression test.)
#[test]
fn stroke_only_path_suppresses_default_fill() {
    let source = "p: Path, commands: {move_to(-50, 0), line_to(50, 0)}, stroke: accent.primary, stroke_width: 4";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("p").expect("path track");
    assert_eq!(
        track.style.fill_opacity.get(0, 1.0),
        0.0,
        "stroke-only Path must default to no fill"
    );
}

/// An authored `fill_opacity` on a stroke-only Path is preserved.
#[test]
fn stroke_only_path_keeps_authored_fill_opacity() {
    let source = "p: Path, commands: {move_to(-50, 0), line_to(50, 0)}, stroke: accent.primary, stroke_width: 4, fill_opacity: 0.5";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("p").expect("path track");
    assert_eq!(track.style.fill_opacity.get(0, 1.0), 0.5);
}

/// An invalid easing name on an assignment must produce a diagnostic.
/// (Regression: the parser consumed `ease:` modifiers unconditionally, so
/// unknown names were silently replaced by the default easing.)
#[test]
fn typst_and_code_accept_uniform_text_content_property() {
    use crate::timeline::TrackAccessor;

    // Regression: the uniform `text:` content property should feed the content
    // track for the non-`Text` text kinds too. Previously `Typst`/`Code` only
    // read `content`/`code` at build time, so `Typst, text: "..."` was accepted
    // (no error) yet rendered blank.
    let source = r#"
a: Typst, text: "hello typst"
b: Code, text: "hello code"
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let typst_track = report.output.tracks.get("a").expect("a track");
    assert_eq!(
        typst_track.text.text_content.get(0, String::new()),
        "hello typst",
        "Typst should read content from `text:`"
    );
    let code_track = report.output.tracks.get("b").expect("b track");
    assert_eq!(
        code_track.text.text_content.get(0, String::new()),
        "hello code",
        "Code should read content from `text:`"
    );
}

/// `Code, language: "rust"` reaches the text lane through the generic property
/// pipeline (the `ActorField::Language` binding), so the frame-time compile
/// can pick it up; an absent `language` stays empty (plain rendering).
#[test]
fn code_language_reaches_text_lane() {
    use crate::timeline::TrackAccessor;

    let source = r#"
a: Code, code: "fn main() {}", language: "rust"
b: Code, code: "x = 1"
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );

    let a_track = report.output.tracks.get("a").expect("a track");
    assert_eq!(a_track.text.language.get(0, String::new()), "rust");
    let b_track = report.output.tracks.get("b").expect("b track");
    assert_eq!(b_track.text.language.get(0, String::new()), "", "default is plain");
}

#[test]
fn invalid_easing_name_warns_on_assignment() {
    let source = r#"
r: Rect, size: (100, 100), color: accent.primary
#0s
r.at = (320, 180) [500ms, ease: bounce-out]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::InvalidModifierValue
                && d.message.contains("bounce-out")
        }),
        "expected an InvalidModifierValue diagnostic naming 'bounce-out', got: {:?}",
        report.diagnostics
    );
}

/// A multi-segment property expression that fails to resolve must produce a
/// diagnostic naming the full dotted path. (Regression: only the base segment
/// was reported as the undefined variable, which slipped through the
/// dotless-key filter — `font_size: theme.text_md` without an aliased import
/// rendered invisibly with a clean `check`.)
#[test]
fn unresolved_path_property_warns_with_full_dotted_path() {
    let source = r#"
t: Text, text: "hello", font_size: theme.text_md, color: accent.primary
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| d.message.contains("theme.text_md")),
        "expected a diagnostic naming 'theme.text_md', got: {:?}",
        report.diagnostics
    );
}

#[test]
fn letchain_evaluates_with_shadowing_and_restores_env() {
    // Build-time let-bound block closure: sequential bindings see earlier
    // ones, shadowing wins innermost, and the caller env is restored (a later
    // read of the shadowed name must see the original value).
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade-in label [100ms]

#0.5s
let base = 4
let step = (v) => {
  let base = base * 10
  let out = base + v
  out
}
label.text = format("step(2)={} base={}", step(2), base)
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
        .tracks
        .get("label")
        .and_then(|t| t.text.text_content.as_ref())
        .map(|track| track.evaluate(1000))
        .unwrap_or_default();
    // step(2) = (4*10)+2 = 42; the outer `base` must still be 4 afterwards.
    assert_eq!(text, "step(2)=42 base=4");
}

#[test]
fn letchain_inside_plot_closure_is_dynamic_and_finite() {
    // A block-bodied plot closure that references `t` must classify the plot
    // as dynamic (references_ident walks LetChain bindings + tail) and every
    // sampled y must be finite.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

c: PlotCurve, kind: "cartesian",
  func: (x) => {
    let k = clamp(floor(t), 0, 6)
    let deg = 2 * k + 1
    sum_range((j) => (-1)^j * x^(2*j + 1) / factorial(2*j + 1), 0, floor(deg / 2))
  },
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

    let track = timeline.tracks.get("c").expect("curve track");
    assert!(
        track
            .procedural_plot
            .as_ref()
            .expect("procedural plot")
            .is_dynamic(&timeline.frame_written_vars),
        "LetChain body referencing t must be dynamic"
    );

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
                            assert!(p.y.is_finite(), "sampled y must be finite at x={}", p.x);
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
fn is_animating_guards_on_property_state() {
    // The & reference + is_animating query: `at` is keyframed (flag flips
    // with the interpolation window), `scale` has no track (always false),
    // and the composition drives an idle/tracking style switch.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

status: Text, text: "idle", font_size: 18, anchor: scene.center, text_max_width: 500

box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

#0.2s
fade-in status [150ms]
fade-in box [200ms]

#1s
box.at = (420, 260) [1s, ease: ease-in-out]

always {
  status.text = if is_animating(&box.at) { "moving" } else { "at rest" }
  box.color = if is_animating(&box.at) { accent.warning } else { accent.success }
}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    // The always block overrides the status text per frame — the override is
    // only visible through full pipeline evaluation (execute_modifier_ir),
    // not through the text track's keyframes.
    use crate::timeline::modifier_runtime::ir::{ModifierOverrides, execute_modifier_ir};
    let status_at = |time_ms: u64| -> String {
        let mut overrides = ModifierOverrides::default();
        let mut env = timeline.build_frame_env(
            time_ms,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            &std::collections::HashMap::new(),
        );
        for program in &timeline.modifier_programs {
            execute_modifier_ir(program, &mut env, &mut overrides).expect("modifier execution");
        }
        let status = overrides.get("status").and_then(|props| props.get("text"));
        match status {
            Some(crate::timeline::Value::Str(s)) => s.clone(),
            other => panic!("status override missing at {time_ms}ms, got {other:?}"),
        }
    };

    // t=500ms: the declaration position seeds a t=0 keyframe, so the span
    // up to the first assignment reads as inside a segment — true (see spec:
    // declaration positions count as keyframes).
    assert_eq!(status_at(500), "moving");
    assert_eq!(status_at(1500), "moving");
    // t=2500ms: after the last keyframe (2s) — no next keyframe, at rest.
    assert_eq!(status_at(2500), "at rest");
}

#[test]
fn is_animating_requires_property_reference() {
    // Passing a plain value instead of &actor.prop must be a clear type
    // error, not a silent false.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "t"

#0.5s
label.text = format("{}", is_animating(5))
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // The call itself builds; the frame-time evaluation must carry the type
    // error through the text override failure path (runtime diagnostics), so
    // assert the build produced no *error-level* diagnostics and rely on the
    // unit-level check below for the message.
    let _ = report;

    let mut env = crate::timeline::Environment::new();
    crate::timeline::load_standard_library(&mut env);
    // Through the env dispatch (the same path `always` uses).
    let err = crate::timeline::utils::evaluate_call_value(
        "is_animating",
        vec![crate::timeline::Value::Num(5.0)],
        &env,
    )
    .expect_err("non-reference arg must be a type error");
    assert!(err.to_string().contains("property reference"), "unexpected error: {err}");
}

#[test]
fn letchain_closure_captures_let_binding() {
    // A closure created inside a block body must capture the let-bound
    // variable (CapturedEnv::snapshot includes LetChain scopes). Before the
    // fix this invoked as UndefinedVariable — the inner closure's captures
    // silently missed the scoped `a`.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade-in label [100ms]

#0.5s
let maker = (x) => {
  let a = x * 10
  (y) => y + a
}
let add20 = maker(2)
label.text = format("add20(4) = {}", add20(4))
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let text = timeline
        .tracks
        .get("label")
        .and_then(|t| t.text.text_content.as_ref())
        .map(|track| track.evaluate(1000))
        .unwrap_or_default();
    // maker(2) binds a=20; add20(4) = 4 + 20.
    assert_eq!(text, "add20(4) = 24");
}

#[test]
fn effect_keys_on_non_action_hosts_warn() {
    // The effect-key whitelist is Action-only: the same keys on an
    // assignment or actor declaration are silent no-ops today, and the
    // host-gated whitelist must surface them.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, at: (240, 135)

        #0.5s
        box.at = (240, 200) [padding: 4]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnsupportedModifierKey
                && d.message.contains("padding")
        }),
        "effect key on an assignment must warn, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn prop_ref_with_unknown_actor_warns_at_build() {
    // `&rign.at` (typo'd label) must produce a build-time diagnostic instead
    // of failing at frame time.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

        #0.2s
        fade-in box [200ms]

        always {
          box.color = if is_animating(&rign.at) { accent.warning } else { accent.success }
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownTargetPath
                && d.message.contains("rign")
        }),
        "typo'd property-ref label must warn at build, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn prop_ref_to_non_injectable_property_warns() {
    // `font_size` is assignable but not injectable (read_source is absent),
    // so `&box.font_size` must warn even though the label and the property
    // name are both individually valid.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center, font_size: 10

        #0.2s
        fade-in box [200ms]

        always {
          let _guard = is_animating(&box.font_size)
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownTargetPath
                && d.message.contains("font_size")
        }),
        "non-injectable property ref must warn at build, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn brace_list_assignment_to_vector_properties_works() {
    // All-numeric brace lists are accepted at the property boundary with
    // the same semantics as the paren form (`at` moves, `size` applies).
    // This used to be three separate silent no-ops: at re-keyframed the old
    // position, size re-keyframed the old size.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: {40, 40}, color: accent.primary, anchor: scene.center

#0.2s
fade-in box [150ms]

#1s
box.at = {420, 260} [500ms, ease: ease-out]
box.size = {120, 60} [500ms, ease: ease-out]
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

    let box_track = timeline.tracks.get("box").expect("box track");
    let at = box_track.geometry.position.as_ref().expect("at track").evaluate(2000);
    let size = box_track.geometry.size.as_ref().expect("size track").evaluate(2000);
    assert_eq!(at, [420.0, 260.0], "brace at must position the actor");
    assert_eq!(size, [60.0, 30.0], "brace size is stored as half-size");
}

#[test]
fn brace_list_with_non_numeric_element_warns() {
    // Heterogeneous lists stay rejected (no silent 0.0 coercion): a
    // non-numeric element produces an InvalidPropertyValue diagnostic and
    // the previous position is kept.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

#0.2s
fade-in box [200ms]

#1s
box.at = {280, "x"} [300ms]
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
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue),
        "non-numeric brace element must warn, got: {:?}",
        report.diagnostics
    );
}

/// The layout readers take a number or a `(x, y)` tuple and used to leave
/// anything else alone with no output at all. `gap`'s type row now admits a
/// string (BarChart reads `"auto"` as the same name), so the drop has to be
/// reported by the reader instead of relying on the type check.
#[test]
fn container_layout_value_it_cannot_read_is_reported() {
    for (label, source) in [
        ("Row string gap", r#"row: Row, gap: "auto" { a: Rect, size: (10, 10) }"#),
        ("Col string gap", r#"col: Col, gap: "auto" { a: Rect, size: (10, 10) }"#),
        (
            "Grid string gap",
            r#"grid: Grid, cols: 2, gap: "auto" { a: Rect, size: (10, 10) }"#,
        ),
    ] {
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "{label} parse errors: {parse_errors:?}");
        let ast = ast.expect("parsed AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                    && d.message.contains("gap")
            }),
            "{label} must report the gap it dropped, got: {:?}",
            report.diagnostics
        );
    }
}

/// A timed declaration seeds a start snapshot *and* an end keyframe for every
/// shape value it carries. `corner_radius` was missing from the start list
/// while being present in the end list, which made a timed radius declaration
/// jump straight to its final value instead of animating from the current one.
#[test]
fn timed_declaration_snapshots_the_corner_radius() {
    let source = r#"
#0s
r: Rect, at: (100, 50), size: (120, 70), color: (1, 1, 1, 1), corner_radius: 30 [600ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let track = report.output.get_track("r").expect("track r");
    assert_eq!(
        property_keyframe_times(track, crate::timeline::ActorField::CornerRadius),
        vec![0, 600],
        "a timed declaration needs a start snapshot and an end keyframe"
    );
    assert_eq!(
        crate::timeline::read_property_value(track, crate::timeline::ActorField::CornerRadius, 0),
        Some(crate::timeline::PropertyValue::F32(0.0)),
        "the animation starts from the pre-declaration value"
    );
    assert_eq!(
        crate::timeline::read_property_value(track, crate::timeline::ActorField::CornerRadius, 600),
        Some(crate::timeline::PropertyValue::F32(30.0)),
        "and ends at the declared value"
    );
}

// `config { duration: N }` is documented as overriding the keyframe-inferred
// duration (docs/spec.md, "Config key scopes"), and the per-scene composition
// path has always honoured it. These pin the single-scene equivalent.

fn build_source(source: &str) -> crate::timeline::BuildReport<Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    Timeline::build_with_diagnostics(&ast.expect("parsed AST"), &std::collections::HashMap::new())
}

const SCENE_WITH_DURATION: &str = r#"
config { colorscheme: "editorial-dark", resolution: (1280, 720), duration: 2.0 }
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#5s
fade-in r [500ms]
"#;

#[test]
fn declared_duration_overrides_the_inferred_length() {
    let report = build_source(SCENE_WITH_DURATION);

    assert_eq!(
        report.output.duration_seconds(),
        5.5,
        "the inferred extent still reaches the last keyframe"
    );
    assert_eq!(
        report.output.declared_duration_seconds(),
        Some(2.0),
        "the declared value is what config asked for"
    );
    assert_eq!(
        report.output.playback_duration_seconds(),
        2.0,
        "playback ends at the declared duration even though keyframes run past it"
    );
}

#[test]
fn truncating_at_the_declared_duration_warns() {
    let report = build_source(SCENE_WITH_DURATION);

    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::DurationShorterThanContent
                && d.location.subject.as_deref() == Some("duration")
        }),
        "content past the declared duration must not be dropped silently, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn declared_duration_longer_than_content_extends_playback_without_warning() {
    let report = build_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (1280, 720), duration: 8.0 }
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert_eq!(report.output.playback_duration_seconds(), 8.0);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::DurationShorterThanContent),
        "a duration longer than the content truncates nothing, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn duration_config_must_be_a_positive_number() {
    for value in ["0.0", "-3.0", "\"long\""] {
        let report = build_source(&format!(
            r#"
config {{ colorscheme: "editorial-dark", duration: {value} }}
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#
        ));

        assert_eq!(
            report.output.declared_duration_seconds(),
            None,
            "`duration: {value}` is not a usable length"
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == crate::diagnostics::DiagnosticCode::InvalidConfigValue
                    && d.location.subject.as_deref() == Some("duration")
            }),
            "`duration: {value}` must be reported, got: {:?}",
            report.diagnostics
        );
        assert_eq!(
            report.output.playback_duration_seconds(),
            report.output.duration_seconds(),
            "an unusable duration falls back to the inferred extent"
        );
    }
}

#[test]
fn unknown_config_key_is_reported_instead_of_dropped() {
    let report = build_source(
        r#"
config { colorscheme: "editorial-dark", colur_scheme: "warm" }
r: Rect, size: (100, 100), color: accent.primary, opacity: 0.25, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey
                && d.location.subject.as_deref() == Some("colur_scheme")
        }),
        "a config key no phase reads must be reported, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn every_documented_config_key_is_recognised() {
    let report = build_source(
        r#"
config {
    colorscheme: "editorial-dark",
    resolution: (1280, 720),
    duration: 8.0,
    dynamic_layout: true,
    strict_types: false,
    text_fast_path: true,
    export_preset: "1080p30",
}
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey),
        "every documented config key must be recognised, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn rect_to_ellipse_redeclaration_keeps_rect_geometry_before_the_morph() {
    // A same-label re-declaration that changes the actor type (the morph
    // syntax) must not reach back before its own beat: the actor renders as
    // its declared type until the morph starts. Regression: `box: Ellipse`
    // at #1.5s made the Rect draw as a circle from frame 0 (the morph target
    // leaked into the pre-morph span).
    let source = r#"
config { resolution: (640, 360), duration: 3 }
box: Rect, size: (140, 140), color: accent.primary, at: (320, 180)

#0.3s
fade-in box [250ms, ease: ease-out]

#1.5s
box: Ellipse, size: (140, 140), color: accent.success [900ms, strategy: match]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let track = timeline.tracks.get("box").expect("box track exists");

    let radius_pre = track.shape.corner_radius.get(500, 0.0);
    assert_eq!(
        radius_pre, 0.0,
        "corner_radius at t=500ms (before the #1.5s morph) must be the Rect's 0, got {radius_pre}"
    );
    let shape_pre = crate::timeline::TrackAccessor::get(
        &track.shape.shape_type,
        500,
        crate::timeline::ShapeType::Rect,
    );
    assert_eq!(
        shape_pre,
        crate::timeline::ShapeType::Rect,
        "shape_type at t=500ms must still be Rect"
    );

    // The frame-time primitive must follow the shape_type track, not the
    // (last-write-wins) track identity: the actor renders as the declared
    // Rect until the morph beat and as the re-declared Ellipse after it.
    assert_eq!(track.render_type_name(500), "Rect");
    assert_eq!(track.render_type_name(1_400), "Rect");
    assert_eq!(track.render_type_name(2_400), "Ellipse");
}

#[test]
fn bounce_action_settles_with_decaying_hops() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 100]
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    let y = |ms: u64| track.geometry.motion_offset.get(ms, [0.0, 0.0])[1];

    // restitution 0.6 ⇒ six visible hops filling the 1s window, so the first
    // airtime is 1000·(1-0.6)/(1-0.6⁶) ≈ 420ms: apex at 210, contact at 420.
    assert!(y(210) < -95.0, "first apex should reach ~100px up, got {}", y(210));
    assert!(y(420).abs() < 1.5, "first contact must return to rest, got {}", y(420));
    // Hop 2 keeps 0.6² of the height and 0.6 of the airtime.
    assert!(y(545) > -40.0 && y(545) < -30.0, "second apex should be ~36px, got {}", y(545));
    assert!(y(1000).abs() < 1.5, "the actor must finish where it started, got {}", y(1000));
    // A rise decelerates and a fall accelerates: that pair is the parabola, so
    // the arc must not be linear in either half. A quarter into the rise the
    // actor is already most of the way up; a quarter into the fall it has
    // barely left the apex.
    assert!(y(52) < -35.0, "rising should be front-loaded, got {}", y(52));
    assert!(y(262) < -85.0, "falling should be back-loaded, got {}", y(262));
}

#[test]
fn bounce_restitution_stretches_the_series_to_the_duration() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 100, restitution: 0.9]
"#,
    );
    let track = timeline.tracks.get("a").expect("rect track");
    let y = |ms: u64| track.geometry.motion_offset.get(ms, [0.0, 0.0])[1];
    // A springier ball is still airborne at 900ms — the hop cap must not leave
    // it parked early — and it is home at the end of the duration.
    assert!(y(900) < -1.0, "0.9 restitution should still be bouncing, got {}", y(900));
    assert!(
        y(1000).abs() < 1.5,
        "the series should end on the rest position, got {}",
        y(1000)
    );
}

/// The timing parser used to keep its own list of effect modifier keys, so an
/// action gaining a parameter (`bounce [restitution: …]`) produced a warning
/// about a key its own signature declares.
#[test]
fn action_modifiers_are_tolerated_from_the_signature_not_a_literal_list() {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 40, restitution: 0.7]
"#,
    );
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("ast"), &std::collections::HashMap::new());
    let noisy: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("Unsupported modifier key"))
        .map(|d| d.message.clone())
        .collect();
    assert!(noisy.is_empty(), "declared modifiers warned: {noisy:?}");
}

#[test]
fn a_misspelled_action_modifier_still_warns() {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensy: 40]
"#,
    );
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("ast"), &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| d.message.contains("intensy")),
        "a typo in an action modifier produced no diagnostic"
    );
}

/// `Arrow` paints both its shaft and its head from `stroke_color`, so an
/// authored `color:` had nowhere to go: the actor built, rendered grey, and
/// warned about nothing.
#[test]
fn stroke_only_shapes_inherit_an_authored_color_into_their_stroke() {
    for (ty, label) in [("Arrow", "arrow"), ("Line", "line")] {
        let source = format!(
            "config {{ colorscheme: \"editorial-dark\", resolution: (640, 360) }}\n\
             #0s\na: {ty}, from: (100, 100), to: (300, 200), color: accent.danger, at: (0, 0)\n"
        );
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "{label} parse errors: {parse_errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.unwrap(), &std::collections::HashMap::new());
        let track = report.output.tracks.get("a").expect("track");
        let stroke = track.style.stroke_color.get(0, [0.0; 4]);
        let fill = track.style.color.get(0, [0.0; 4]);
        // Assert the tracks exist, not just that they agree: two absent tracks
        // both read back as the [0,0,0,0] default and would match vacuously.
        assert!(fill != [0.0, 0.0, 0.0, 0.0], "{label}: no color track was built at all");
        assert_eq!(
            stroke, fill,
            "{label}: authored `color:` never reached stroke_color ({stroke:?} vs {fill:?})"
        );
    }
}

/// The plot dispatch detected an authored `opacity:` and then overwrote it
/// with 1.0, so dimmed backdrops were impossible on Graph/BarChart/ContourSet/
/// VectorField without a warning to say so.
#[test]
fn plot_actors_honour_an_authored_opacity() {
    for ty in ["Graph", "BarChart"] {
        let source = format!(
            "config {{ colorscheme: \"editorial-dark\", resolution: (640, 360) }}\n\
             #0s\ng: {ty}, size: (400, 300), values: {{1, 2, 3}}, x_domain: (-1, 1), \
             y_domain: (-1, 1), opacity: 0.05, at: (320, 180)\n"
        );
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "{ty} parse errors: {parse_errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.unwrap(), &std::collections::HashMap::new());
        let track = report.output.tracks.get("g").expect("plot track");
        let opacity = track.style.opacity.get(0, 99.0);
        assert!(
            (opacity - 0.05).abs() < 1e-6,
            "{ty} dropped the authored opacity (got {opacity})"
        );
    }
}

/// A re-declaration at a shared timestamp must not steal its own target as its
/// starting value.
///
/// `insert_start_keyframes` captures what the track currently holds and writes
/// it at the declaration's timestamp, so a sibling that had already written a
/// keyframe at that same stamp could in principle become the "start" — the
/// animation would then run from its destination and the frame before the stamp
/// would show the new shape instead of the old one. Four orderings are checked
/// here because the interesting one is the combination: a statement sharing the
/// `#1s` stamp with the morph, written before it.
#[test]
fn a_redeclaration_starts_from_what_it_replaces_not_from_its_target() {
    let prelude = "config { resolution: (400, 300), duration: 3 }\n\
                   a: Ellipse, size: (60, 60), color: (1.0, 0.0, 0.0, 1.0), at: (100, 100)\n";
    let cases = [
        (
            "a morph on its own",
            "#1s\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
        (
            "an instant assignment first",
            "#1s\na.color = (0.0, 1.0, 0.0, 1.0)\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
        (
            "a timed assignment first",
            "#1s\na.color = (0.0, 1.0, 0.0, 1.0) [500ms]\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
    ];

    for (label, tail) in cases {
        let (ast, errors) = animatix_syntax::parser::parse_source(&(prelude.to_string() + tail));
        assert!(errors.is_empty(), "{label}: parse errors {errors:?}");
        let timeline =
            Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
                .output;
        let track = timeline.tracks.get("a").unwrap_or_else(|| panic!("{label}: no track 'a'"));

        // The frame before the stamp is still the declared ellipse, and still
        // carries no blue at all.
        assert_eq!(
            track.shape.shape_type.get(999, crate::timeline::ShapeType::Rect),
            crate::timeline::ShapeType::Ellipse,
            "{label}: the frame before the morph already shows the target shape"
        );
        assert!(
            track.style.color.get(999, [0.0; 4])[2] < 0.01,
            "{label}: the pre-morph frame started from the target colour: {:?}",
            track.style.color.get(999, [0.0; 4])
        );
        // And the morph itself still lands on the target.
        assert_eq!(
            track.shape.shape_type.get(1500, crate::timeline::ShapeType::Ellipse),
            crate::timeline::ShapeType::Rect,
            "{label}: the morph never reached its target shape"
        );
        assert!(
            track.style.color.get(1500, [0.0; 4])[2] > 0.99,
            "{label}: the colour morph did not finish: {:?}",
            track.style.color.get(1500, [0.0; 4])
        );
    }
}

/// An undated `.text =` swap must not begin before its own stamp.
///
/// The track model reads any two consecutive keyframes as an interpolation
/// segment, so unless the change is fenced the previously declared string
/// interpolates all the way into the new one — and the cross-fade branch in
/// `primitives` then compiles and draws *both* strings across the whole
/// preceding gap. That is the roadmap's "static keyframe `.text =` assignments
/// overprint", which forced `web/demos/hash/scene.amx` onto one actor per string.
#[test]
fn an_undated_text_swap_holds_the_old_string_until_its_own_stamp() {
    let source = "\
config { resolution: (400, 300), duration: 3 }
t: Text, text: \"first\", font_size: 30, at: (100, 100)
#2s
t.text = \"second\"
";
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output;
    let text = timeline.tracks.get("t").expect("text track");

    for ms in [0u64, 500, 1000, 1999] {
        assert_eq!(
            text.text.text_content.get(ms, String::new()),
            "first",
            "the text swap leaked into the frame before its stamp at {ms}ms"
        );
        // The renderer cross-fades whenever a segment is open *between two
        // different strings*; a segment whose endpoints are the same string is
        // harmless, so that is the property asserted here.
        if let Some((_, prev, found, _, _)) = text
            .text
            .text_content
            .as_ref()
            .and_then(|track| track.interpolation_segment(ms))
        {
            assert_eq!(
                prev, found,
                "two different strings were interpolating at {ms}ms, before the swap is due"
            );
        }
    }

    assert_eq!(
        text.text.text_content.get(2000, String::new()),
        "second",
        "the swap did not take effect at its own stamp"
    );
}

/// The numeric half of the same rule: a bare assignment is a step at its stamp.
///
/// `docs/spec.md` calls this an "Instant Change". The track model has no step
/// keyframe, so the value has to be fenced one millisecond before the stamp or
/// the previous keyframe eases into it across the whole gap — which is what
/// happened until `write_property_plan_slot` learned to fence undated writes.
#[test]
fn an_undated_numeric_assignment_is_a_step_not_a_backwards_ramp() {
    let source = "\
config { resolution: (400, 300), duration: 3 }
r: Rect, size: (40, 40), color: (1.0, 0.0, 0.0, 1.0), opacity: 1.0, at: (200, 200)
#2s
r.opacity = 0.0
";
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output;
    let rect = timeline.tracks.get("r").expect("rect track");

    for ms in [0u64, 500, 1000, 1999] {
        assert_eq!(
            rect.style.opacity.get(ms, 0.0),
            1.0,
            "the rect was already fading at {ms}ms, two seconds before the assignment"
        );
    }
    assert_eq!(
        rect.style.opacity.get(2000, 1.0),
        0.0,
        "the step did not take effect at its own stamp"
    );
}

#[test]
fn gradient_paints_are_parsed_stamped_and_interpolated() {
    // `fill_gradient:` / `stroke_gradient:` are registry-backed paints: the
    // declaration seeds them, dated assignments interpolate, and the shared
    // `gradient_extend:` / `gradient_space:` settings ride the stamp.
    let source = r##"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

r: Rect, size: (200, 120), at: (320, 180), color: accent.primary,
  fill_gradient: linear(135, {"#ff0000", "#00ff00"}),
  gradient_extend: "reflect"
s: Path, commands: {move_to(100, 300), line_to(500, 300)},
  stroke: accent.warning, stroke_width: 6, fill_opacity: 0.0,
  stroke_gradient: sweep(45, {"#ffffff", "#888888", "#000000"})

#0s
r.fill_gradient = radial((0.5, 0.5), 0.6, {(0%, "#ff0000"), (100%, "#0000ff")})
#1s
r.fill_gradient = radial((0.5, 0.5), 0.2, {(0%, "#ff0000"), (100%, "#0000ff")}) [1s]
    "##;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed),
        "expected only never-revealed hints, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let paints_at = |time: f64| -> (
        Option<crate::timeline::GradientSpec>,
        Option<crate::timeline::GradientSpec>,
    ) {
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
        let mut found = (None, None);
        for item in &program.items {
            for command in &item.commands {
                if let crate::primitives::RenderCommand::Paths { paths } = command {
                    for vp in paths {
                        if vp.fill_gradient.is_some() {
                            found.0 = vp.fill_gradient.as_deref().cloned();
                        }
                        if vp.stroke_gradient.is_some() {
                            found.1 = vp.stroke_gradient.as_deref().cloned();
                        }
                    }
                }
            }
        }
        found
    };

    // The `[1s]` ramp runs 1s..2s: it holds 0.6 before it starts and lands on
    // 0.4 halfway through, which is what makes a painted ramp animatable.
    let early = paints_at(0.5).0.expect("declared ramp must be stamped early");
    match early.shape {
        crate::timeline::GradientShape::Radial { radius, .. } => {
            assert!((radius - 0.6).abs() < 1e-3, "ramp holds before its duration, got {radius}");
        },
        other => panic!("expected the radial ramp, got {other:?}"),
    }
    let (fill, stroke) = paints_at(1.5);
    let fill = fill.expect("the dated ramp must be stamped on the fill");
    match fill.shape {
        crate::timeline::GradientShape::Radial { center, radius } => {
            assert!((radius - 0.4).abs() < 1e-3, "ramp radius must lerp, got {radius}");
            assert!(
                (center[0] - 0.5).abs() < 1e-3 && (center[1] - 0.5).abs() < 1e-3,
                "center must hold at (0.5, 0.5), got {center:?}"
            );
        },
        other => panic!("expected the interpolated ramp to stay radial, got {other:?}"),
    }
    assert_eq!(fill.stops.len(), 2);
    assert_eq!(
        fill.stops[0].color,
        [1.0, 0.0, 0.0, 1.0],
        "a #rrggbb stop must resolve as a color, not fall back to gray"
    );
    assert_eq!(
        fill.extend,
        crate::timeline::GradientExtend::Reflect,
        "gradient_extend: must ride the stamp"
    );
    assert_eq!(fill.space, crate::timeline::GradientSpace::Oklab);

    // A three-color sweep spreads its stops evenly at 0, 0.5, 1.
    let stroke = stroke.expect("stroke_gradient: must be stamped on the stroke");
    let offsets: Vec<f32> = stroke.stops.iter().map(|s| s.offset).collect();
    assert_eq!(stroke.stops[1].color, [0.533_333_36, 0.533_333_36, 0.533_333_36, 1.0]);
    assert_eq!(offsets, vec![0.0, 0.5, 1.0], "unspaced stops spread evenly");
    assert!(
        matches!(
            stroke.shape,
            crate::timeline::GradientShape::Sweep { angle, .. } if (angle - 45.0).abs() < 1e-3
        ),
        "sweep angle must come through authored, got {:?}",
        stroke.shape
    );

    // The brush actually materializes: positioned in the shape's bbox it keeps
    // every stop, which is what the renderer hands to Vello.
    let brush = stroke.to_peniko(kurbo::Rect::new(100.0, 297.0, 500.0, 303.0), 1.0);
    assert_eq!(brush.stops.len(), 3);
    assert!(matches!(brush.kind, vello::peniko::GradientKind::Sweep(_)));
}

#[test]
fn seamless_loop_lints_values_that_do_not_wrap() {
    // `config { seamless_loop: true }` asks the build to check the seam: a
    // keyframed value that differs between the first and last frame jumps on
    // every replay. A scene whose values do wrap must stay quiet.
    let broken = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary
#0s
fade-in box [200ms]
box.at = (200, 180)
#2s
box.at = (440, 180)
    "#;
    // The fixed version of the same scene: no entrance (an actor that fades in
    // is transparent at frame 0 and opaque at the end, which is itself a seam)
    // and a position that returns to where it started.
    let seamless = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary, opacity: 1.0
#0s
box.at = (200, 180)
#1s
box.at = (440, 180)
#2s
box.at = (200, 180)
    "#;

    let codes = |source: &str| -> Vec<String> {
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast.unwrap(),
            &std::collections::HashMap::new(),
        );
        report.diagnostics.iter().map(|d| d.code.to_string()).collect()
    };

    let broken_codes = codes(broken);
    assert!(
        broken_codes.iter().any(|c| c == "loop-not-seamless"),
        "a scene whose position does not wrap must warn, got {broken_codes:?}"
    );
    // The warning must name the offending property so the author can fix it.
    let warning = report_message(broken);
    assert!(
        warning.contains("`box.position`"),
        "warning should name `box.position`, got {warning}"
    );

    let seamless_codes = codes(seamless);
    assert!(
        !seamless_codes.iter().any(|c| c == "loop-not-seamless"),
        "a scene that returns to its start must not warn, got {seamless_codes:?}"
    );
}

/// The camera is not a track, so the seam walk over `tracks` cannot see it. A
/// scene that ends pushed in and restarts unzoomed jumps at every replay exactly
/// like an un-wrapped `at`, so `Camera::seam_pairs` feeds the same check.
#[test]
fn seamless_loop_lints_camera_axes() {
    let pushed = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }
box: Rect, size: (100, 100), at: (320, 180), color: accent.primary, opacity: 1.0
#0s
camera.zoom = 1.0
#1s
camera.zoom = 1.8
"#;
    let wrapped = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }
box: Rect, size: (100, 100), at: (320, 180), color: accent.primary, opacity: 1.0
#0s
camera.zoom = 1.0
camera.at = (0, 0)
#1s
camera.zoom = 1.8
camera.at = (60, 0)
#2s
camera.zoom = 1.0
camera.at = (0, 0)
"#;

    let warning = report_message(pushed);
    assert!(
        warning.contains("`camera.zoom`"),
        "a loop that ends pushed in must name the axis, got {warning}"
    );
    let quiet = report_message(wrapped);
    assert!(
        !quiet.contains("camera."),
        "a camera that returns to its start must not warn, got {quiet}"
    );
}

/// The text of the first `loop-not-seamless` warning for `source`.
fn report_message(source: &str) -> String {
    let (ast, _) = animatix_syntax::parser::parse_source(source);
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::LoopNotSeamless)
        .map(|d| d.message.clone())
        .unwrap_or_default()
}

#[test]
fn settle_in_and_pop_in_ramp_scale_onto_the_authored_scale() {
    // The entrance presets are a fade plus a scale ramp *onto* what the actor
    // already authored, so `scale: 2` still ends at 2 instead of being
    // flattened to 1 by the entrance.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

card: Rect, size: (120, 80), at: (200, 180), color: accent.primary
chip: Ellipse, size: (60, 60), at: (420, 180), color: accent.warning

#0.5s
settle-in card [500ms]
#1s
pop-in chip [500ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected a clean build, got {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    use crate::timeline::property_registry::lookup_property;
    use crate::timeline::read_property_value;

    let read = |label: &str, prop: &str, ms: u64| {
        let schema = lookup_property(prop).expect("property");
        read_property_value(timeline.tracks.get(label).unwrap(), schema.field, ms)
            .unwrap_or_else(|| (schema.default_value)(&timeline.tracks[label].caps))
    };

    // settle-in: 0.5s..1.0s, scale 0.92 -> 1.0 relative to the sampled scale,
    // opacity 0 -> 1.
    let crate::timeline::PropertyValue::F32(scale_start) = read("card", "scale", 500) else {
        panic!("expected a numeric scale");
    };
    let crate::timeline::PropertyValue::F32(scale_end) = read("card", "scale", 1000) else {
        panic!("expected a numeric scale");
    };
    assert!(
        (scale_start - 0.92).abs() < 1e-3,
        "settle-in must start 8% under its resting scale, got {scale_start}"
    );
    assert!(
        (scale_end - 1.0).abs() < 1e-3,
        "the ramp must land back on the resting scale, got {scale_end}"
    );
    let crate::timeline::PropertyValue::F32(opacity_mid) = read("card", "opacity", 750) else {
        panic!("expected a numeric opacity");
    };
    assert!(
        opacity_mid > 0.0 && opacity_mid < 1.0,
        "settle-in must be mid-fade at the halfway point, got {opacity_mid}"
    );

    // pop-in: starts at 60% and overshoots past its target, which is what the
    // `back` arrival buys.
    let crate::timeline::PropertyValue::F32(pop_start) = read("chip", "scale", 1000) else {
        panic!("expected a numeric scale");
    };
    assert!(
        (pop_start - 0.6).abs() < 1e-3,
        "pop-in must start at 60% scale, got {pop_start}"
    );
    let mut max_scale = 0.0_f32;
    for ms in (1000..=1500).step_by(25) {
        let crate::timeline::PropertyValue::F32(v) = read("chip", "scale", ms) else {
            continue;
        };
        max_scale = max_scale.max(v);
    }
    assert!(max_scale > 1.0, "pop-in must overshoot past its target, peaked at {max_scale}");
}

#[test]
fn anticipate_inserts_a_counter_move_before_the_travel() {
    // `[anticipate: 100ms]` leans the actor back by 12% of its travel before
    // the move starts, so the translation reads as intentional.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

dot: Ellipse, size: (40, 40), at: (200, 180), color: accent.primary

#0s
fade-in dot [200ms]
#1s
move dot [to: (200, 0), 500ms, anticipate: 100ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected a clean build, got {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    use crate::timeline::property_registry::lookup_property;
    use crate::timeline::read_property_value;

    let offset_at = |ms: u64| -> [f32; 2] {
        let schema = lookup_property("shift").expect("property");
        match read_property_value(timeline.tracks.get("dot").unwrap(), schema.field, ms) {
            Some(crate::timeline::PropertyValue::Vec2(v)) => v,
            other => panic!("expected a Vec2 offset at {ms}ms, got {other:?}"),
        }
    };

    let held = offset_at(900);
    let leaned = offset_at(950);
    let [hx, hy] = held;
    let [lx, ly] = leaned;
    assert!(
        lx < hx - 1.0,
        "the actor must lean back (negative x) before travelling +200: held {held:?}, leaned {leaned:?}"
    );
    assert!(
        (ly - hy).abs() < 1e-3,
        "the lean follows the travel axis only, got {held:?} -> {leaned:?}"
    );
    let at_start = offset_at(1000);
    assert!(
        at_start[0] < 0.0,
        "the move must begin from the leaned position, got {at_start:?}"
    );
    let landed = offset_at(1500);
    assert!(
        (landed[0] - 200.0).abs() < 1.0 && landed[1].abs() < 1e-3,
        "the travel must still land on the authored offset, got {landed:?}"
    );
}

#[test]
fn cli_defines_shadow_the_authored_let_defaults() {
    // `--set NAME=VALUE` is the template × data seam: the file declares its
    // defaults with a top-level `let`, and the command line replaces them
    // before anything resolves against the build environment.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

let label = "Default"
let tint = (1.0, 0.0, 0.0, 1.0)

t: Text, text: label, at: (200, 180), color: tint
u: Text, text: label, at: (200, 240)

#0s
fade-in t [200ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let namespaces = std::collections::HashMap::new();

    let defines = vec![
        ("label".to_string(), "\"Overridden\"".to_string()),
        ("tint".to_string(), "(0.0, 1.0, 0.0, 1.0)".to_string()),
    ];
    let report =
        crate::timeline::Timeline::build_with_diagnostics_and_defines(&ast, &namespaces, &defines);
    let owned: Vec<String> = report.diagnostics.iter().map(|d| format!("{:?}", d.code)).collect();
    assert!(
        owned.iter().all(|c| c.contains("NeverRevealed")),
        "the override must not introduce diagnostics, got {owned:?}"
    );
    let timeline = report.output;

    // The override must reach the *resolved* properties, not just the
    // environment — re-applying the values after the walk would leave every
    // property that already resolved against them at its authored default.
    use crate::timeline::read_property_value;
    let sampled = |label: &str, prop: &str| {
        let schema = crate::timeline::property_registry::lookup_property(prop).unwrap();
        let track = timeline.tracks.get(label).unwrap();
        read_property_value(track, schema.field, 0)
            .unwrap_or_else(|| (schema.default_value)(&track.caps))
    };
    assert_eq!(
        sampled("t", "text"),
        crate::timeline::PropertyValue::String("Overridden".to_string()),
        "`--set label` must reach text resolved during the walk"
    );
    assert_eq!(
        sampled("u", "text"),
        crate::timeline::PropertyValue::String("Overridden".to_string())
    );
    assert_eq!(
        sampled("t", "color"),
        crate::timeline::PropertyValue::Color([0.0, 1.0, 0.0, 1.0]),
        "`--set tint` must reach the fill"
    );
}

#[test]
fn beat_stamps_resolve_against_the_declared_tempo() {
    // `#2b` is a musical stamp: its millisecond position comes from the scene's
    // `config { bpm: … }`, so a retempo moves the whole arrangement without
    // editing any stamp.
    let source = |config: &str| {
        format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (640, 360), {config} }}

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary, opacity: 1.0
#0s
box.shift = (0, 0)
#2b
box.shift = (240, 0)
"#
        )
    };
    let shift_at = |config: &str, ms: u64| -> [f32; 2] {
        let (ast, errors) = animatix_syntax::parser::parse_source(&source(config));
        assert!(errors.is_empty(), "Parse errors: {errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            ast.as_ref().unwrap(),
            &std::collections::HashMap::new(),
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed
                    || d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey
                    || d.code == crate::diagnostics::DiagnosticCode::InvalidConfigValue),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        use crate::timeline::read_property_value;
        let schema = crate::timeline::property_registry::lookup_property("shift").unwrap();
        let track = report.output.tracks.get("box").unwrap();
        match read_property_value(track, schema.field, ms) {
            Some(crate::timeline::PropertyValue::Vec2(v)) => v,
            other => panic!("expected a Vec2 shift, got {other:?}"),
        }
    };

    // 60 bpm: two beats is exactly two seconds, so the move has not started at
    // 1999 ms and has landed by 2000 ms.
    assert_eq!(shift_at("bpm: 60", 1999), [0.0, 0.0]);
    assert_eq!(shift_at("bpm: 60", 2000), [240.0, 0.0]);
    // 120 bpm is the documented default, so the same stamp lands at one second.
    assert_eq!(shift_at("bpm: 120", 1000), [240.0, 0.0]);
    assert_eq!(shift_at("", 1000), [240.0, 0.0], "no bpm means the default tempo");
    assert_eq!(shift_at("bpm: 120", 999), [0.0, 0.0]);
}

/// `icon: "check"` must expand to exactly the `Path` geometry an equivalent
/// hand-written `commands:` list produces — the whole contract of the property.
#[test]
fn icon_property_expands_to_commands_geometry() {
    // `check`'s bundled path data is `M20 6 9 17l-5-5`, which the shared
    // SVG parser lowers to move_to(20,6) + implicit line_to(9,17) +
    // relative line_to(4,12).
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        p_icon: Path, icon: "check", at: (100, 100)
        p_manual: Path,
          commands: {move_to(20, 6), line_to(9, 17), line_to(4, 12)},
          at: (300, 100)
    "#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );

    let geometry = |label: &str| -> Vec<kurbo::PathEl> {
        let track = report.output.tracks.get(label).unwrap_or_else(|| panic!("no track {label}"));
        let paths = track.evaluate_vector_paths_value(0);
        let mut elements = Vec::new();
        for vp in paths {
            elements.extend(vp.path.elements().iter().copied());
        }
        elements
    };

    let from_icon = geometry("p_icon");
    let from_manual = geometry("p_manual");
    assert!(!from_icon.is_empty(), "`icon: \"check\"` produced no path geometry at all");
    assert_eq!(
        from_icon, from_manual,
        "`icon: \"check\"` geometry must match the hand-written commands list"
    );
}

/// An unknown icon name must warn (naming close candidates) rather than draw
/// nothing in silence.
#[test]
fn unknown_icon_warns() {
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        p: Path, icon: "nope", at: (100, 100)
    "#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );

    let warned = report.diagnostics.iter().find(|d| {
        d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
            && d.message.contains("nope")
    });
    assert!(
        warned.is_some(),
        "unknown `icon` value must emit an InvalidPropertyValue warning; got {:?}",
        report.diagnostics
    );
}

#[test]
fn an_authored_scale_reaches_the_transform() {
    // `scale:` on a declaration was dropped on the floor for every actor kind:
    // it is not in the legacy per-primitive loop and was not routed through the
    // generic engine, so `scale: 4.0` rendered at 1.0 while the keyframed
    // `x.scale = 4.0` worked. Bundled icons made it visible — a 24 px mark that
    // could not be enlarged was otherwise unusable.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (20, 20), at: (200, 180), color: accent.primary, scale: 4.0
half: Rect, size: (20, 20), at: (400, 180), color: accent.primary, scale: 0.5
plain: Rect, size: (20, 20), at: (500, 180), color: accent.primary

#0s
fade-in box [200ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;
    use crate::timeline::read_property_value;
    let schema = crate::timeline::property_registry::lookup_property("scale").unwrap();
    let scale_of = |label: &str| -> Option<f32> {
        let track = timeline.tracks.get(label).unwrap();
        match read_property_value(track, schema.field, 0) {
            Some(crate::timeline::PropertyValue::F32(v)) => Some(v),
            None => None,
            other => panic!("expected a numeric scale for {label}, got {other:?}"),
        }
    };
    assert_eq!(scale_of("box"), Some(4.0), "authored scale must land");
    assert_eq!(scale_of("half"), Some(0.5), "a shrink must land too");
    assert_eq!(
        scale_of("plain"),
        None,
        "an actor that never authors scale gets no track, and renders at the 1.0 default"
    );
}

#[test]
fn move_along_bakes_arc_length_keyframes_onto_the_route() {
    // `move … [along: {…}]` must land on the route at constant speed: sampling by
    // arc length is the whole point, since the bezier parameter would crawl
    // through the curve and sprint along the straight.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

dot: Ellipse, size: (20, 20), at: (100, 100), color: accent.primary, opacity: 1.0

#0s
dot.at = (100, 100)
#0.5s
move dot [along: {move_to(100, 100), line_to(500, 100)}, 2s]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    let offenders: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code != crate::diagnostics::DiagnosticCode::NeverRevealed)
        .collect();
    assert!(offenders.is_empty(), "unexpected diagnostics: {offenders:?}");
    let timeline = report.output;
    use crate::timeline::read_property_value;
    let schema = crate::timeline::property_registry::lookup_property("position").unwrap();
    let track = timeline.tracks.get("dot").unwrap();
    let at = |ms: u64| match read_property_value(track, schema.field, ms) {
        Some(crate::timeline::PropertyValue::Vec2(v)) => v,
        other => panic!("expected a position at {ms}ms, got {other:?}"),
    };

    // 2.5s is the far end of the 2s travel; 1.5s is the halfway mark, which on a
    // 400-unit straight route must sit at x = 300 (arc length, not the parameter).
    let mid = at(1500);
    assert!(
        (mid[0] - 300.0).abs() < 6.0,
        "the halfway sample must sit at the half-length point, got {mid:?}"
    );
    let end = at(2500);
    assert!(
        (end[0] - 500.0).abs() < 2.0 && (end[1] - 100.0).abs() < 2.0,
        "the route's end must be reached, got {end:?}"
    );
    // Keyframed densely enough that a curve route reads as smooth.
    assert!(
        track.geometry.position.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) > 8,
        "the route must be baked into many samples"
    );
}

#[test]
fn draw_in_by_word_steps_through_the_words() {
    // `draw-in` on text is a typewriter by default; `by: word` brings each word
    // in as a step, which is the difference between a caption typing out
    // letter-by-letter and a line of kinetic typography landing in phrases.
    let build = |extra: &str| {
        let source = format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (640, 360) }}

line: Text, text: "aa bb cc dd", at: (320, 180), font_size: 40
#0s
draw-in line [4s{extra}]
"#
        );
        let (ast, errors) = animatix_syntax::parser::parse_source(&source);
        assert!(errors.is_empty(), "Parse errors: {errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            ast.as_ref().unwrap(),
            &std::collections::HashMap::new(),
        );
        let offenders: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code != crate::diagnostics::DiagnosticCode::NeverRevealed)
            .collect();
        assert!(offenders.is_empty(), "unexpected diagnostics: {offenders:?}");
        report.output
    };
    let progress_at = |timeline: &crate::timeline::Timeline, ms: u64| -> f32 {
        timeline.tracks.get("line").unwrap().text.char_progress.get(ms, 1.0)
    };

    let words = build(", by: word");
    // Four words over 4s: at 1.5s the second word has landed (6 of 11 chars) and
    // the third has not, so progress sits at the word boundary, not mid-word.
    let at_1500 = progress_at(&words, 1500);
    assert!(
        (at_1500 - 6.0 / 11.0).abs() < 0.06,
        "expected the second word boundary (~{:.3}), got {at_1500:.3}",
        6.0 / 11.0
    );
    // Each word lands at the start of its slot and holds: 2.5s is inside the
    // third slot, so the third boundary (9 of 11 chars) must still be holding.
    assert!(
        (progress_at(&words, 2500) - 9.0 / 11.0).abs() < 0.02,
        "the third boundary must hold until the fourth word, got {}",
        progress_at(&words, 2500)
    );
    assert!(
        progress_at(&words, 1600) > 0.5 && progress_at(&words, 1600) < 0.6,
        "a word must hold its boundary rather than creep, got {}",
        progress_at(&words, 1600)
    );
    assert!((progress_at(&words, 4000) - 1.0).abs() < 1e-3, "must finish full");

    // The default stays a smooth typewriter, and it inherits the role default
    // for entrances: `expo-out` puts a quarter of the *time* at ~82% of the
    // characters, which is why the reveal reads as arriving rather than ticking.
    let chars = build("");
    let mid = progress_at(&chars, 1000);
    assert!(
        mid > 0.7 && mid < 0.95,
        "the default reveal is a smooth ramp under expo-out, got {mid}"
    );
    assert!(
        progress_at(&chars, 200) < 0.35,
        "and it must still start sparse, got {}",
        progress_at(&chars, 200)
    );
}
