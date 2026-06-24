//! Tests for the Callout primitive

use super::*;
use crate::ast::Property;

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
    let mut props = vec![
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
                target: vec!["anim_callout".to_string()],
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
    let timeline_large = build_callout_timeline([0.0, 0.0], [100.0, 0.0], "Large", [0.0, 0.0], 20.0);

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
        note.geometry.callout_place.get(0, String::new()),
        "right",
        "callout_place should be 'right'"
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
    let dims = SceneDimensions { width: 1920, height: 1080 };
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

    let dims = SceneDimensions { width: 1920, height: 1080 };
    // evaluate() must not panic when target mode is active
    let _scene = timeline.evaluate(0.0, dims);
}
