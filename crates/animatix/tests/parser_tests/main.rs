#![allow(clippy::approx_constant)]

use animatix::timeline::eval_shared::eval_builtin_fn;
use animatix::timeline::{Timeline as AnimatixTimeline, Value};
use animatix_syntax::ast::{
    Action, ByteSpan, Expr, InlineItem, LoopPattern, MatchPattern, Modifier, Property, Stmt, Time,
    UnaryOp,
};
use animatix_syntax::parser::parse_source;

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
        Stmt::LetDecl {
            is_pub: false,
            name: "a".to_string(),
            value: Expr::Num(42.0),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let b = 3.14"),
        Stmt::LetDecl {
            is_pub: false,
            name: "b".to_string(),
            value: Expr::Num(3.14),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let c = true"),
        Stmt::LetDecl {
            is_pub: false,
            name: "c".to_string(),
            value: Expr::Bool(true),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let d = false"),
        Stmt::LetDecl {
            is_pub: false,
            name: "d".to_string(),
            value: Expr::Bool(false),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let e = null"),
        Stmt::LetDecl {
            is_pub: false,
            name: "e".to_string(),
            value: Expr::Null,
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let f = \"string\""),
        Stmt::LetDecl {
            is_pub: false,
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
        Stmt::LetDecl {
            is_pub: false,
            name: "coords".to_string(),
            value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let arr = {a, b}"),
        Stmt::LetDecl {
            is_pub: false,
            name: "arr".to_string(),
            value: Expr::List(vec![Expr::Ident("a".to_string()), Expr::Ident("b".to_string())]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let pct = (50%, 25%)"),
        Stmt::LetDecl {
            is_pub: false,
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
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "btn".to_string()
            )],
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
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "morpher".to_string()
            )],
            property: "size".to_string(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("2s".to_string()),
            },],
            easing: Some(animatix_syntax::easing::Easing::EaseOut),
            value_span: Some(ByteSpan { start: 15, end: 25 }),
            span: None,
        }
    );

    // Easing extraction: linear
    assert_eq!(
        parse_single_stmt("btn.at = (100, 100) [ease: linear]"),
        Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "btn".to_string()
            )],
            property: "at".to_string(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
            modifiers: vec![],
            easing: Some(animatix_syntax::easing::Easing::Linear),
            value_span: Some(ByteSpan { start: 9, end: 19 }),
            span: None,
        }
    );
    // Easing extraction: bounce
    assert_eq!(
        parse_single_stmt("btn.opacity = 0.5 [ease: bounce]"),
        Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "btn".to_string()
            )],
            property: "opacity".to_string(),
            value: Expr::Num(0.5),
            modifiers: vec![],
            easing: Some(animatix_syntax::easing::Easing::Bounce),
            value_span: Some(ByteSpan { start: 14, end: 17 }),
            span: None,
        }
    );
    // No easing
    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        Stmt::Assignment {
            target: vec![
                animatix_syntax::ast::TargetSegment::Static("left".to_string()),
                animatix_syntax::ast::TargetSegment::Static("badge".to_string())
            ],
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
        Stmt::LetDecl {
            is_pub: false,
            name: "x".to_string(),
            value: Expr::Path(vec!["container".to_string(), "child".to_string()]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let fill = left.badge.color"),
        Stmt::LetDecl {
            is_pub: false,
            name: "fill".to_string(),
            value: Expr::Path(vec!["left".to_string(), "badge".to_string(), "color".to_string()]),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("let center = scene.center"),
        Stmt::LetDecl {
            is_pub: false,
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
                Stmt::Action(
                    Action {
                        verb: "fade-in".to_string(),
                        targets: vec!["badge".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("500ms".to_string()),
                        }],
                        byte_span: Some(ByteSpan { start: 11, end: 32 }),
                        target_index: vec![None; 1],
                    },
                    None
                ),
                Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "badge".to_string()
                    )],
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
                    value_span: Some(ByteSpan { start: 47, end: 50 }),
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
                Stmt::Action(
                    Action {
                        verb: "fade-in".to_string(),
                        targets: vec!["first".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("200ms".to_string()),
                        }],
                        byte_span: Some(ByteSpan { start: 18, end: 39 }),
                        target_index: vec![None; 1],
                    },
                    None
                ),
                Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "second".to_string()
                    )],
                    property: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("100ms".to_string()),
                    }],
                    easing: None,
                    value_span: Some(ByteSpan { start: 55, end: 58 }),
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
            body: vec![Stmt::Action(
                Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["first".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("200ms".to_string()),
                    }],
                    byte_span: Some(ByteSpan { start: 24, end: 45 }),
                    target_index: vec![None; 1],
                },
                None
            )],
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
                    value_span: Some(ByteSpan { start: 36, end: 40 }),
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
                value_span: Some(ByteSpan { start: 22, end: 38 }),
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
                value_span: Some(ByteSpan { start: 24, end: 26 }),
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
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "badge".to_string()
            )],
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
            value_span: Some(ByteSpan { start: 15, end: 17 }),
            span: None,
        }
    );
    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        Stmt::Assignment {
            target: vec![
                animatix_syntax::ast::TargetSegment::Static("left".to_string()),
                animatix_syntax::ast::TargetSegment::Static("badge".to_string())
            ],
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
                    value_span: Some(ByteSpan { start: 66, end: 76 }),
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
fn test_callout_block_props_parse_as_actor_props() {
    let stmt =
        parse_single_stmt("note: Callout { target: bar[2], place: right, label: \"Look here\" }");
    match stmt {
        Stmt::ActorDecl {
            ty,
            props,
            children,
            ..
        } => {
            assert_eq!(ty, "Callout");
            assert_eq!(props.len(), 3);
            let target = props.iter().find(|p| p.name == "target").expect("target prop");
            assert!(matches!(target.value, Expr::Index(_, _)));
            assert!(props.iter().any(|p| p.name == "place"));
            assert!(children.is_empty());
        },
        other => panic!("expected Callout actor declaration, got {:?}", other),
    }
}

mod part2;
mod part3;
