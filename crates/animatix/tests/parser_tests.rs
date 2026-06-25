#![allow(clippy::approx_constant)]

use animatix_syntax::ast::{
    Action, ByteSpan, Expr, InlineItem, LoopPattern, Modifier, Property, Stmt, Time,
    UnaryOp,
};
use animatix_syntax::parser::parse_source;
use animatix::timeline::Timeline as AnimatixTimeline;

// Helper function to extract a single statement.
// Actions, sequences, and staggers at the top level are wrapped in an implicit
// `#0s` keyframe; actor declarations, assignments, let decls, imports, etc.
// remain top-level.
fn parse_single_stmt(src: &str) -> Stmt {
    let (ast, errors) = parse_source(src);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let ast = ast.expect("parsed AST");
    match &ast[0] {
        Stmt::Keyframe { body, .. } => body[0].clone(),
        other => other.clone(),
    }
}

fn parse_program(src: &str) -> Vec<Stmt> {
    let (ast, errors) = parse_source(src);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    ast.expect("parsed AST")
}

fn parse_error(src: &str) -> bool {
    let (_, errors) = parse_source(src);
    !errors.is_empty()
}

const LAYOUT_FIXTURE: &str = r#"// Layout: Grid, Col, Row, Stack, anchors, offsets, nesting.

config { colorscheme: "editorial-dark" }

#0s
dashboard: Grid, cols: 2, gap: 28, anchor: scene.center, offset: (0, -40) {
  p1: Rect, size: (200, 100), color: surface.primary
  p2: Rect, size: (200, 100), color: surface.secondary
  p3: Rect, size: (200, 100), color: surface.primary
  p4: Rect, size: (200, 100), color: surface.secondary
}

sidebar: Col, anchor: scene.left, offset: (140, 0), gap: 16, align: "center" {
  a: Rect, size: (120, 50), color: accent.primary
  b: Rect, size: (120, 50), color: accent.success
  c: Rect, size: (120, 50), color: accent.warning
}

toolbar: Row, anchor: scene.top, offset: (0, 100), gap: 20, align: "center" {
  t1: Ellipse, size: (36, 36), color: accent.danger
  t2: Ellipse, size: (36, 36), color: accent.primary
  t3: Ellipse, size: (36, 36), color: accent.success
}

overlay: Stack, anchor: (82%, 76%) {
  plate: Rect, size: (260, 100), color: (0.10, 0.14, 0.22, 1.0)
  ring: Ellipse, size: (56, 56), color: accent.warning
  core: Ellipse, size: (28, 28), color: text.primary
}

// Nested layout with a manual child override
mixed: Row, anchor: scene.bottom, offset: (0, -100), gap: 24, align: "center" {
  focus: Ellipse, size: (32, 32), color: text.primary, at: (-80, -20)
  x: Ellipse, size: (56, 56), color: accent.danger
  y: Ellipse, size: (44, 44), color: accent.success
  z: Ellipse, size: (64, 64), color: accent.primary
}
"#;

#[test]
fn test_let_decl_types() {
    assert_eq!(
        parse_single_stmt("let a = 42"),
        Stmt::LetDecl { is_pub: false,
            name: "a".to_string(),
            value: Expr::Num(42.0),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let b = 3.14"),
        Stmt::LetDecl { is_pub: false,
            name: "b".to_string(),
            value: Expr::Num(3.14),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let c = true"),
        Stmt::LetDecl { is_pub: false,
            name: "c".to_string(),
            value: Expr::Bool(true),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let d = false"),
        Stmt::LetDecl { is_pub: false,
            name: "d".to_string(),
            value: Expr::Bool(false),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let e = null"),
        Stmt::LetDecl { is_pub: false,
            name: "e".to_string(),
            value: Expr::Null,
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let f = \"string\""),
        Stmt::LetDecl { is_pub: false,
            name: "f".to_string(),
            value: Expr::Str("string".to_string()),
            span: None,
        }
    );
}

#[test]
fn test_collections() {
    assert_eq!(
        parse_single_stmt("let coords = (10, 20.5)"),
        Stmt::LetDecl { is_pub: false,
            name: "coords".to_string(),
            value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let arr = {a, b}"),
        Stmt::LetDecl { is_pub: false,
            name: "arr".to_string(),
            value: Expr::List(vec![
                Expr::Ident("a".to_string()),
                Expr::Ident("b".to_string())
            ]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let pct = (50%, 25%)"),
        Stmt::LetDecl { is_pub: false,
            name: "pct".to_string(),
            value: Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)]),
            span: None,
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
            easing: None,
            value_span: Some(ByteSpan { start: 12, end: 17 }),
            span: None,
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
            ],
            easing: Some(animatix_syntax::easing::Easing::EaseOut),
            value_span: Some(ByteSpan { start: 15, end: 26 }),
            span: None,
        }
    );

    // Easing extraction: linear
    assert_eq!(
        parse_single_stmt("btn.at = (100, 100) [ease: linear]"),
        Stmt::Assignment {
            target: vec!["btn".to_string()],
            property: "at".to_string(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
            modifiers: vec![],
            easing: Some(animatix_syntax::easing::Easing::Linear),
            value_span: Some(ByteSpan { start: 9, end: 20 }),
            span: None,
        }
    );
    // Easing extraction: bounce
    assert_eq!(
        parse_single_stmt("btn.opacity = 0.5 [ease: bounce]"),
        Stmt::Assignment {
            target: vec!["btn".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(0.5),
            modifiers: vec![],
            easing: Some(animatix_syntax::easing::Easing::Bounce),
            value_span: Some(ByteSpan { start: 14, end: 18 }),
            span: None,
        }
    );
    // No easing
    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        Stmt::Assignment {
            target: vec!["left".to_string(), "badge".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("red".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: Some(ByteSpan { start: 19, end: 22 }),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let x = container.child"),
        Stmt::LetDecl { is_pub: false,
            name: "x".to_string(),
            value: Expr::Path(vec!["container".to_string(), "child".to_string()]),
            span: None,
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
            ]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let center = scene.center"),
        Stmt::LetDecl { is_pub: false,
            name: "center".to_string(),
            value: Expr::Path(vec!["scene".to_string(), "center".to_string()]),
            span: None,
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
                    byte_span: Some(ByteSpan { start: 11, end: 33 }),
                }, None),
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
                    easing: None,
                    value_span: Some(ByteSpan { start: 47, end: 51 }),
            span: None,
                },
            ],
            span: None,
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
                    byte_span: Some(ByteSpan { start: 18, end: 40 }),
                }, None),
                Stmt::Assignment {
                    target: vec!["second".to_string()],
                    property: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("100ms".to_string()),
                    }],
                    easing: None,
                    value_span: Some(ByteSpan { start: 55, end: 59 }),
            span: None,
                },
            ],
            span: None,
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
                byte_span: Some(ByteSpan { start: 24, end: 46 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_actor_decl_full() {
    assert_eq!(
        parse_single_stmt("circle: Ellipse, radius: 50, color: blue [2s, ease: bounce]"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "circle".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius".to_string(),
                    value: Expr::Num(50.0),
                    value_span: Some(ByteSpan { start: 25, end: 27 }),
                trailing_comment: None,
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("blue".to_string()),
                    value_span: Some(ByteSpan { start: 36, end: 41 }),
                trailing_comment: None,
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
            span: None,
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
                value_span: Some(ByteSpan { start: 22, end: 39 }),
            trailing_comment: None,
            }],
            span: None,
        }
    );
}

#[test]
fn test_qualified_play_parse() {
    match &parse_program("play module.SceneName [fade, 300ms]")[0] {
        Stmt::Play { scene_name, .. } => assert_eq!(scene_name, "module.SceneName"),
        other => panic!("Expected play statement, got {:?}", other),
    }
}

#[test]
fn test_actor_decl_colorscheme_alias_parse() {
    assert_eq!(
        parse_single_stmt("badge: Ellipse, color: auto, stroke: stroke.default"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "badge".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                    value_span: Some(ByteSpan { start: 23, end: 27 }),
                trailing_comment: None,
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Path(vec!["stroke".to_string(), "default".to_string()]),
                    value_span: Some(ByteSpan { start: 37, end: 51 }),
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
fn test_text_colorscheme_alias_parse() {
    assert_eq!(
        parse_single_stmt("title: Text, text: \"Animatix\", color: text.primary"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "title".to_string(),
            array_index: None,
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Animatix".to_string()),
                    value_span: Some(ByteSpan { start: 19, end: 29 }),
                trailing_comment: None,
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["text".to_string(), "primary".to_string()]),
                    value_span: Some(ByteSpan { start: 38, end: 50 }),
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
fn test_modifier_delay_and_duplicates_parse() {
    assert_eq!(
        parse_single_stmt("badge: Ellipse, radius: 20 [delay: 250ms, ease: ease-in, ease: bounce]"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "badge".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(20.0),
                value_span: Some(ByteSpan { start: 24, end: 27 }),
            trailing_comment: None,
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
            span: None,
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
            easing: None,
            value_span: Some(ByteSpan { start: 15, end: 18 }),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        Stmt::Assignment {
            target: vec!["left".to_string(), "badge".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("red".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: Some(ByteSpan { start: 19, end: 22 }),
            span: None,
        }
    );

    assert_eq!(
        parse_single_stmt("card: MetricCard, title: \"Latency\""),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "card".to_string(),
            array_index: None,
            ty: "MetricCard".to_string(),
            props: vec![Property {
                name: "title".to_string(),
                value: Expr::Str("Latency".to_string()),
                value_span: Some(ByteSpan { start: 25, end: 34 }),
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    );
}

#[test]
fn test_code_stmt_parse() {
    assert_eq!(
        parse_single_stmt("snippet: Code, code: \"fn main() {}\", font_size: 18"),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "snippet".to_string(),
            array_index: None,
            ty: "Code".to_string(),
            props: vec![
                Property {
                    name: "code".to_string(),
                    value: Expr::Str("fn main() {}".to_string()),
                    value_span: Some(ByteSpan { start: 21, end: 35 }),
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(18.0),
                    value_span: Some(ByteSpan { start: 48, end: 50 }),
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
fn test_image_stmt() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.png\", at: (100, 120), size: (240, 180) }"
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
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(120.0)]),
                    value_span: Some(ByteSpan { start: 48, end: 58 }),
                    trailing_comment: None,
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(180.0)]),
                    value_span: Some(ByteSpan { start: 66, end: 77 }),
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
                    value_span: Some(ByteSpan { start: 83, end: 87 }),
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
                    value_span: Some(ByteSpan { start: 90, end: 101 }),
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
                    value_span: Some(ByteSpan { start: 23, end: 107 }),
                trailing_comment: None,
                },
                Property {
                    name: "stroke".to_string(),
                    value: Expr::Ident("white".to_string()),
                    value_span: Some(ByteSpan { start: 117, end: 122 }),
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
                        value_span: Some(ByteSpan { start: 52, end: 55 }),
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
                        value_span: Some(ByteSpan { start: 46, end: 49 }),
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
                        value_span: Some(ByteSpan { start: 33, end: 36 }),
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
                            value_span: Some(ByteSpan { start: 57, end: 62 }),
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
                        value_span: Some(ByteSpan { start: 80, end: 83 }),
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
        Stmt::Action(Action {
            verb: "fade-out".to_string(),
            targets: vec!["ball".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("1s".to_string())
            }],
            byte_span: Some(ByteSpan { start: 0, end: 18 }),
        }, None)
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
        Stmt::LetDecl { is_pub: false,
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
            body: vec![Stmt::LetDecl { is_pub: false,
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
            target: vec!["pulse".to_string()],
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
            then_branch: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 12, end: 23 }),
            }, None)],
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
            then_branch: vec![Stmt::Action(Action {
                verb: "fade-in".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 12, end: 24 }),
            }, None)],
            else_branch: Some(vec![Stmt::Action(Action {
                verb: "fade-out".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 33, end: 46 }),
            }, None)]),
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
            body: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["item".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 22, end: 34 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_with_range() {
    let result = parse_single_stmt("for i in {1, 2, 3} { scale btn [0.1s] }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Single("i".to_string()),
            index_var: None,
            iterable: Expr::List(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0),]),
            body: vec![Stmt::Action(Action {
                verb: "scale".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("0.1s".to_string()),
                }],
                byte_span: Some(ByteSpan { start: 21, end: 38 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_destructuring_basic() {
    let result = parse_single_stmt("for (x, y) in points { appear dot }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]),
            index_var: None,
            iterable: Expr::Ident("points".to_string()),
            body: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["dot".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 23, end: 34 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_three_elements() {
    let result = parse_single_stmt("for (r, g, b) in colors { fade-out fade }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec![
                "r".to_string(),
                "g".to_string(),
                "b".to_string()
            ]),
            index_var: None,
            iterable: Expr::Ident("colors".to_string()),
            body: vec![Stmt::Action(Action {
                verb: "fade-out".to_string(),
                targets: vec!["fade".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 26, end: 40 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_with_index() {
    let result = parse_single_stmt("for (a, b), i in items { appear dot }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["a".to_string(), "b".to_string()]),
            index_var: Some("i".to_string()),
            iterable: Expr::Ident("items".to_string()),
            body: vec![Stmt::Action(Action {
                verb: "appear".to_string(),
                targets: vec!["dot".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: Some(ByteSpan { start: 25, end: 36 }),
            }, None)],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_destructuring_in_inline() {
    let result = parse_single_stmt(
        "group: Group { for (x, y) in pts { dot: Rect, at: (x, y) } }"
    );
    if let Stmt::ActorDecl { label, ty, children, .. } = result {
        assert_eq!(label, "group");
        assert_eq!(ty, "Group");
        assert_eq!(children.len(), 1);
        match &children[0] {
            InlineItem::ForLoop { var, index_var, iterable, body } => {
                assert_eq!(var, &LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
                assert_eq!(index_var, &None);
                assert_eq!(iterable, &Expr::Ident("pts".to_string()));
                assert_eq!(body.len(), 1);
                match &body[0] {
                    InlineItem::Labeled { label, ty, props, .. } => {
                        assert_eq!(label, "dot");
                        assert_eq!(ty, "Rect");
                        assert_eq!(props.len(), 1);
                        assert_eq!(props[0].name, "at");
                    }
                    other => panic!("Expected Labeled inline item, got {:?}", other),
                }
            }
            other => panic!("Expected InlineItem::ForLoop, got {:?}", other),
        }
    } else {
        panic!("Expected ActorDecl, got {:?}", result);
    }
}

#[test]
fn test_for_loop_tuple_inline_with_index() {
    let result = parse_single_stmt(
        "group: Group { for (x, y), i in pts { dot[i]: Rect, at: (x, y) } }"
    );
    if let Stmt::ActorDecl { label, ty, children, .. } = result {
        assert_eq!(label, "group");
        assert_eq!(ty, "Group");
        assert_eq!(children.len(), 1);
        match &children[0] {
            InlineItem::ForLoop { var, index_var, iterable, body } => {
                assert_eq!(var, &LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
                assert_eq!(index_var, &Some("i".to_string()));
                assert_eq!(iterable, &Expr::Ident("pts".to_string()));
                assert_eq!(body.len(), 1);
            }
            other => panic!("Expected InlineItem::ForLoop, got {:?}", other),
        }
    } else {
        panic!("Expected ActorDecl, got {:?}", result);
    }
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
            span: None,
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
            span: None,
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
            span: None,
        }
    );
}

#[test]
fn test_slot_marker_in_container() {
    let src = r#"header: Col {
  @slot
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "header");
        assert_eq!(ty, "Col");
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], InlineItem::SlotMarker));
    } else {
        panic!("Expected ActorDecl, got {:?}", stmt);
    }
}

#[test]
fn test_slot_marker_with_defaults_in_container() {
    let src = r#"footer: Col {
  @slot
  Text, text: "Default"
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "footer");
        assert_eq!(ty, "Col");
        assert_eq!(children.len(), 2);
        assert!(matches!(children[0], InlineItem::SlotMarker));
        match &children[1] {
            InlineItem::Anonymous { ty, props, .. } => {
                assert_eq!(ty, "Text");
                assert!(!props.is_empty());
            }
            _ => panic!("Expected Anonymous item, got {:?}", children[1]),
        }
    } else {
        panic!("Expected ActorDecl, got {:?}", stmt);
    }
}

#[test]
fn test_slot_fill_parsing() {
    let src = r#"slide: SlideLayout {
  @header {
    title: Text, text: "Hello"
  }
  @body {
    badge: Ellipse, radius: 20
  }
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "slide");
        assert_eq!(ty, "SlideLayout");
        assert_eq!(children.len(), 2);

        match &children[0] {
            InlineItem::SlotFill { slot, items } => {
                assert_eq!(slot, "header");
                assert_eq!(items.len(), 1);
                match &items[0] {
                    InlineItem::Labeled { label, ty, .. } => {
                        assert_eq!(label, "title");
                        assert_eq!(ty, "Text");
                    }
                    _ => panic!("Expected Labeled item in slot fill"),
                }
            }
            _ => panic!("Expected SlotFill, got {:?}", children[0]),
        }

        match &children[1] {
            InlineItem::SlotFill { slot, items } => {
                assert_eq!(slot, "body");
                assert_eq!(items.len(), 1);
            }
            _ => panic!("Expected SlotFill, got {:?}", children[1]),
        }
    } else {
        panic!("Expected ActorDecl, got {:?}", stmt);
    }
}

#[test]
fn test_mixed_slot_fill_parsing() {
    // @slot itself should also parse as a slot fill when used as @slot { items }
    let src = r#"mycomp: MyComponent {
  @slot {
    Text, text: "Content"
  }
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { children, .. } = stmt {
        assert_eq!(children.len(), 1);
        match &children[0] {
            InlineItem::SlotFill { slot, items } => {
                assert_eq!(slot, "slot");
                assert_eq!(items.len(), 1);
            }
            _ => panic!("Expected SlotFill"),
        }
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_empty_slot_fill() {
    // Tests parsing a slot fill with empty body @slotname { }
    let src = r#"modal: Dialog {
  @slot { }
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "modal");
        assert_eq!(ty, "Dialog");
        assert_eq!(children.len(), 1);
        match &children[0] {
            InlineItem::SlotFill { slot, items } => {
                assert_eq!(slot, "slot");
                assert!(items.is_empty());
            }
            _ => panic!("Expected SlotFill"),
        }
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_slot_fill_with_multiple_items() {
    // Tests a slot fill containing multiple items
    let src = r#"header: Header {
  @title {
    Text, text: "Welcome"
    Text, text: "Subtitle"
  }
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "header");
        assert_eq!(ty, "Header");
        assert_eq!(children.len(), 1);
        match &children[0] {
            InlineItem::SlotFill { slot, items } => {
                assert_eq!(slot, "title");
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected SlotFill"),
        }
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_slot_marker_as_only_child() {
    // Tests @slot as the only item in the container (no defaults)
    let src = r#"sidebar: Sidebar {
  @slot
}"#;
    let stmt = parse_single_stmt(src);
    if let Stmt::ActorDecl { label, ty, children, .. } = stmt {
        assert_eq!(label, "sidebar");
        assert_eq!(ty, "Sidebar");
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], InlineItem::SlotMarker));
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_at_slot_as_slot_fill() {
    // When @slot is followed by { }, it's parsed as a slot fill with name "slot"
    // This is a valid-but-weird case
    let src = r#"comp: Component {
  @slot {
    Text, text: "Inline content"
  }
}"#;
    let stmt = parse_single_stmt(src);
    // Just ensure it parses without error
    if let Stmt::ActorDecl { children, .. } = stmt {
        assert!(!children.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Gap 1b integration: array-indexed targets build correctly
// ---------------------------------------------------------------------------

#[test]
fn test_indexed_target_build_property_assignment() {
    // dots__0 is declared by `dots[0]: Rect` inside a keyframe.
    // `dots[0].opacity = 1` (outside keyframe, processed at t=0) sets a
    // keyframe on the dots__0 track, verifying the indexed target resolves.
    let src = r#"
#0s
dots__0: Rect, at: (0,0), size: (10,10)
dots[0].opacity = 1
"#;
    let (ast, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let ast = ast.unwrap();
    let timeline = AnimatixTimeline::build(&ast);
    assert!(
        timeline.tracks().contains_key("dots__0"),
        "track dots__0 should exist; tracks: {:?}",
        timeline.tracks().keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_indexed_target_build_action() {
    // dots__1 is declared as a direct track.
    // `fade-in dots[1] [300ms]` should generate an action event targeting `dots__1`.
    let src = r#"
#0s
dots__1: Rect, at: (0,0), size: (10,10)
fade-in dots[1] [300ms]
"#;
    let (ast, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let ast = ast.unwrap();
    let timeline = AnimatixTimeline::build(&ast);
    let has_target = timeline
        .action_events
        .iter()
        .any(|e| e.targets.iter().any(|t| t == "dots__1"));
    assert!(has_target, "expected action event targeting dots__1; events: {:?}", timeline.action_events);
}
