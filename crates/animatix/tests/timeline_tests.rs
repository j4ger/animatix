use animatix::ast::{Expr, Property, Stmt, Time};
use animatix::easing::Easing;
use animatix::timeline::{
    evaluate_expr, parse_color, time_to_ms, AnimationTrack, Interpolate, PropertyTrack, Timeline,
};

#[test]
fn test_time_to_ms() {
    assert_eq!(time_to_ms(&Time::Seconds(2.5)), 2500.0);
    assert_eq!(time_to_ms(&Time::Milliseconds(500)), 500.0);
}

#[test]
fn test_parse_color() {
    assert_eq!(
        parse_color(&Expr::Ident("red".to_string())),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        parse_color(&Expr::Ident("unknown".to_string())),
        [0.8, 0.8, 0.8, 1.0]
    );
    assert_eq!(parse_color(&Expr::Num(1.0)), [0.8, 0.8, 0.8, 1.0]);
}

#[test]
fn test_interpolation() {
    let p1: [f32; 2] = [0.0, 0.0];
    let p2: [f32; 2] = [100.0, 50.0];

    let interpolated = p1.interpolate(&p2, 0.5);
    assert_eq!(interpolated, [50.0, 25.0]);
}

#[test]
fn test_property_track_evaluation() {
    let mut track = PropertyTrack::new([0.0, 0.0]);

    track.add_keyframe(0, [0.0, 0.0], Easing::Linear);
    track.add_keyframe(1000, [100.0, 0.0], Easing::Linear);
    track.add_keyframe(2000, [100.0, 100.0], Easing::Linear);

    // Exactly at first keyframe
    assert_eq!(track.evaluate(0), [0.0, 0.0]);

    // Midway between 1st and 2nd
    assert_eq!(track.evaluate(500), [50.0, 0.0]);

    // Exactly at 2nd keyframe
    assert_eq!(track.evaluate(1000), [100.0, 0.0]);

    // Midway between 2nd and 3rd
    assert_eq!(track.evaluate(1500), [100.0, 50.0]);

    // Beyond last keyframe
    assert_eq!(track.evaluate(2500), [100.0, 100.0]);
}

#[test]
fn test_timeline_build_and_evaluate() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "actor1".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("red".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "actor1".to_string(),
                ty: "Square".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("blue".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        },
    ];

    let timeline = Timeline::build(&ast);
    assert_eq!(timeline.tracks.len(), 1);

    // Evaluate at 0.5s (500ms) — access the track directly instead of inspecting the rendered Scene
    let track = timeline
        .tracks
        .get("actor1")
        .expect("actor1 track should exist");
    let position = track.position.evaluate(500);
    let color = track.color.evaluate(500);

    // Position should be interpolated from [0.0, 0.0] to [100.0, 100.0] at 500ms → [50.0, 50.0]
    assert_eq!(position, [50.0, 50.0]);

    // Color should be interpolated between red [1.0, 0.0, 0.0, 1.0] and blue [0.0, 0.0, 1.0, 1.0]
    // i.e., [0.5, 0.0, 0.5, 1.0]
    assert_eq!(color, [0.5, 0.0, 0.5, 1.0]);
}

#[test]
fn test_missing_properties() {
    let track = AnimationTrack::new("empty_actor".to_string());

    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(track.size.evaluate(0), [50.0, 50.0]);
    assert_eq!(track.color.evaluate(0), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(track.shape_type.evaluate(0), 0);
    assert_eq!(track.opacity.evaluate(0), 1.0);
}

#[test]
fn test_evaluate_expr_sin_cos() {
    // sin(0) = 0
    let result = evaluate_expr(&Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]));
    assert!({
        let v = result.as_num();
        v.abs() < 1e-10
    });

    // sin(PI/2) ≈ 1
    let result = evaluate_expr(&Expr::Call(
        "sin".to_string(),
        vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
    ));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(0) = 1
    let result = evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(PI) ≈ -1
    let result = evaluate_expr(&Expr::Call(
        "cos".to_string(),
        vec![Expr::Num(std::f64::consts::PI)],
    ));
    assert!((result.as_num() + 1.0).abs() < 1e-10);

    // sin nested: sin(PI/6) * 2
    let result = evaluate_expr(&Expr::Binary(
        Box::new(Expr::Call(
            "sin".to_string(),
            vec![Expr::Num(std::f64::consts::FRAC_PI_6)],
        )),
        animatix::ast::BinaryOp::Mul,
        Box::new(Expr::Num(2.0)),
    ));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_format() {
    // format("value: {}", 42)
    let result = evaluate_expr(&Expr::Call(
        "format".to_string(),
        vec![Expr::Str("value: {}".to_string()), Expr::Num(42.0)],
    ));
    assert_eq!(result.as_str(), "value: 42");

    // format("x={}, y={}", 10, 20)
    let result = evaluate_expr(&Expr::Call(
        "format".to_string(),
        vec![
            Expr::Str("x={}, y={}".to_string()),
            Expr::Num(10.0),
            Expr::Num(20.0),
        ],
    ));
    assert_eq!(result.as_str(), "x=10, y=20");

    // format with no args
    let result = evaluate_expr(&Expr::Call("format".to_string(), vec![]));
    assert_eq!(result.as_str(), "");

    // format with text and sin
    let result = evaluate_expr(&Expr::Call(
        "format".to_string(),
        vec![
            Expr::Str("sin(π/2) = {}".to_string()),
            Expr::Call(
                "sin".to_string(),
                vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
            ),
        ],
    ));
    assert_eq!(result.as_str(), "sin(π/2) = 1");
}

#[test]
fn test_evaluate_expr_constants() {
    assert!(
        (evaluate_expr(&Expr::Ident("PI".to_string())).as_num() - std::f64::consts::PI).abs()
            < 1e-10
    );
    assert!(
        (evaluate_expr(&Expr::Ident("TAU".to_string())).as_num() - std::f64::consts::TAU).abs()
            < 1e-10
    );
}

#[test]
fn test_evaluate_expr_tuple() {
    let result = evaluate_expr(&Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]));
    assert_eq!(result.as_tuple2(), [100.0, 200.0]);

    // Tuple with call expressions
    let result = evaluate_expr(&Expr::Tuple(vec![
        Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
        Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
    ]));
    assert_eq!(result.as_tuple2(), [0.0, 1.0]);
}

#[test]
fn test_timeline_with_expr_call_properties() {
    // Verify that sin/cos expressions work in property values during timeline build
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Assignment {
            target: "actor1".to_string(),
            property: "position".to_string(),
            value: Expr::Tuple(vec![
                Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
                Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
            ]),
            modifiers: vec![],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("actor1").expect("actor1 should exist");
    let pos = track.position.evaluate(0);
    // sin(0)=0, cos(0)=1
    assert!((pos[0] - 0.0).abs() < 1e-6);
    assert!((pos[1] - 1.0).abs() < 1e-6);
}
