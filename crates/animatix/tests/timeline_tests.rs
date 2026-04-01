use animatix::ast::{Expr, Property, Stmt, Time};
use animatix::timeline::{ActorState, AnimationTrack, Timeline, parse_color, time_to_ms};

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
fn test_actor_state_interpolation() {
    let mut state1 = ActorState::new();
    state1.position = [0.0, 0.0];
    state1.size = [10.0, 10.0];
    state1.color = [1.0, 0.0, 0.0, 1.0];
    state1.shape_type = 0;
    state1.opacity = 1.0;

    let mut state2 = ActorState::new();
    state2.position = [100.0, 50.0];
    state2.size = [20.0, 30.0];
    state2.color = [0.0, 1.0, 0.0, 0.5];
    state2.shape_type = 1;
    state2.opacity = 0.5;

    let interpolated = state1.interpolate(&state2, 0.5);

    assert_eq!(interpolated.position, [50.0, 25.0]);
    assert_eq!(interpolated.size, [15.0, 20.0]);
    assert_eq!(interpolated.color, [0.5, 0.5, 0.0, 0.75]);
    assert_eq!(interpolated.shape_type, 1);
    assert_eq!(interpolated.opacity, 0.75);

    // shape_type switches at 0.5
    let before_mid = state1.interpolate(&state2, 0.49);
    assert_eq!(before_mid.shape_type, 0);

    let after_mid = state1.interpolate(&state2, 0.51);
    assert_eq!(after_mid.shape_type, 1);
}

#[test]
fn test_animation_track_evaluation() {
    let mut track = AnimationTrack::new("test_actor".to_string());

    let mut state1 = ActorState::new();
    state1.position = [0.0, 0.0];

    let mut state2 = ActorState::new();
    state2.position = [100.0, 0.0];

    let mut state3 = ActorState::new();
    state3.position = [100.0, 100.0];

    track.add_keyframe(0.0, state1);
    track.add_keyframe(1000.0, state2);
    track.add_keyframe(2000.0, state3);

    // Before first keyframe
    assert_eq!(track.evaluate(-500.0).position, [0.0, 0.0]);

    // Exactly at first keyframe
    assert_eq!(track.evaluate(0.0).position, [0.0, 0.0]);

    // Midway between 1st and 2nd
    assert_eq!(track.evaluate(500.0).position, [50.0, 0.0]);

    // Exactly at 2nd keyframe
    assert_eq!(track.evaluate(1000.0).position, [100.0, 0.0]);

    // Midway between 2nd and 3rd
    assert_eq!(track.evaluate(1500.0).position, [100.0, 50.0]);

    // Beyond last keyframe
    assert_eq!(track.evaluate(2500.0).position, [100.0, 100.0]);
}

#[test]
fn test_timeline_build_and_evaluate() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
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

    // Evaluate at 0.5s (500ms)
    let instances = timeline.evaluate(0.5);
    assert_eq!(instances.len(), 1);

    let instance = &instances[0];

    // Position should be interpolated to 50.0, 50.0
    assert_eq!(instance.position, [50.0, 50.0]);

    // Color should be interpolated between red [1.0, 0.0, 0.0, 1.0] and blue [0.0, 0.0, 1.0, 1.0]
    // i.e., [0.5, 0.0, 0.5, 1.0]
    assert_eq!(instance.fill_color, [0.5, 0.0, 0.5, 1.0]);
}

#[test]
fn test_missing_properties() {
    let mut track = AnimationTrack::new("empty_actor".to_string());

    // An empty state uses defaults
    track.add_keyframe(0.0, ActorState::new());

    let state = track.evaluate(0.0);
    assert_eq!(state.position, [0.0, 0.0]);
    assert_eq!(state.size, [50.0, 50.0]);
    assert_eq!(state.color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(state.shape_type, 0);
    assert_eq!(state.opacity, 1.0);
}
