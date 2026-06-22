use super::*;

#[test]
fn test_keyframe_scoped_variables_create_tracks() {
    let ast = vec![
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(1.0),
                span: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(3.0),
            body: vec![Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(1.7),
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    assert!(
        timeline.variable_tracks.contains_key("freq"),
        "Expected variable track for 'freq'"
    );
    let freq_track = timeline.variable_tracks.get("freq").unwrap();
    assert_eq!(freq_track.keyframes.len(), 2, "Expected 2 keyframes for freq");

    // At t=0s, freq should be 1.0
    let val_0s = freq_track.evaluate(0);
    assert!(val_0s.is_some(), "Expected freq value at t=0s");
    assert_eq!(val_0s.unwrap(), Value::Num(1.0));

    // At t=1.5s, freq should still be 1.0 (piecewise-constant)
    let val_1_5s = freq_track.evaluate(1500);
    assert!(val_1_5s.is_some(), "Expected freq value at t=1.5s");
    assert_eq!(val_1_5s.unwrap(), Value::Num(1.0));

    // At t=3s, freq should be 1.7
    let val_3s = freq_track.evaluate(3000);
    assert!(val_3s.is_some(), "Expected freq value at t=3s");
    assert_eq!(val_3s.unwrap(), Value::Num(1.7));

    // At t=5s, freq should still be 1.7
    let val_5s = freq_track.evaluate(5000);
    assert!(val_5s.is_some(), "Expected freq value at t=5s");
    assert_eq!(val_5s.unwrap(), Value::Num(1.7));
}

#[test]
fn test_keyframe_scoped_variables_injected_into_frame_env() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "tracker".to_string(),
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
            Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(2.0),
                span: None,
            },
            Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec!["tracker".to_string()],
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
                                    vec![Expr::Binary(
                                        Box::new(Expr::Ident("freq".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("t".to_string())),
                                    )],
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
                                    vec![Expr::Binary(
                                        Box::new(Expr::Ident("freq".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("t".to_string())),
                                    )],
                                )),
                            )),
                        ),
                    ]),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
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

    // Evaluate at t=0s — with freq=2.0, cos(2*0)=1, sin(2*0)=0
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

    let tracker_at = overrides.get("tracker").and_then(|m| m.get("at"));
    assert!(
        tracker_at.is_some(),
        "Expected tracker.at override from always block using freq variable"
    );
    if let Some(Value::Vec2([x, y])) = tracker_at {
        assert!((x - 740.0).abs() < 0.1, "Expected x≈740, got {}", x);
        assert!((y - 360.0).abs() < 0.1, "Expected y≈360, got {}", y);
    } else {
        panic!("Expected Vec2 override for tracker.at, got {:?}", tracker_at);
    }
}
