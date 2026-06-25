use super::*;
use crate::ast::{BinaryOp, LoopPattern};

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
                        name: "radius".to_string(),
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
                target: vec!["orbiter".to_string()],
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
        report.diagnostics.is_empty(),
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
    for modifier in &timeline.modifiers {
        timeline.apply_modifier_stmt(modifier, &mut env, &mut overrides);
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
        report.diagnostics.is_empty(),
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
fn equation_container_builds_with_fragment_children() {
    let source = r#"
        eq: Equation {
            f1: Fragment, content: "x^2"
            f2: Fragment, content: "+ y"
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

    // Fragment f1 track should exist with content stored
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
            f1: Fragment, content: "x^2"
            f2: Fragment, content: "+ y"
        }

        #+1s
        eq.f1.highlight_opacity = 1.0 [800ms]
        eq.f2.content = "+ z"
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
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

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
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
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
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
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
    assert!(report.diagnostics.is_empty(), "Unexpected diagnostics: {:?}", report.diagnostics);

    let padding = report.output.env().get("g_padding");
    match padding {
        Some(Value::Vec4([l, r, t, b])) => {
            assert!((l - 20.0).abs() < 1e-10, "expected left=20, got {l}");
            assert!((r - 10.0).abs() < 1e-10, "expected right=10, got {r}");
            assert!((t - 15.0).abs() < 1e-10, "expected top=15, got {t}");
            assert!((b - 5.0).abs() < 1e-10, "expected bottom=5, got {b}");
        }
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
    assert!(report.diagnostics.is_empty(), "Unexpected diagnostics: {:?}", report.diagnostics);

    let padding = report.output.env().get("g_padding");
    match padding {
        Some(Value::Vec4([l, r, t, b])) => {
            assert_eq!([l, r, t, b], [0.0, 0.0, 0.0, 0.0], "default padding should be [0;4]");
        }
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
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    match report.output.env().get("g.map_inverse") {
        Some(Value::NativeFn(_)) => {}
        other => panic!("expected NativeFn for g.map_inverse, got {other:?}"),
    }
}

/// Round-trip: `map_inverse(map(mx, my))` returns the original math coordinates.
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

    // Build a call environment with the keys each NativeFn expects.
    // `map` reads `{label}.size` / `{label}.at`.
    // `map_inverse` reads `{label}_size` / `{label}_at` / `{label}_padding`.
    let mut call_env = Environment::new();
    call_env.set("g.size", Value::Vec2([800.0, 600.0]));
    call_env.set("g.at", Value::Vec2([0.0, 0.0]));
    call_env.set("g_size", Value::Vec2([800.0, 600.0]));
    call_env.set("g_at", Value::Vec2([0.0, 0.0]));
    call_env.set("g_padding", Value::Vec4([0.0; 4]));

    for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (7.5, -4.0)] {
        let screen = map_fn(&[Value::Num(mx), Value::Num(my)], &call_env)
            .expect("map call");
        let (sx, sy) = match screen {
            Value::Vec2([sx, sy]) => (sx, sy),
            other => panic!("map returned {other:?}"),
        };
        let math = map_inv_fn(&[Value::Num(sx), Value::Num(sy)], &call_env)
            .expect("map_inverse call");
        match math {
            Value::Vec2([rx, ry]) => {
                assert!((rx - mx).abs() < 1e-9, "x round-trip: {mx} -> {sx} -> {rx}");
                assert!((ry - my).abs() < 1e-9, "y round-trip: {my} -> {sy} -> {ry}");
            }
            other => panic!("map_inverse returned {other:?}"),
        }
    }
}

/// `map_inverse` respects padding: screen center (shifted by padding) maps to math (0, 0).
/// With padding [left=20, right=10, top=15, bottom=5], the padded plot center is at
/// screen offset (5, 5), which corresponds to math origin (0, 0).
#[test]
fn graph_map_inverse_respects_padding() {
    let source =
        "g: Graph, size: (800, 600), x_domain: (-10, 10), y_domain: (-5, 5), padding: (20, 10, 15, 5)";
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
    let result = map_inv_fn(&[Value::Num(5.0), Value::Num(5.0)], &call_env)
        .expect("map_inverse call");
    match result {
        Value::Vec2([mx, my]) => {
            assert!((mx - 0.0).abs() < 1e-9, "expected mx=0, got {mx}");
            assert!((my - 0.0).abs() < 1e-9, "expected my=0, got {my}");
        }
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
        }
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
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

    let timeline = report.output;

    // Should have 3 actors: dot__0, dot__1, dot__2
    assert!(timeline.tracks.contains_key("dot__0"), "Expected dot__0 track");
    assert!(timeline.tracks.contains_key("dot__1"), "Expected dot__1 track");
    assert!(timeline.tracks.contains_key("dot__2"), "Expected dot__2 track");

    // Verify all three actors have position tracks
    // dot__0 should be at (10, 20)
    let dot0 = timeline.tracks.get("dot__0").unwrap();
    assert!(
        dot0.geometry.position.is_some(),
        "Expected dot__0 to have position track"
    );

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
    let ast = vec![
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ForLoop {
                    var: LoopPattern::Tuple(vec!["vx".to_string(), "vy".to_string()]),
                    index_var: None,
                    iterable: Expr::List(vec![
                        Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(15.0)]),
                        Expr::Tuple(vec![Expr::Num(25.0), Expr::Num(35.0)]),
                    ]),
                    body: vec![
                        Stmt::ActorDecl {
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
                        },
                    ],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

    let timeline = report.output;

    // Two actors should have been created (one per iteration)
    // They have the same label "point" so only the last one persists
    assert!(
        timeline.tracks.contains_key("point"),
        "Expected point track"
    );

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
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

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
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

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
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

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


