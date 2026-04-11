use animatix::ast::{Expr, Property, Stmt, Time};
use animatix::easing::Easing;
use animatix::renderer::text::TextPath;
use animatix::timeline::{
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
    Timeline, evaluate_expr, parse_color, time_to_ms,
};
use kurbo::Shape;

fn text_paths_width(paths: &[TextPath]) -> f64 {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;

    for text_path in paths {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
    }

    if min_x.is_finite() && max_x.is_finite() {
        max_x - min_x
    } else {
        0.0
    }
}

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
fn test_text_spacing_preserves_space_width() {
    let text_with_space = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Text {
            label: Some("spaced".to_string()),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("A B".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                },
            ],
            modifiers: vec![],
        }],
    }];

    let text_without_space = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Text {
            label: Some("tight".to_string()),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("AB".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                },
            ],
            modifiers: vec![],
        }],
    }];

    let spaced_timeline = Timeline::build(&text_with_space);
    let tight_timeline = Timeline::build(&text_without_space);

    let spaced_paths = &spaced_timeline
        .tracks
        .get("spaced")
        .expect("spaced track should exist")
        .text_paths
        .evaluate(0);
    let tight_paths = &tight_timeline
        .tracks
        .get("tight")
        .expect("tight track should exist")
        .text_paths
        .evaluate(0);

    assert!(text_paths_width(spaced_paths) > text_paths_width(tight_paths));
}

#[test]
fn test_missing_properties() {
    let track = AnimationTrack::new("empty_actor".to_string());

    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
    assert_eq!(track.size.evaluate(0), [50.0, 50.0]);
    assert_eq!(track.color.evaluate(0), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(track.shape_type.evaluate(0), 0);
    assert_eq!(track.opacity.evaluate(0), 1.0);
}

#[test]
fn test_row_child_with_explicit_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "origin_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_row_child_without_at_is_layout_managed() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                },
            ],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "auto_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("auto_child")
        .expect("auto_child track should exist");

    assert_eq!(
        track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_row_mixed_manual_and_layout_children() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                },
            ],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "manual_child".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                        },
                        Property {
                            name: "at".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "layout_child".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let manual_track = timeline
        .tracks
        .get("manual_child")
        .expect("manual_child track should exist");
    let layout_track = timeline
        .tracks
        .get("layout_child")
        .expect("layout_child track should exist");

    assert_eq!(
        manual_track.placement_mode.evaluate(0),
        PlacementMode::Manual
    );
    assert_eq!(manual_track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        layout_track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(layout_track.position.evaluate(0), [30.0, 0.0]);
}

#[test]
fn test_col_child_with_explicit_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "col".to_string(),
            ty: "Col".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(400.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                },
            ],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "origin_child".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(60.0)]),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_row_child_with_explicit_non_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "manual_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(45.0), Expr::Num(55.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("manual_child")
        .expect("manual_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [45.0, 55.0]);
}

#[test]
fn test_assignment_at_marks_manual_from_assignment_start() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "child".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: "child".to_string(),
                property: "at".to_string(),
                value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(50.0)]),
                modifiers: vec![animatix::ast::Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            }],
        },
    ];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("child")
        .expect("child track should exist");

    assert_eq!(track.placement_mode.evaluate(1000), PlacementMode::Manual);
    assert_eq!(track.placement_mode.evaluate(1500), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(1500), [50.0, 25.0]);
}

#[test]
fn test_root_row_without_at_uses_container_default_center_binding() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("row").expect("row track should exist");

    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::ContainerDefault {
            anchor: SceneAnchor::Center,
        }
    );
}

#[test]
fn test_grid_layout_positions_children_in_cells() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "grid".to_string(),
            ty: "Grid".to_string(),
            props: vec![
                Property {
                    name: "cols".to_string(),
                    value: Expr::Num(2.0),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                },
            ],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "a".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "b".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "c".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "d".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("a")
            .expect("a track")
            .position
            .evaluate(0),
        [-25.0, -15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("b")
            .expect("b track")
            .position
            .evaluate(0),
        [25.0, -15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("c")
            .expect("c track")
            .position
            .evaluate(0),
        [-25.0, 15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("d")
            .expect("d track")
            .position
            .evaluate(0),
        [25.0, 15.0]
    );
}

#[test]
fn test_stack_layout_overlaps_children_by_default() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "stack".to_string(),
            ty: "Stack".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "base".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(50.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "overlay".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(12.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("base")
            .expect("base track")
            .position
            .evaluate(0),
        [0.0, 0.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("overlay")
            .expect("overlay track")
            .position
            .evaluate(0),
        [0.0, 0.0]
    );
}

#[test]
fn test_scene_relative_bindings_are_recorded_on_tracks() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "anchored".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "anchor".to_string(),
                        value: Expr::Path(vec!["scene".to_string(), "top".to_string()]),
                    },
                    Property {
                        name: "offset".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "percent".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            },
        ],
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("anchored")
            .expect("anchored track")
            .position_binding
            .evaluate(0),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Top,
            offset: [0.0, 48.0],
        }
    );
    assert_eq!(
        timeline
            .tracks
            .get("percent")
            .expect("percent track")
            .position_binding
            .evaluate(0),
        PositionBinding::ScenePercent {
            x: 0.5,
            y: 0.25,
            offset: [0.0, 0.0],
        }
    );
}

#[test]
fn test_plot_without_at_stays_local_to_parent_graph() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "graph".to_string(),
            ty: "Graph".to_string(),
            props: vec![Property {
                name: "func".to_string(),
                value: Expr::Closure(
                    vec!["x".to_string()],
                    Box::new(Expr::Ident("x".to_string())),
                ),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "plot".to_string(),
                ty: "CartesianPlot".to_string(),
                props: vec![Property {
                    name: "func".to_string(),
                    value: Expr::Closure(
                        vec!["x".to_string()],
                        Box::new(Expr::Ident("x".to_string())),
                    ),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("plot")
        .expect("plot track should exist");
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
}

#[test]
fn test_evaluate_expr_sin_cos() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    // sin(0) = 0
    let result = evaluate_expr(&Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!({
        let v = result.as_num();
        v.abs() < 1e-10
    });

    // sin(PI/2) ≈ 1
    let result = evaluate_expr(
        &Expr::Call(
            "sin".to_string(),
            vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(0) = 1
    let result = evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(PI) ≈ -1
    let result = evaluate_expr(
        &Expr::Call("cos".to_string(), vec![Expr::Num(std::f64::consts::PI)]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() + 1.0).abs() < 1e-10);

    // sin nested: sin(PI/6) * 2
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call(
                "sin".to_string(),
                vec![Expr::Num(std::f64::consts::FRAC_PI_6)],
            )),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_format() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    // format("value: {}", 42)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![Expr::Str("value: {}".to_string()), Expr::Num(42.0)],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "value: 42");

    // format("x={}, y={}", 10, 20)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("x={}, y={}".to_string()),
                Expr::Num(10.0),
                Expr::Num(20.0),
            ],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "x=10, y=20");

    // format with no args
    let result = evaluate_expr(&Expr::Call("format".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "");

    // format with text and sin
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("sin(π/2) = {}".to_string()),
                Expr::Call(
                    "sin".to_string(),
                    vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
                ),
            ],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "sin(π/2) = 1");
}

#[test]
fn test_evaluate_expr_constants() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    assert!(
        (evaluate_expr(&Expr::Ident("PI".to_string()), &env)
            .unwrap_or(animatix::timeline::Value::Num(0.0))
            .as_num()
            - std::f64::consts::PI)
            .abs()
            < 1e-10
    );
    assert!(
        (evaluate_expr(&Expr::Ident("TAU".to_string()), &env)
            .unwrap_or(animatix::timeline::Value::Num(0.0))
            .as_num()
            - std::f64::consts::TAU)
            .abs()
            < 1e-10
    );
}

#[test]
fn test_evaluate_expr_tuple() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    let result = evaluate_expr(&Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [100.0, 200.0]);

    // Tuple with call expressions
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
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

#[test]
fn test_evaluate_expr_modulo() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Basic modulo: 10 % 3 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(10.0)),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(3.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with division: 7 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(7.0)),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with sin result: sin(PI/2) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call(
                "sin".to_string(),
                vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
            )),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Nested modulo: (10 % 3) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Num(10.0)),
                animatix::ast::BinaryOp::Mod,
                Box::new(Expr::Num(3.0)),
            )),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_vec2_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Vec2 addition: (10, 20) + (5, 5) = (15, 25)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(5.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [15.0, 25.0]);

    // Vec2 subtraction: (10, 20) - (3, 8) = (7, 12)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Sub,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(8.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [7.0, 12.0]);

    // Vec2 multiplication: (10, 20) * (2, 3) = (20, 60)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(3.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 60.0]);

    // Vec2 division: (10, 20) / (2, 4) = (5, 5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Div,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [5.0, 5.0]);

    // Vec2 modulo: (10, 21) % (3, 4) = (1, 1)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(21.0)])),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [1.0, 1.0]);

    // Scalar-Vec2 multiplication: 2 * (10, 20) = (20, 40)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 40.0]);

    // Vec2 with sin/cos: (sin(0), cos(0)) = (0, 1)
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
}

#[test]
fn test_evaluate_expr_vec3_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Vec3 addition: (1, 2, 3) + (4, 5, 6) = (5, 7, 9)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![
                Expr::Num(1.0),
                Expr::Num(2.0),
                Expr::Num(3.0),
            ])),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Tuple(vec![
                Expr::Num(4.0),
                Expr::Num(5.0),
                Expr::Num(6.0),
            ])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec3(), [5.0, 7.0, 9.0]);

    // Vec3 scalar multiplication: 2 * (1, 2, 3) = (2, 4, 6)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![
                Expr::Num(1.0),
                Expr::Num(2.0),
                Expr::Num(3.0),
            ])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec3(), [2.0, 4.0, 6.0]);
}

#[test]
fn test_evaluate_expr_color_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Color addition: RED + GREEN = (1, 1, 0, 2)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("RED".to_string())),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Ident("GREEN".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 1.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 0.0).abs() < 1e-10);
    assert!((color[3] - 2.0).abs() < 1e-10);

    // Color scalar multiplication: 0.5 * BLUE = (0, 0, 0.5, 0.5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(0.5)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Ident("BLUE".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 0.0).abs() < 1e-10);
    assert!((color[2] - 0.5).abs() < 1e-10);
    assert!((color[3] - 0.5).abs() < 1e-10);

    // Color subtraction: WHITE - RED = (0, 1, 1, 0) - alpha fades out
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("WHITE".to_string())),
            animatix::ast::BinaryOp::Sub,
            Box::new(Expr::Ident("RED".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_rand() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // rand() should return a value between 0 and 1
    let result = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    let val = result.as_num();
    assert!(
        val >= 0.0 && val < 1.0,
        "rand() should return value in [0, 1), got {}",
        val
    );

    // rand() should be called multiple times and produce different results
    let result1 = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    let result2 = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    // Note: This test might occasionally fail due to random collision, but extremely unlikely
    // In practice, rand() should produce different values; we test the range here
    let val1 = result1.as_num();
    let val2 = result2.as_num();
    assert!(val1 >= 0.0 && val1 < 1.0);
    assert!(val2 >= 0.0 && val2 < 1.0);

    // rand() with expressions: rand() * 100 should be in [0, 100)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call("rand".to_string(), vec![])),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Num(100.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let val = result.as_num();
    assert!(
        val >= 0.0 && val < 100.0,
        "rand() * 100 should be in [0, 100), got {}",
        val
    );
}
