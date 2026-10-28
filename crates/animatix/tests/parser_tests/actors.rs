use super::*;

#[test]
fn test_svg_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "icon: Svg { url: \"examples/vector.svg\", anchor: scene.top, offset: (0, 24), scale: 1.5 }"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "icon".to_string(),
            array_index: None,
            ty: "Svg".to_string(),
            props: vec![
                Property {
                    name: "url".to_string(),
                    value: Expr::Str("examples/vector.svg".to_string()),
                    value_span: Some(ByteSpan { start: 17, end: 38 }),
                    trailing_comment: None,
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "top".to_string()]),
                    value_span: Some(ByteSpan { start: 48, end: 57 }),
                    trailing_comment: None,
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(24.0)]),
                    value_span: Some(ByteSpan { start: 67, end: 74 }),
                    trailing_comment: None,
                },
                Property {
                    name: "scale".to_string(),
                    value: Expr::Num(1.5),
                    value_span: Some(ByteSpan { start: 83, end: 86 }),
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_image_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.png\", anchor: scene.bottom, offset: (0, -40), size: (240, 180) }"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "photo".to_string(),
            array_index: None,
            ty: "Image".to_string(),
            props: vec![
                Property {
                    name: "url".to_string(),
                    value: Expr::Str("examples/checker.png".to_string()),
                    value_span: Some(ByteSpan { start: 20, end: 42 }),
                    trailing_comment: None,
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "bottom".to_string()]),
                    value_span: Some(ByteSpan { start: 52, end: 64 }),
                    trailing_comment: None,
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Num(0.0),
                        Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(40.0))),
                    ]),
                    value_span: Some(ByteSpan { start: 74, end: 82 }),
                    trailing_comment: None,
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(180.0)]),
                    value_span: Some(ByteSpan {
                        start: 90,
                        end: 100
                    }),
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_line_actor_decl() {
    assert_eq!(
        parse_single_stmt("axis: Line, from: (-40, 0), to: (40, 0), stroke: blue, stroke_width: 3"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "axis".to_string(),
            array_index: None,
            ty: "Line".to_string(),
            props: vec![
                Property {
                    name: "from".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(40.0))),
                        Expr::Num(0.0),
                    ]),
                    value_span: Some(ByteSpan { start: 18, end: 26 }),
                    trailing_comment: None,
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(0.0)]),
                    value_span: Some(ByteSpan { start: 32, end: 39 }),
                    trailing_comment: None,
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("blue".to_string()),
                    value_span: Some(ByteSpan { start: 49, end: 53 }),
                    trailing_comment: None,
                },
                Property {
                    name: "stroke_width".to_string(),
                    value: Expr::Num(3.0),
                    value_span: Some(ByteSpan { start: 69, end: 70 }),
                    trailing_comment: None,
                }
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_ellipse_actor_decl() {
    assert_eq!(
        parse_single_stmt("halo: Ellipse, radius_x: 80, radius_y: 30, color: green"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "halo".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                    value_span: Some(ByteSpan { start: 25, end: 27 }),
                    trailing_comment: None,
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(30.0),
                    value_span: Some(ByteSpan { start: 39, end: 41 }),
                    trailing_comment: None,
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("green".to_string()),
                    value_span: Some(ByteSpan { start: 50, end: 55 }),
                    trailing_comment: None,
                }
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_arc_actor_decl() {
    assert_eq!(
        parse_single_stmt(
            "ring: Ellipse, radius_x: 80, radius_y: 50, start_angle: 0, sweep_angle: 3.14, stroke: gold"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "ring".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                    value_span: Some(ByteSpan { start: 25, end: 27 }),
                    trailing_comment: None,
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(50.0),
                    value_span: Some(ByteSpan { start: 39, end: 41 }),
                    trailing_comment: None,
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(0.0),
                    value_span: Some(ByteSpan { start: 56, end: 57 }),
                    trailing_comment: None,
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(3.14),
                    value_span: Some(ByteSpan { start: 72, end: 76 }),
                    trailing_comment: None,
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("gold".to_string()),
                    value_span: Some(ByteSpan { start: 86, end: 90 }),
                    trailing_comment: None,
                }
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_polygon_actor_decl() {
    assert_eq!(
        parse_single_stmt(
            "badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "badge".to_string(),
            array_index: None,
            ty: "Polygon".to_string(),
            props: vec![
                Property {
                    name: "points".to_string(),
                    value: Expr::List(vec![
                        Expr::Tuple(vec![
                            Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(80.0))),
                            Expr::Num(0.0),
                        ]),
                        Expr::Tuple(vec![
                            Expr::Num(0.0),
                            Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(70.0)))
                        ]),
                        Expr::Tuple(vec![Expr::Num(90.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(80.0)]),
                    ]),
                    value_span: Some(ByteSpan { start: 24, end: 62 }),
                    trailing_comment: None,
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("cyan".to_string()),
                    value_span: Some(ByteSpan { start: 71, end: 75 }),
                    trailing_comment: None,
                }
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_path_actor_decl() {
    assert_eq!(
        parse_single_stmt(
            "guide: Path, commands: {move_to(-120, 0), line_to(-40, -80), curve_to(20, -120, 80, 40, 140, -10), close()}, stroke: white"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "guide".to_string(),
            array_index: None,
            ty: "Path".to_string(),
            props: vec![
                Property {
                    name: "commands".to_string(),
                    value: Expr::List(vec![
                        Expr::Call(
                            "move_to".to_string(),
                            vec![
                                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(120.0))),
                                Expr::Num(0.0)
                            ],
                        ),
                        Expr::Call(
                            "line_to".to_string(),
                            vec![
                                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(40.0))),
                                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(80.0))),
                            ],
                        ),
                        Expr::Call(
                            "curve_to".to_string(),
                            vec![
                                Expr::Num(20.0),
                                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(120.0))),
                                Expr::Num(80.0),
                                Expr::Num(40.0),
                                Expr::Num(140.0),
                                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(10.0))),
                            ],
                        ),
                        Expr::Call("close".to_string(), vec![]),
                    ]),
                    value_span: Some(ByteSpan {
                        start: 23,
                        end: 107
                    }),
                    trailing_comment: None,
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("white".to_string()),
                    value_span: Some(ByteSpan {
                        start: 117,
                        end: 122
                    }),
                    trailing_comment: None,
                }
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_actor_decl_nested() {
    assert_eq!(
        parse_single_stmt("group: Group { a: Ellipse, size: 10, b: Rect, size: 20 }"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "group".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix_syntax::ast::InlineItem::Labeled {
                    label: "a".to_string(),
                    array_index: None,
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(10.0),
                        value_span: Some(ByteSpan { start: 33, end: 35 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![]
                },
                animatix_syntax::ast::InlineItem::Labeled {
                    label: "b".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(20.0),
                        value_span: Some(ByteSpan { start: 52, end: 54 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                }
            ],
            span: None,
        }
    );
}

#[test]
fn test_actor_decl_anonymous() {
    assert_eq!(
        parse_single_stmt("group: Group { Ellipse, size: 10, Rect, size: 20 }"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "group".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix_syntax::ast::InlineItem::Anonymous {
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(10.0),
                        value_span: Some(ByteSpan { start: 30, end: 32 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![]
                },
                animatix_syntax::ast::InlineItem::Anonymous {
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(20.0),
                        value_span: Some(ByteSpan { start: 46, end: 48 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                }
            ],
            span: None,
        }
    );
}

#[test]
fn test_actor_decl_nested_with_children() {
    // Inline items can have both properties and nested children blocks.
    // The `{...}` after properties attaches to the preceding item.
    assert_eq!(
        parse_single_stmt(
            "group: Group { a: Ellipse, size: 10 { child: Text, text: \"hi\" }, b: Rect, size: 20 }"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "group".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix_syntax::ast::InlineItem::Labeled {
                    label: "a".to_string(),
                    array_index: None,
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(10.0),
                        value_span: Some(ByteSpan { start: 33, end: 35 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![animatix_syntax::ast::InlineItem::Labeled {
                        label: "child".to_string(),
                        array_index: None,
                        ty: "Text".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("hi".to_string()),
                            value_span: Some(ByteSpan { start: 57, end: 61 }),
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    }],
                },
                animatix_syntax::ast::InlineItem::Labeled {
                    label: "b".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(20.0),
                        value_span: Some(ByteSpan { start: 80, end: 82 }),
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                }
            ],
            span: None,
        }
    );
}

#[test]
fn test_demo_layout_parse() {
    let ast = parse_program(LAYOUT_FIXTURE);
    assert!(!ast.is_empty());
}

#[test]
fn test_action() {
    assert_eq!(
        parse_single_stmt("fade-out ball [1s]"),
        Stmt::Action(
            Action {
                verb: "fade-out".to_string(),
                targets: vec!["ball".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string())
                }],
                byte_span: Some(ByteSpan { start: 0, end: 18 }),
                target_index: vec![None; 1],
            },
            None
        )
    );
}

#[test]
fn test_swap_action_space_separated_targets() {
    let stmt = parse_single_stmt("swap bar1 bar2 [500ms, ease: ease-in-out]");
    if let Stmt::Action(action, _) = stmt {
        assert_eq!(action.verb, "swap");
        assert_eq!(action.targets, vec!["bar1".to_string(), "bar2".to_string()]);
    } else {
        panic!("Expected Action, got {:?}", stmt);
    }
}

#[test]
fn test_action_comma_separated_targets() {
    let stmt = parse_single_stmt("pulse btn, icon [200ms]");
    if let Stmt::Action(action, _) = stmt {
        assert_eq!(action.verb, "pulse");
        assert_eq!(action.targets, vec!["btn".to_string(), "icon".to_string()]);
    } else {
        panic!("Expected Action, got {:?}", stmt);
    }
}

#[test]
fn test_keyframes() {
    let src = r#"
        #500ms
        let x = 1
        #2.5s
        let y = 2
    "#;
    let ast = parse_program(src);
    assert_eq!(ast.len(), 2);

    if let Stmt::Keyframe { time, body, .. } = &ast[0] {
        assert_eq!(time, &Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }

    if let Stmt::Keyframe { time, body, .. } = &ast[1] {
        assert_eq!(time, &Time::Seconds(2.5));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }
}

#[test]
fn test_relative_keyframes() {
    let src = r#"
        #0s
        let x = 1
        #+500ms
        let y = 2
        #+1.5s
        let z = 3
    "#;
    let ast = parse_program(src);
    assert_eq!(ast.len(), 3);

    if let Stmt::Keyframe { time, body, .. } = &ast[0] {
        assert_eq!(time, &Time::Seconds(0.0));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected absolute Keyframe");
    }

    if let Stmt::RelativeKeyframe { offset, body, .. } = &ast[1] {
        assert_eq!(offset, &Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected RelativeKeyframe");
    }

    if let Stmt::RelativeKeyframe { offset, body, .. } = &ast[2] {
        assert_eq!(offset, &Time::Seconds(1.5));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected RelativeKeyframe");
    }
}

#[test]
fn test_negative_relative_keyframes_rejected() {
    assert!(parse_error("#-1s\nlet x = 1"));
    assert!(parse_error("#+-1s\nlet x = 1"));
}

#[test]
fn test_lifecycle_hook_syntax_rejected() {
    assert!(parse_error("on appear { fade-in btn }"));
    assert!(parse_error("on disappear { fade-out btn }"));
}

#[test]
fn test_on_is_not_reserved_identifier() {
    assert_eq!(
        parse_single_stmt("let on = 1"),
        Stmt::LetDecl {
            is_pub: false,
            name: "on".to_string(),
            value: Expr::Num(1.0),
            span: None,
        }
    );
}

#[test]
fn test_always() {
    let result = parse_single_stmt("always { let x = btn.x }");
    assert_eq!(
        result,
        Stmt::Always {
            body: vec![Stmt::LetDecl {
                is_pub: false,
                name: "x".to_string(),
                value: Expr::Path(vec!["btn".to_string(), "x".to_string()]),
                span: None,
            }],
            span: None,
        }
    );
}

#[test]
fn test_expression_conditional() {
    assert_eq!(
        parse_single_stmt("pulse.size = if active { (120, 120) } else { (180, 180) }"),
        Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "pulse".to_string()
            )],
            property: "size".to_string(),
            value: Expr::Conditional(
                Box::new(Expr::Ident("active".to_string())),
                Box::new(Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(120.0)])),
                Box::new(Expr::Tuple(vec![Expr::Num(180.0), Expr::Num(180.0)])),
            ),
            modifiers: vec![],
            easing: None,
            value_span: Some(ByteSpan { start: 13, end: 57 }),
            span: None,
        }
    );
}

#[test]
fn test_legacy_loop_syntax_rejected() {
    assert!(parse_error("loop { move btn [1s] }"));
    assert!(parse_error("loop 5s { fade btn [1s] }"));
    assert!(parse_error("yield"));
    assert!(parse_error("stop job"));
    assert!(parse_error("pause job"));
    assert!(parse_error("resume job"));
}

#[test]
fn test_conditional() {
    let result = parse_single_stmt("if active { appear btn }");
    assert_eq!(
        result,
        Stmt::Conditional {
            condition: Expr::Ident("active".to_string()),
            then_branch: vec![Stmt::Action(
                Action {
                    verb: "appear".to_string(),
                    targets: vec!["btn".to_string()],
                    args: vec![],
                    modifiers: vec![],
                    byte_span: Some(ByteSpan { start: 12, end: 22 }),
                    target_index: vec![None; 1],
                },
                None
            )],
            else_branch: None,
            span: None,
        }
    );
}

#[test]
fn test_conditional_with_else() {
    let result = parse_single_stmt("if active { fade-in btn } else { fade-out btn }");
    assert_eq!(
        result,
        Stmt::Conditional {
            condition: Expr::Ident("active".to_string()),
            then_branch: vec![Stmt::Action(
                Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["btn".to_string()],
                    args: vec![],
                    modifiers: vec![],
                    byte_span: Some(ByteSpan { start: 12, end: 23 }),
                    target_index: vec![None; 1],
                },
                None
            )],
            else_branch: Some(vec![Stmt::Action(
                Action {
                    verb: "fade-out".to_string(),
                    targets: vec!["btn".to_string()],
                    args: vec![],
                    modifiers: vec![],
                    byte_span: Some(ByteSpan { start: 33, end: 45 }),
                    target_index: vec![None; 1],
                },
                None
            )]),
            span: None,
        }
    );
}

#[test]
fn test_for_loop() {
    let result = parse_single_stmt("for item in buttons { appear item }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Single("item".to_string()),
            index_var: None,
            iterable: Expr::Ident("buttons".to_string()),
            modifiers: vec![],
            body: vec![Stmt::Action(
                Action {
                    verb: "appear".to_string(),
                    targets: vec!["item".to_string()],
                    args: vec![],
                    modifiers: vec![],
                    byte_span: Some(ByteSpan { start: 22, end: 33 }),
                    target_index: vec![None; 1],
                },
                None
            )],
            span: None,
        }
    );
}
