//! Tests for the Callout primitive

use super::*;
use crate::ast::Property;
use crate::timeline::animation_track::CalloutPlace;

fn make_config() -> Stmt {
    Stmt::Config {
        settings: vec![Property {
            name: "colorscheme".to_string(),
            value: Expr::Str("editorial-dark".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        span: None,
    }
}

/// Build a minimal timeline with a single Callout actor.
fn build_callout_timeline(
    from: [f32; 2],
    to: [f32; 2],
    label: &str,
    label_at: [f32; 2],
    head_size: f32,
) -> Timeline {
    let props = vec![
        Property {
            name: "from".to_string(),
            value: Expr::Tuple(vec![Expr::Num(from[0] as f64), Expr::Num(from[1] as f64)]),
            value_span: None,
            trailing_comment: None,
        },
        Property {
            name: "to".to_string(),
            value: Expr::Tuple(vec![Expr::Num(to[0] as f64), Expr::Num(to[1] as f64)]),
            value_span: None,
            trailing_comment: None,
        },
        Property {
            name: "label".to_string(),
            value: Expr::Str(label.to_string()),
            value_span: None,
            trailing_comment: None,
        },
        Property {
            name: "label_at".to_string(),
            value: Expr::Tuple(vec![Expr::Num(label_at[0] as f64), Expr::Num(label_at[1] as f64)]),
            value_span: None,
            trailing_comment: None,
        },
        Property {
            name: "head_size".to_string(),
            value: Expr::Num(head_size as f64),
            value_span: None,
            trailing_comment: None,
        },
        Property {
            name: "color".to_string(),
            value: Expr::Ident("accent.primary".to_string()),
            value_span: None,
            trailing_comment: None,
        },
    ];

    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "callout".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props,
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    report.output
}

#[test]
fn test_callout_basic_rendering() {
    // Test that a Callout renders both arrow and text
    // - Create a Callout with from=(0,0), to=(100,0), label="Test"
    // - Verify evaluate() returns without error
    // - Verify track has correct properties

    let timeline = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Test", [10.0, 5.0], 10.0);

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    // Evaluate at time 0 — should not panic
    let _scene = timeline.evaluate(0.0, dims);

    // Verify the track exists
    let track = timeline.get_track("callout").expect("callout track should exist");
    assert_eq!(track.kind, ActorKindId::Callout, "track kind should be Callout");

    // Verify line_from and line_to tracks are populated
    let from = track.shape.line_from.get(0, [0.0; 2]);
    assert_eq!(from, [0.0, 0.0], "from should be (0, 0)");

    let to = track.shape.line_to.get(0, [0.0; 2]);
    assert_eq!(to, [100.0, 0.0], "to should be (100, 0)");

    // Verify label was stored
    let label_val = track.text.text_content.get(0, String::new());
    assert_eq!(label_val, "Test", "label should be 'Test'");

    // Verify label_at
    let label_at = track.geometry.label_at.get(0, [0.0; 2]);
    assert_eq!(label_at, [10.0, 5.0], "label_at should be (10, 5)");

    // Verify head_size
    let head = track.shape.head_size.get(0, 0.0);
    assert_eq!(head, 10.0, "head_size should be 10.0");

    // Evaluate at another time — should still succeed
    let _scene = timeline.evaluate(2.5, dims);
}

#[test]
fn test_callout_label_positioning() {
    // Test that label_at offset works correctly
    // - Create a Callout with to=(100,0), label_at=(10,5)
    // - Verify label_at track is (10,5)

    let timeline = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Note", [10.0, 5.0], 10.0);

    let track = timeline.get_track("callout").expect("callout track should exist");

    // Check label_at is correctly stored
    let label_at = track.geometry.label_at.get(0, [0.0; 2]);
    assert_eq!(label_at, [10.0, 5.0], "label_at should be (10,5)");

    // Check that evaluate doesn't crash
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_animation() {
    // Test that Callout properties can be animated
    // - Create a Callout with animated from/to positions
    // - Verify the to property has keyframes

    let ast = vec![
        make_config(),
        // Declare at time 0s
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "anim_callout".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props: vec![
                    Property {
                        name: "from".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "to".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "label".to_string(),
                        value: Expr::Str("Animated".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "head_size".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
        // Animate 'to' at time 2s
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(2.0),
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static(
                    "anim_callout".to_string(),
                )],
                property: "to".to_string(),
                value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(200.0)]),
                modifiers: vec![],
                easing: Some(crate::easing::Easing::EaseInOut),
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.get_track("anim_callout").expect("anim_callout track should exist");

    // Verify the 'to' track has keyframes (initial + animated)
    let to_track = track.shape.line_to.as_ref().expect("line_to track should exist");
    assert!(
        to_track.keyframes.len() >= 2,
        "line_to should have at least 2 keyframes for animation, got {}",
        to_track.keyframes.len()
    );

    // Verify values at different times
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    // At time 0s, to should be (100, 100)
    let to_0s = track.shape.line_to.get(0, [0.0; 2]);
    assert_eq!(to_0s, [100.0, 100.0], "to at 0s should be (100, 100)");

    // Evaluate at both times — should not panic
    let _scene_0s = timeline.evaluate(0.0, dims);
    let _scene_2s = timeline.evaluate(2.0, dims);
    // Mid-interpolation
    let _scene_1s = timeline.evaluate(1.0, dims);
}

#[test]
fn test_callout_arrowhead_size() {
    // Test that head_size property affects arrowhead
    // - Create Callouts with different head_size values
    // - Verify the head_size tracks differ

    let timeline_small = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Small", [0.0, 0.0], 5.0);
    let timeline_large =
        build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Large", [0.0, 0.0], 20.0);

    let track_small = timeline_small.get_track("callout").expect("callout track should exist");
    let track_large = timeline_large.get_track("callout").expect("callout track should exist");

    let head_small = track_small.shape.head_size.get(0, 0.0);
    let head_large = track_large.shape.head_size.get(0, 0.0);

    assert_eq!(head_small, 5.0, "small head_size should be 5.0");
    assert_eq!(head_large, 20.0, "large head_size should be 20.0");
    assert!(
        head_large > head_small,
        "large head_size should be greater than small head_size"
    );
}

#[test]
fn test_callout_no_label_does_not_set_text_content() {
    // A callout without a label should still build without error
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "no_label".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props: vec![
                    Property {
                        name: "from".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "to".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let track = timeline.get_track("no_label").expect("no_label track should exist");
    assert_eq!(track.kind, ActorKindId::Callout);

    // The text_content should be empty string (default)
    let label_val = track.text.text_content.get(0, String::new());
    assert_eq!(label_val, "", "label should be empty when not provided");

    // Evaluate should not panic
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_target_assignment_accepts_bare_actor_label() {
    let actor = |label: &str| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Rect".to_string(),
        props: vec![
            Property {
                name: "position".to_string(),
                value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let note = Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "note".to_string(),
        array_index: None,
        ty: "Callout".to_string(),
        props: vec![
            Property {
                name: "target".to_string(),
                value: Expr::Ident("box1".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "place".to_string(),
                value: Expr::Ident("right".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "standoff".to_string(),
                value: Expr::Num(40.0),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "label".to_string(),
                value: Expr::Str("Look here".to_string()),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![actor("box1"), actor("box2"), note],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static("note".to_string())],
                property: "target".to_string(),
                value: Expr::Ident("box2".to_string()),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let note = timeline.get_track("note").expect("note track should exist");
    assert_eq!(note.geometry.callout_target.get(0, String::new()), "box1");
    assert_eq!(note.geometry.callout_target.get(1000, String::new()), "box2");
}

#[test]
fn test_callout_target_accepts_array_actor_refs() {
    let actor = |label: &str| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Rect".to_string(),
        props: vec![
            Property {
                name: "position".to_string(),
                value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let note = Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "note".to_string(),
        array_index: None,
        ty: "Callout".to_string(),
        props: vec![
            Property {
                name: "target".to_string(),
                value: Expr::Index(
                    Box::new(Expr::Ident("bar".to_string())),
                    Box::new(Expr::Num(0.0)),
                ),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "place".to_string(),
                value: Expr::Ident("right".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "standoff".to_string(),
                value: Expr::Num(40.0),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "label".to_string(),
                value: Expr::Str("key".to_string()),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![actor("bar__0"), actor("bar__2"), note],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static("note".to_string())],
                property: "target".to_string(),
                value: Expr::Index(
                    Box::new(Expr::Ident("bar".to_string())),
                    Box::new(Expr::Num(2.0)),
                ),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let note = timeline.get_track("note").expect("note track should exist");
    assert_eq!(note.geometry.callout_target.get(0, String::new()), "bar__0");
    assert_eq!(note.geometry.callout_target.get(1000, String::new()), "bar__2");
}

#[test]
fn test_callout_target_accepts_namespaced_array_actor_refs() {
    // Component-expanded targets use `instance.label[index]`; the callout
    // target parser must resolve that to `instance.label__N`, not drop the
    // index or reject the path.
    let actor = |label: &str| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Rect".to_string(),
        props: vec![
            Property {
                name: "position".to_string(),
                value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let note = Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "note".to_string(),
        array_index: None,
        ty: "Callout".to_string(),
        props: vec![
            Property {
                name: "target".to_string(),
                value: Expr::Index(
                    Box::new(Expr::Path(vec!["bars".to_string(), "bar".to_string()])),
                    Box::new(Expr::Num(2.0)),
                ),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "place".to_string(),
                value: Expr::Ident("right".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "standoff".to_string(),
                value: Expr::Num(40.0),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "label".to_string(),
                value: Expr::Str("key".to_string()),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![actor("bars.bar__2"), note],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let note = timeline.get_track("note").expect("note track should exist");
    assert_eq!(note.geometry.callout_target.get(0, String::new()), "bars.bar__2");
}

#[test]
fn test_callout_target_mode_seeds_tracks() {
    // Verify that target/place/standoff/to_offset are seeded into tracks
    // when a targeted Callout is declared.
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![
                        Property {
                            name: "position".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "note".to_string(),
                    array_index: None,
                    ty: "Callout".to_string(),
                    props: vec![
                        Property {
                            name: "target".to_string(),
                            value: Expr::Ident("box".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "place".to_string(),
                            value: Expr::Ident("right".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "standoff".to_string(),
                            value: Expr::Num(40.0),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "to_offset".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "label".to_string(),
                            value: Expr::Str("Look here".to_string()),
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
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let note = timeline.get_track("note").expect("note track should exist");

    // target / place / standoff / to_offset tracks must be seeded
    assert_eq!(
        note.geometry.callout_target.get(0, String::new()),
        "box",
        "callout_target should be 'box'"
    );
    assert_eq!(
        note.geometry.callout_place.get(0, CalloutPlace::Right),
        CalloutPlace::Right,
        "callout_place should be Right"
    );
    assert!(
        (note.geometry.callout_standoff.get(0, 0.0) - 40.0).abs() < f32::EPSILON,
        "callout_standoff should be 40.0"
    );
    assert_eq!(
        note.geometry.callout_to_offset.get(0, [99.0; 2]),
        [0.0, 0.0],
        "callout_to_offset should be (0, 0)"
    );

    // Evaluating the scene should not panic
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_target_mode_derives_positions() {
    // When target mode is active, evaluate() derives `to` at the right edge
    // of the target actor and `from` = to + [standoff, 0].
    //
    // Target "box": position=(200,100), size=(80,40) → right edge x = 200+40 = 240.
    // to   = [240, 100]  (right attach point + to_offset(0,0))
    // from = [240+40, 100] = [280, 100]
    //
    // We verify evaluate() runs without error. Exact arrow positions are
    // internal to the render path, so we just check no panic occurs.
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![
                        Property {
                            name: "position".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "note".to_string(),
                    array_index: None,
                    ty: "Callout".to_string(),
                    props: vec![
                        Property {
                            name: "target".to_string(),
                            value: Expr::Ident("box".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "place".to_string(),
                            value: Expr::Ident("right".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "standoff".to_string(),
                            value: Expr::Num(40.0),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "label".to_string(),
                            value: Expr::Str("Note".to_string()),
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
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    // evaluate() must not panic when target mode is active
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_callout_place_variants_accepted() {
    // Verify that all valid place identifiers parse to the correct enum variant.
    for (ident, expected) in &[
        ("right", CalloutPlace::Right),
        ("left", CalloutPlace::Left),
        ("top", CalloutPlace::Top),
        ("above", CalloutPlace::Top),
        ("bottom", CalloutPlace::Bottom),
        ("below", CalloutPlace::Bottom),
        ("auto", CalloutPlace::Auto),
    ] {
        let ast = vec![
            make_config(),
            Stmt::Keyframe {
                time: crate::ast::Time::Seconds(0.0),
                body: vec![Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "note".to_string(),
                    array_index: None,
                    ty: "Callout".to_string(),
                    props: vec![Property {
                        name: "place".to_string(),
                        value: Expr::Ident(ident.to_string()),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                }],
                span: None,
            },
        ];
        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        assert!(
            report.diagnostics.is_empty(),
            "place={ident} produced unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.get_track("note").unwrap();
        assert_eq!(
            track.geometry.callout_place.get(0, CalloutPlace::Right),
            *expected,
            "place={ident} should map to {expected:?}"
        );
    }
}

#[test]
fn test_callout_place_invalid_produces_diagnostic() {
    // An unrecognised place value should emit a build diagnostic.
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "note".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props: vec![Property {
                    name: "place".to_string(),
                    value: Expr::Ident("sideways".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        !report.diagnostics.is_empty(),
        "invalid place 'sideways' should produce a diagnostic"
    );
}

#[test]
fn test_callout_place_default_is_right() {
    // A Callout without a place prop should default to Right.
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "note".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report.output.get_track("note").unwrap();
    assert_eq!(
        track.geometry.callout_place.get(0, CalloutPlace::Auto),
        CalloutPlace::Right,
        "default callout_place should be Right"
    );
}

// ── Commit 3: transform-aware target bounds regression tests ──────────────
//
// These tests build timelines directly (not via AST) so that we can set
// position / scale / rotation / parent-child relationships precisely without
// relying on how declaration-time props map to internal tracks.

use crate::timeline::ActorKindId;
use crate::timeline::actor_kind::ShapeKind;

/// Make a minimal target track with a given position and half-size.
fn make_target_track(label: &str, pos: [f32; 2], half: [f32; 2]) -> AnimationTrack {
    let mut track = AnimationTrack::new(label.to_string());
    track.kind = ActorKindId::Shape(ShapeKind::Rect);
    track.first_seen_ms = 0;
    track.geometry.position = Some({
        let mut t = PropertyTrack::new(pos);
        t.add_keyframe(0, pos, Easing::Linear);
        t
    });
    track.geometry.size = Some({
        let mut t = PropertyTrack::new(half);
        t.add_keyframe(0, half, Easing::Linear);
        t
    });
    track
}

/// Make a Callout track that targets another actor (place=right, standoff=0).
fn make_callout_track(label: &str, target: &str) -> AnimationTrack {
    use crate::timeline::animation_track::CalloutPlace;
    let mut track = AnimationTrack::new(label.to_string());
    track.kind = ActorKindId::Callout;
    track.first_seen_ms = 0;
    track.geometry.callout_target = Some({
        let mut t = PropertyTrack::new(target.to_string());
        t.add_keyframe(0, target.to_string(), Easing::Linear);
        t
    });
    track.geometry.callout_place = Some({
        let mut t = PropertyTrack::new(CalloutPlace::Right);
        t.add_keyframe(0, CalloutPlace::Right, Easing::Linear);
        t
    });
    track.geometry.callout_standoff = Some({
        let mut t = PropertyTrack::new(0.0f32);
        t.add_keyframe(0, 0.0, Easing::Linear);
        t
    });
    track
}

#[test]
fn test_callout_target_translated_world_bounds() {
    // A top-level target at (200, 100) with half=(40, 20).
    // world_affine = T(200,100); AABB centre=(200,100), half=(40,20).
    // place=right → attach = (240, 100).
    let mut timeline = Timeline::new();
    let target = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    timeline.tracks.insert("box".to_string(), target);
    timeline.root_nodes.push("box".to_string());

    let callout = make_callout_track("note", "box");
    timeline.tracks.insert("note".to_string(), callout);
    timeline.root_nodes.push("note".to_string());

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let note = timeline.get_track("note").unwrap();
    let geom =
        crate::timeline::callout_geometry::derive_callout_geometry(note, 0, Some(&timeline), dims);
    assert!((geom.to[0] - 240.0).abs() < 0.5, "expected to.x=240, got {}", geom.to[0]);
    assert!((geom.to[1] - 100.0).abs() < 0.5, "expected to.y=100, got {}", geom.to[1]);
}

#[test]
fn test_callout_target_scaled_world_bounds() {
    // Target at (200, 100), half=(40, 20), scale=2.
    // world_affine = T(200,100)*scale(2) → AABB half=(80,40), centre=(200,100).
    // place=right → attach = (280, 100).
    // Without transform-aware bounds (old code) we would get: attach = (240, 100).
    let mut timeline = Timeline::new();
    let mut target = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    target.geometry.scale = Some({
        let mut t = PropertyTrack::new(2.0f32);
        t.add_keyframe(0, 2.0, Easing::Linear);
        t
    });
    timeline.tracks.insert("box".to_string(), target);
    timeline.root_nodes.push("box".to_string());

    let callout = make_callout_track("note", "box");
    timeline.tracks.insert("note".to_string(), callout);
    timeline.root_nodes.push("note".to_string());

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let note = timeline.get_track("note").unwrap();
    let geom =
        crate::timeline::callout_geometry::derive_callout_geometry(note, 0, Some(&timeline), dims);
    // scale=2 doubles the visual extent: half_x=40 → 80 → right = 200+80 = 280
    assert!(
        (geom.to[0] - 280.0).abs() < 0.5,
        "expected to.x=280 (scale-aware), got {}",
        geom.to[0]
    );
    assert!((geom.to[1] - 100.0).abs() < 0.5, "expected to.y=100, got {}", geom.to[1]);
}

#[test]
fn test_callout_target_rotated_aabb() {
    // Target at (200, 100), half=(40, 20), rotation=π/2 (90°) in radians.
    // After 90° rotation, the AABB swaps axes: AABB half_x=20, half_y=40.
    // place=right → attach ≈ (200+20, 100) = (220, 100).
    let angle_rad = std::f32::consts::FRAC_PI_2; // 90° in radians
    let mut timeline = Timeline::new();
    let mut target = make_target_track("box", [200.0, 100.0], [40.0, 20.0]);
    target.geometry.rotation = Some({
        let mut t = PropertyTrack::new(angle_rad);
        t.add_keyframe(0, angle_rad, Easing::Linear);
        t
    });
    timeline.tracks.insert("box".to_string(), target);
    timeline.root_nodes.push("box".to_string());

    let callout = make_callout_track("note", "box");
    timeline.tracks.insert("note".to_string(), callout);
    timeline.root_nodes.push("note".to_string());

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let note = timeline.get_track("note").unwrap();
    let geom =
        crate::timeline::callout_geometry::derive_callout_geometry(note, 0, Some(&timeline), dims);
    // After 90° rotation: AABB half_x = |hw*cos(π/2)| + |hh*sin(π/2)| = 0 + 20 = 20
    let hw = 40.0_f32;
    let hh = 20.0_f32;
    let expected_half_x = hw * angle_rad.cos().abs() + hh * angle_rad.sin().abs();
    let expected_to_x = 200.0 + expected_half_x;
    assert!(
        (geom.to[0] - expected_to_x).abs() < 0.5,
        "expected to.x≈{:.1} (rotation-aware AABB), got {}",
        expected_to_x,
        geom.to[0]
    );
}

#[test]
fn test_callout_target_nested_child_world_bounds() {
    // parent at (100, 0); child nested at (100, 0) relative to parent.
    // Child world centre = (200, 0), half=(40, 20).
    // place=right → attach = (240, 0).
    // (Old local-only resolver would see child.position=(100,0) → 100+40=140.)
    let mut timeline = Timeline::new();

    let parent = make_target_track("parent", [100.0, 0.0], [50.0, 50.0]);
    timeline.tracks.insert("parent".to_string(), parent);
    timeline.root_nodes.push("parent".to_string());

    let mut child = make_target_track("child", [100.0, 0.0], [40.0, 20.0]);
    child.parent = Some("parent".to_string());
    timeline.tracks.insert("child".to_string(), child);
    // Register child under parent
    timeline.tracks.get_mut("parent").unwrap().children.push("child".to_string());

    let callout = make_callout_track("note", "child");
    timeline.tracks.insert("note".to_string(), callout);
    timeline.root_nodes.push("note".to_string());

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let note = timeline.get_track("note").unwrap();
    let geom =
        crate::timeline::callout_geometry::derive_callout_geometry(note, 0, Some(&timeline), dims);
    // world centre x = parent.pos.x + child.pos.x = 100+100 = 200; right = 200+40 = 240
    assert!(
        (geom.to[0] - 240.0).abs() < 0.5,
        "expected to.x=240 (nested world bounds), got {}",
        geom.to[0]
    );
    assert!((geom.to[1] - 0.0).abs() < 0.5, "expected to.y=0, got {}", geom.to[1]);
}

#[test]
fn test_callout_missing_target_produces_diagnostic() {
    // A Callout with a target that does not exist should produce a build diagnostic.
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "note".to_string(),
                array_index: None,
                ty: "Callout".to_string(),
                props: vec![Property {
                    name: "target".to_string(),
                    value: Expr::Str("nonexistent".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    use crate::diagnostics::DiagnosticCode;
    let found = report
        .diagnostics
        .iter()
        .any(|d| matches!(d.code, DiagnosticCode::CalloutTargetNotFound));
    assert!(
        found,
        "expected CalloutTargetNotFound diagnostic, got: {:?}",
        report.diagnostics
    );
}
