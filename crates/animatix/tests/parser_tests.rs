use animatix::ast::{Action, Expr, LoopKind, Modifier, Property, Stmt, Time, UnaryOp};
use animatix::parser::parser;
use chumsky::Parser;

// Helper function to extract a single statement from the implicit 0s keyframe wrapper
fn parse_single_stmt(src: &str) -> Stmt {
    let ast = parser().parse(src).into_result().unwrap();
    if let Stmt::Keyframe { body, .. } = &ast[0] {
        body[0].clone()
    } else {
        panic!("Expected implicit Keyframe wrapper");
    }
}

#[test]
fn test_let_decl_types() {
    assert_eq!(
        parse_single_stmt("let a = 42"),
        Stmt::LetDecl {
            name: "a".to_string(),
            value: Expr::Num(42.0)
        }
    );
    assert_eq!(
        parse_single_stmt("let b = 3.14"),
        Stmt::LetDecl {
            name: "b".to_string(),
            value: Expr::Num(3.14)
        }
    );
    assert_eq!(
        parse_single_stmt("let c = true"),
        Stmt::LetDecl {
            name: "c".to_string(),
            value: Expr::Bool(true)
        }
    );
    assert_eq!(
        parse_single_stmt("let d = false"),
        Stmt::LetDecl {
            name: "d".to_string(),
            value: Expr::Bool(false)
        }
    );
    assert_eq!(
        parse_single_stmt("let e = null"),
        Stmt::LetDecl {
            name: "e".to_string(),
            value: Expr::Null
        }
    );
    assert_eq!(
        parse_single_stmt("let f = \"string\""),
        Stmt::LetDecl {
            name: "f".to_string(),
            value: Expr::Str("string".to_string())
        }
    );
}

#[test]
fn test_collections() {
    assert_eq!(
        parse_single_stmt("let coords = (10, 20.5)"),
        Stmt::LetDecl {
            name: "coords".to_string(),
            value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)])
        }
    );
    assert_eq!(
        parse_single_stmt("let arr = {a, b}"),
        Stmt::LetDecl {
            name: "arr".to_string(),
            value: Expr::Tuple(vec![
                Expr::Ident("a".to_string()),
                Expr::Ident("b".to_string())
            ])
        }
    );
    assert_eq!(
        parse_single_stmt("let pct = (50%, 25%)"),
        Stmt::LetDecl {
            name: "pct".to_string(),
            value: Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)])
        }
    );
}

#[test]
fn test_assignments_and_paths() {
    assert_eq!(
        parse_single_stmt("btn.color = \"red\""),
        Stmt::Assignment {
            target: "btn".to_string(),
            property: "color".to_string(),
            value: Expr::Str("red".to_string()),
            modifiers: vec![],
        }
    );
    assert_eq!(
        parse_single_stmt("morpher.size = (100, 100) [2s, ease: ease-out]"),
        Stmt::Assignment {
            target: "morpher".to_string(),
            property: "size".to_string(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
            modifiers: vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("2s".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("ease-out".to_string()),
                },
            ],
        }
    );
    assert_eq!(
        parse_single_stmt("let x = container.child"),
        Stmt::LetDecl {
            name: "x".to_string(),
            value: Expr::Path(vec!["container".to_string(), "child".to_string()])
        }
    );
    assert_eq!(
        parse_single_stmt("let center = scene.center"),
        Stmt::LetDecl {
            name: "center".to_string(),
            value: Expr::Path(vec!["scene".to_string(), "center".to_string()])
        }
    );
}

#[test]
fn test_actor_decl_full() {
    assert_eq!(
        parse_single_stmt("circle: Circle, radius: 50, color: blue [2s, ease: bounce]"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "circle".to_string(),
            ty: "Circle".to_string(),
            props: vec![
                Property {
                    name: "radius".to_string(),
                    value: Expr::Num(50.0)
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("blue".to_string())
                }
            ],
            modifiers: vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("2s".to_string())
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string())
                }
            ],
            children: vec![],
        }
    );
}

#[test]
fn test_line_actor_decl() {
    assert_eq!(
        parse_single_stmt("axis: Line, from: (-40, 0), to: (40, 0), stroke: blue, stroke_width: 3"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "axis".to_string(),
            ty: "Line".to_string(),
            props: vec![
                Property {
                    name: "from".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(40.0))),
                        Expr::Num(0.0),
                    ]),
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(0.0)]),
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("blue".to_string()),
                },
                Property {
                    name: "stroke_width".to_string(),
                    value: Expr::Num(3.0),
                }
            ],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_ellipse_actor_decl() {
    assert_eq!(
        parse_single_stmt("halo: Ellipse, radius_x: 80, radius_y: 30, color: green"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "halo".to_string(),
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(30.0),
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("green".to_string()),
                }
            ],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_actor_decl_nested() {
    assert_eq!(
        parse_single_stmt("group: Group { a: Circle, size: 10, b: Rect, size: 20 }"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "group".to_string(),
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "a".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(10.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "b".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(20.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                }
            ],
        }
    );
}

#[test]
fn test_actor_decl_anonymous() {
    assert_eq!(
        parse_single_stmt("group: Group { Circle, size: 10, Rect, size: 20 }"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "group".to_string(),
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Anonymous {
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(10.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Anonymous {
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Num(20.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                }
            ],
        }
    );
}

#[test]
fn test_demo_layout_parse() {
    let src = include_str!("../../../examples/layout_demo.amx");
    let ast = parser().parse(src).into_result().unwrap();
    assert!(!ast.is_empty());
}

#[test]
fn test_action() {
    assert_eq!(
        parse_single_stmt("fade-out ball [1s]"),
        Stmt::Action(Action {
            verb: "fade-out".to_string(),
            targets: vec!["ball".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("1s".to_string())
            }],
        })
    );
}

#[test]
fn test_comments() {
    assert_eq!(
        parse_single_stmt("// This is a comment"),
        Stmt::Comment(" This is a comment".to_string())
    );
}

#[test]
fn test_keyframes() {
    let src = r#"
        #500ms
        let x = 1
        #2.5s
        let y = 2
    "#;
    let ast = parser().parse(src).into_result().unwrap();
    assert_eq!(ast.len(), 2);

    if let Stmt::Keyframe { time, body } = &ast[0] {
        assert_eq!(*time, Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }

    if let Stmt::Keyframe { time, body } = &ast[1] {
        assert_eq!(*time, Time::Seconds(2.5));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }
}

#[test]
fn test_loop_infinite() {
    let result = parse_single_stmt("loop { move btn [1s] }");
    assert_eq!(
        result,
        Stmt::Loop {
            kind: LoopKind::Infinite,
            label: None,
            body: vec![Stmt::Action(Action {
                verb: "move".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            })],
        }
    );
}

#[test]
fn test_loop_bounded_time() {
    let result = parse_single_stmt("loop 5s { fade btn [1s] }");
    assert_eq!(
        result,
        Stmt::Loop {
            kind: LoopKind::Bounded(Time::Seconds(5.0)),
            label: None,
            body: vec![Stmt::Action(Action {
                verb: "fade".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            })],
        }
    );
}

#[test]
fn test_loop_count() {
    let result = parse_single_stmt("loop 3s { shake btn [0.5s] }");
    assert_eq!(
        result,
        Stmt::Loop {
            kind: LoopKind::Bounded(Time::Seconds(3.0)),
            label: None,
            body: vec![Stmt::Action(Action {
                verb: "shake".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("0.5s".to_string()),
                }],
            })],
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
                name: "x".to_string(),
                value: Expr::Path(vec!["btn".to_string(), "x".to_string()]),
            }],
        }
    );
}

#[test]
fn test_labeled_always() {
    let result = parse_single_stmt("reactive: always { btn.color = red }");
    assert_eq!(
        result,
        Stmt::LabeledAlways {
            label: "reactive".to_string(),
            body: vec![Stmt::Assignment {
                target: "btn".to_string(),
                property: "color".to_string(),
                value: Expr::Ident("red".to_string()),
                modifiers: vec![],
            }],
        }
    );
}

#[test]
fn test_labeled_always_simple() {
    let result = parse_single_stmt("reactive: always { let x = 1 }");
    assert_eq!(
        result,
        Stmt::LabeledAlways {
            label: "reactive".to_string(),
            body: vec![Stmt::LetDecl {
                name: "x".to_string(),
                value: Expr::Num(1.0),
            }],
        }
    );
}

#[test]
fn test_conditional() {
    let result = parse_single_stmt("if active { appear btn }");
    assert_eq!(
        result,
        Stmt::Conditional {
            condition: Expr::Ident("active".to_string()),
            then_branch: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
            })],
            else_branch: None,
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
            then_branch: vec![Stmt::Action(Action {
                verb: "fade-in".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
            })],
            else_branch: Some(vec![Stmt::Action(Action {
                verb: "fade-out".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
            })]),
        }
    );
}

#[test]
fn test_for_loop() {
    let result = parse_single_stmt("for item in buttons { appear item }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: "item".to_string(),
            iterable: Expr::Ident("buttons".to_string()),
            body: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["item".to_string()],
                args: vec![],
                modifiers: vec![],
            })],
        }
    );
}

#[test]
fn test_for_loop_with_range() {
    let result = parse_single_stmt("for i in {1, 2, 3} { scale btn [0.1s] }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: "i".to_string(),
            iterable: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0),]),
            body: vec![Stmt::Action(Action {
                verb: "scale".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("0.1s".to_string()),
                }],
            })],
        }
    );
}
