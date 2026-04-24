use animatix::ast::{
    Action, ComponentDef, Expr, Modifier, ParamDef, Property, Stmt, Time, UnaryOp,
};
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

fn parse_program(src: &str) -> Vec<Stmt> {
    parser().parse(src).into_result().unwrap()
}

fn parse_error(src: &str) -> bool {
    parser().parse(src).into_result().is_err()
}

#[test]
fn test_let_decl_types() {
    assert_eq!(
        parse_single_stmt("let a = 42"),
        Stmt::LetDecl { is_pub: false,
            name: "a".to_string(),
            value: Expr::Num(42.0)
        }
    );
    assert_eq!(
        parse_single_stmt("let b = 3.14"),
        Stmt::LetDecl { is_pub: false,
            name: "b".to_string(),
            value: Expr::Num(3.14)
        }
    );
    assert_eq!(
        parse_single_stmt("let c = true"),
        Stmt::LetDecl { is_pub: false,
            name: "c".to_string(),
            value: Expr::Bool(true)
        }
    );
    assert_eq!(
        parse_single_stmt("let d = false"),
        Stmt::LetDecl { is_pub: false,
            name: "d".to_string(),
            value: Expr::Bool(false)
        }
    );
    assert_eq!(
        parse_single_stmt("let e = null"),
        Stmt::LetDecl { is_pub: false,
            name: "e".to_string(),
            value: Expr::Null
        }
    );
    assert_eq!(
        parse_single_stmt("let f = \"string\""),
        Stmt::LetDecl { is_pub: false,
            name: "f".to_string(),
            value: Expr::Str("string".to_string())
        }
    );
}

#[test]
fn test_collections() {
    assert_eq!(
        parse_single_stmt("let coords = (10, 20.5)"),
        Stmt::LetDecl { is_pub: false,
            name: "coords".to_string(),
            value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)])
        }
    );
    assert_eq!(
        parse_single_stmt("let arr = {a, b}"),
        Stmt::LetDecl { is_pub: false,
            name: "arr".to_string(),
            value: Expr::Tuple(vec![
                Expr::Ident("a".to_string()),
                Expr::Ident("b".to_string())
            ])
        }
    );
    assert_eq!(
        parse_single_stmt("let pct = (50%, 25%)"),
        Stmt::LetDecl { is_pub: false,
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
            target: vec!["btn".to_string()],
            property: "color".to_string(),
            value: Expr::Str("red".to_string()),
            modifiers: vec![],
        }
    );
    assert_eq!(
        parse_single_stmt("morpher.size = (100, 100) [2s, ease: ease-out]"),
        Stmt::Assignment {
            target: vec!["morpher".to_string()],
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
        parse_single_stmt("left.badge.color = red"),
        Stmt::Assignment {
            target: vec!["left".to_string(), "badge".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("red".to_string()),
            modifiers: vec![],
        }
    );
    assert_eq!(
        parse_single_stmt("let x = container.child"),
        Stmt::LetDecl { is_pub: false,
            name: "x".to_string(),
            value: Expr::Path(vec!["container".to_string(), "child".to_string()])
        }
    );
    assert_eq!(
        parse_single_stmt("let fill = left.badge.color"),
        Stmt::LetDecl { is_pub: false,
            name: "fill".to_string(),
            value: Expr::Path(vec![
                "left".to_string(),
                "badge".to_string(),
                "color".to_string()
            ])
        }
    );
    assert_eq!(
        parse_single_stmt("let center = scene.center"),
        Stmt::LetDecl { is_pub: false,
            name: "center".to_string(),
            value: Expr::Path(vec!["scene".to_string(), "center".to_string()])
        }
    );
}

#[test]
fn test_sequence_parse() {
    assert_eq!(
        parse_single_stmt(
            "sequence { fade-in badge [500ms] badge.color = red [250ms, delay: 100ms] }"
        ),
        Stmt::Sequence {
            body: vec![
                Stmt::Action(Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["badge".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("500ms".to_string()),
                    }],
                }),
                Stmt::Assignment {
                    target: vec!["badge".to_string()],
                    property: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    modifiers: vec![
                        Modifier {
                            name: None,
                            value: Expr::Ident("250ms".to_string()),
                        },
                        Modifier {
                            name: Some("delay".to_string()),
                            value: Expr::Ident("100ms".to_string()),
                        },
                    ],
                },
            ],
        }
    );
}

#[test]
fn test_stagger_parse() {
    assert_eq!(
        parse_single_stmt("stagger [150ms] { fade-in first [200ms] second.color = red [100ms] }"),
        Stmt::Stagger {
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("150ms".to_string()),
            }],
            body: vec![
                Stmt::Action(Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["first".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("200ms".to_string()),
                    }],
                }),
                Stmt::Assignment {
                    target: vec!["second".to_string()],
                    property: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("100ms".to_string()),
                    }],
                },
            ],
        }
    );
}

#[test]
fn test_stagger_each_parse() {
    assert_eq!(
        parse_single_stmt("stagger [each: 150ms] { fade-in first [200ms] }"),
        Stmt::Stagger {
            modifiers: vec![Modifier {
                name: Some("each".to_string()),
                value: Expr::Ident("150ms".to_string()),
            }],
            body: vec![Stmt::Action(Action {
                verb: "fade-in".to_string(),
                targets: vec!["first".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("200ms".to_string()),
                }],
            })],
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
fn test_config_parse() {
    assert_eq!(
        parse_program("config { colorscheme: \"editorial-dark\" }")[0],
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        }
    );
}

#[test]
fn test_actor_decl_colorscheme_alias_parse() {
    assert_eq!(
        parse_single_stmt("badge: Circle, color: auto, stroke: stroke.default"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Circle".to_string(),
            props: vec![
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Path(vec!["stroke".to_string(), "default".to_string()]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_text_colorscheme_alias_parse() {
    assert_eq!(
        parse_single_stmt("title: Text, text: \"Animatix\", color: text.primary"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "title".to_string(),
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Animatix".to_string()),
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["text".to_string(), "primary".to_string()]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_modifier_delay_and_duplicates_parse() {
    assert_eq!(
        parse_single_stmt("badge: Circle, radius: 20 [delay: 250ms, ease: ease-in, ease: bounce]"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Circle".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![
                Modifier {
                    name: Some("delay".to_string()),
                    value: Expr::Ident("250ms".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("ease-in".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string()),
                },
            ],
            children: vec![],
        }
    );

    assert_eq!(
        parse_single_stmt("badge.radius = 40 [delay: 1s, 500ms]"),
        Stmt::Assignment {
            target: vec!["badge".to_string()],
            property: "radius".to_string(),
            value: Expr::Num(40.0),
            modifiers: vec![
                Modifier {
                    name: Some("delay".to_string()),
                    value: Expr::Ident("1s".to_string()),
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("500ms".to_string()),
                },
            ],
        }
    );
}

#[test]
fn test_morph_modifier_keys_parse() {
    assert_eq!(
        parse_single_stmt(
            "badge: Circle, radius: 20 [1s, strategy: match, path_arc: 1.57, stretch: false]"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Circle".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
                Modifier {
                    name: Some("strategy".to_string()),
                    value: Expr::Ident("match".to_string()),
                },
                Modifier {
                    name: Some("path_arc".to_string()),
                    value: Expr::Num(1.57),
                },
                Modifier {
                    name: Some("stretch".to_string()),
                    value: Expr::Bool(false),
                },
            ],
            children: vec![],
        }
    );
}

#[test]
fn test_component_definition_and_instantiation_parse() {
    assert_eq!(
        parse_single_stmt(
            "pub component MetricCard(title: \"Throughput\") { label: Text, text: title }"
        ),
        Stmt::ComponentDef(ComponentDef {
            is_pub: true,
            name: "MetricCard".to_string(),
            params: vec![ParamDef {
                name: "title".to_string(),
                param_type: None,
                default: Some(Expr::Str("Throughput".to_string())),
            }],
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "label".to_string(),
                ty: "Text".to_string(),
                props: vec![Property {
                    name: "text".to_string(),
                    value: Expr::Ident("title".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        })
    );

    assert_eq!(
        parse_single_stmt("card: MetricCard, title: \"Latency\""),
        Stmt::ActorDecl {
            is_pub: false,
            label: "card".to_string(),
            ty: "MetricCard".to_string(),
            props: vec![Property {
                name: "title".to_string(),
                value: Expr::Str("Latency".to_string()),
            }],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_code_stmt_parse() {
    assert_eq!(
        parse_single_stmt("snippet: Code, code: \"fn main() {}\", font_size: 18"),
        Stmt::ActorDecl {
            is_pub: false,
            label: "snippet".to_string(),
            ty: "Code".to_string(),
            props: vec![
                Property {
                    name: "code".to_string(),
                    value: Expr::Str("fn main() {}".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(18.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }
    );
}

#[test]
fn test_image_stmt() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.ppm\", at: (100, 120), size: (240, 180) }"
        ),
        Stmt::Image {
            label: Some("photo".to_string()),
            url: "examples/checker.ppm".to_string(),
            at: Some(Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(120.0)])),
            anchor: None,
            offset: None,
            size: Some((240.0, 180.0)),
        }
    );
}

#[test]
fn test_svg_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "icon: Svg { url: \"examples/vector.svg\", anchor: scene.top, offset: (0, 24), scale: 1.5 }"
        ),
        Stmt::Svg {
            label: Some("icon".to_string()),
            url: "examples/vector.svg".to_string(),
            at: None,
            anchor: Some(Expr::Path(vec!["scene".to_string(), "top".to_string()])),
            offset: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(24.0)])),
            scale: 1.5,
        }
    );
}

#[test]
fn test_image_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.ppm\", anchor: scene.bottom, offset: (0, -40), size: (240, 180) }"
        ),
        Stmt::Image {
            label: Some("photo".to_string()),
            url: "examples/checker.ppm".to_string(),
            at: None,
            anchor: Some(Expr::Path(vec!["scene".to_string(), "bottom".to_string()])),
            offset: Some(Expr::Tuple(vec![
                Expr::Num(0.0),
                Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(40.0))),
            ])),
            size: Some((240.0, 180.0)),
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
fn test_arc_actor_decl() {
    assert_eq!(
        parse_single_stmt(
            "ring: Arc, radius_x: 80, radius_y: 50, start_angle: 0, sweep_angle: 3.14, stroke: gold"
        ),
        Stmt::ActorDecl {
            is_pub: false,
            label: "ring".to_string(),
            ty: "Arc".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(50.0),
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(0.0),
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(3.14),
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("gold".to_string()),
                }
            ],
            modifiers: vec![],
            children: vec![],
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
            label: "badge".to_string(),
            ty: "Polygon".to_string(),
            props: vec![
                Property {
                    name: "points".to_string(),
                    value: Expr::Tuple(vec![
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
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("cyan".to_string()),
                }
            ],
            modifiers: vec![],
            children: vec![],
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
            label: "guide".to_string(),
            ty: "Path".to_string(),
            props: vec![
                Property {
                    name: "commands".to_string(),
                    value: Expr::Tuple(vec![
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
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("white".to_string()),
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
fn test_actor_decl_nested_with_children() {
    // Inline items can have both properties and nested children blocks.
    // The `{...}` after properties attaches to the preceding item.
    assert_eq!(
        parse_single_stmt(
            "group: Group { a: Circle, size: 10 { child: Text, text: \"hi\" }, b: Rect, size: 20 }"
        ),
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
                    children: vec![animatix::ast::InlineItem::Labeled {
                        label: "child".to_string(),
                        ty: "Text".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("hi".to_string()),
                        }],
                        modifiers: vec![],
                        children: vec![],
                    }],
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
fn test_demo_layout_parse() {
    let src = include_str!("../../../examples/layout.amx");
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
fn test_relative_keyframes() {
    let src = r#"
        #0s
        let x = 1
        #+500ms
        let y = 2
        #+1.5s
        let z = 3
    "#;
    let ast = parser().parse(src).into_result().unwrap();
    assert_eq!(ast.len(), 3);

    if let Stmt::Keyframe { time, body } = &ast[0] {
        assert_eq!(*time, Time::Seconds(0.0));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected absolute Keyframe");
    }

    if let Stmt::RelativeKeyframe { offset, body } = &ast[1] {
        assert_eq!(*offset, Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected RelativeKeyframe");
    }

    if let Stmt::RelativeKeyframe { offset, body } = &ast[2] {
        assert_eq!(*offset, Time::Seconds(1.5));
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
        Stmt::LetDecl { is_pub: false,
            name: "on".to_string(),
            value: Expr::Num(1.0),
        }
    );
}

#[test]
fn test_always() {
    let result = parse_single_stmt("always { let x = btn.x }");
    assert_eq!(
        result,
        Stmt::Always {
            body: vec![Stmt::LetDecl { is_pub: false,
                name: "x".to_string(),
                value: Expr::Path(vec!["btn".to_string(), "x".to_string()]),
            }],
        }
    );
}

#[test]
fn test_expression_conditional() {
    assert_eq!(
        parse_single_stmt("pulse.size = if active { (120, 120) } else { (180, 180) }"),
        Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "size".to_string(),
            value: Expr::Conditional(
                Box::new(Expr::Ident("active".to_string())),
                Box::new(Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(120.0)])),
                Box::new(Expr::Tuple(vec![Expr::Num(180.0), Expr::Num(180.0)])),
            ),
            modifiers: vec![],
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
fn test_labeled_always() {
    let result = parse_single_stmt("reactive: always { btn.color = red }");
    assert_eq!(
        result,
        Stmt::LabeledAlways {
            label: "reactive".to_string(),
            body: vec![Stmt::Assignment {
                target: vec!["btn".to_string()],
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
            body: vec![Stmt::LetDecl { is_pub: false,
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

#[test]
fn test_pub_let_declaration() {
    let result = parse_single_stmt("pub let pi = 3.14");
    assert_eq!(
        result,
        Stmt::LetDecl {
            is_pub: true,
            name: "pi".to_string(),
            value: Expr::Num(3.14),
        }
    );
}

#[test]
fn test_let_declaration_without_pub() {
    let result = parse_single_stmt("let x = 42");
    assert_eq!(
        result,
        Stmt::LetDecl {
            is_pub: false,
            name: "x".to_string(),
            value: Expr::Num(42.0),
        }
    );
}

#[test]
fn test_import_with_alias() {
    let result = parse_single_stmt(r#"import "theme.amx" as theme"#);
    assert_eq!(
        result,
        Stmt::Import {
            path: "theme.amx".to_string(),
            alias: Some("theme".to_string()),
        }
    );
}

#[test]
fn test_import_without_alias() {
    let result = parse_single_stmt(r#"import "shared.amx""#);
    assert_eq!(
        result,
        Stmt::Import {
            path: "shared.amx".to_string(),
            alias: None,
        }
    );
}
