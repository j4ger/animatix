#![allow(clippy::approx_constant)]

use animatix::timeline::eval_shared::eval_builtin_fn;
use animatix::timeline::{Timeline as AnimatixTimeline, Value};
use animatix_syntax::ast::{
    Action, ByteSpan, Expr, InlineItem, LoopPattern, MatchPattern, Modifier, Property, Stmt,
    TargetSegment, Time, UnaryOp,
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

/// AST constructors shared by this module and its `actors` / `control_flow`
/// children. Byte spans stay explicit: a shifted span is a parser regression,
/// so every fixture pins its own offsets.
fn at(start: usize, end: usize) -> Option<ByteSpan> {
    Some(ByteSpan { start, end })
}

fn prop(name: &str, value: Expr, value_span: (usize, usize)) -> Property {
    Property {
        name: name.to_string(),
        value,
        value_span: at(value_span.0, value_span.1),
        trailing_comment: None,
    }
}

fn modifier(value: &str) -> Modifier {
    Modifier {
        name: None,
        value: Expr::Ident(value.to_string()),
    }
}

fn named_modifier(name: &str, value: &str) -> Modifier {
    Modifier {
        name: Some(name.to_string()),
        value: Expr::Ident(value.to_string()),
    }
}

fn target(segments: &[&str]) -> Vec<TargetSegment> {
    segments
        .iter()
        .map(|segment| TargetSegment::Static(segment.to_string()))
        .collect()
}

fn actor_with(label: &str, ty: &str, props: Vec<Property>, modifiers: Vec<Modifier>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers,
        children: vec![],
        span: None,
    }
}

fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    actor_with(label, ty, props, vec![])
}

fn assignment(segments: &[&str], property: &str, value: Expr, value_span: (usize, usize)) -> Stmt {
    Stmt::Assignment {
        target: target(segments),
        property: property.to_string(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: at(value_span.0, value_span.1),
        span: None,
    }
}

fn action(
    verb: &str,
    targets: &[&str],
    modifiers: Vec<Modifier>,
    byte_span: (usize, usize),
) -> Stmt {
    Stmt::Action(
        Action {
            verb: verb.to_string(),
            targets: targets.iter().map(|name| name.to_string()).collect(),
            args: vec![],
            modifiers,
            byte_span: at(byte_span.0, byte_span.1),
            target_index: vec![None; targets.len()],
        },
        None,
    )
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
    for (source, name, value) in [
        ("let a = 42", "a", Expr::Num(42.0)),
        ("let b = 3.14", "b", Expr::Num(3.14)),
        ("let c = true", "c", Expr::Bool(true)),
        ("let d = false", "d", Expr::Bool(false)),
        ("let e = null", "e", Expr::Null),
        ("let f = \"string\"", "f", Expr::Str("string".to_string())),
    ] {
        assert_eq!(
            parse_single_stmt(source),
            Stmt::LetDecl {
                is_pub: false,
                name: name.to_string(),
                value,
                span: None
            },
            "let declaration for {source:?}"
        );
    }
}

#[test]
fn test_collections() {
    for (source, name, value) in [
        (
            "let coords = (10, 20.5)",
            "coords",
            Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)]),
        ),
        (
            "let arr = {a, b}",
            "arr",
            Expr::List(vec![Expr::Ident("a".to_string()), Expr::Ident("b".to_string())]),
        ),
        (
            "let pct = (50%, 25%)",
            "pct",
            Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)]),
        ),
    ] {
        assert_eq!(
            parse_single_stmt(source),
            Stmt::LetDecl {
                is_pub: false,
                name: name.to_string(),
                value,
                span: None
            },
            "collection literal for {source:?}"
        );
    }
}

#[test]
fn test_assignments_and_paths() {
    let mut eased = assignment(
        &["morpher"],
        "size",
        Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
        (15, 25),
    );
    if let Stmt::Assignment {
        modifiers, easing, ..
    } = &mut eased
    {
        *modifiers = vec![modifier("2s")];
        *easing = Some(animatix_syntax::easing::Easing::EaseOut);
    }
    assert_eq!(parse_single_stmt("morpher.size = (100, 100) [2s, ease: ease-out]"), eased);

    // Easing extraction: linear
    let mut linear =
        assignment(&["btn"], "at", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]), (9, 19));
    if let Stmt::Assignment { easing, .. } = &mut linear {
        *easing = Some(animatix_syntax::easing::Easing::Linear);
    }
    assert_eq!(parse_single_stmt("btn.at = (100, 100) [ease: linear]"), linear);

    // Easing extraction: bounce
    let mut bounce = assignment(&["btn"], "opacity", Expr::Num(0.5), (14, 17));
    if let Stmt::Assignment { easing, .. } = &mut bounce {
        *easing = Some(animatix_syntax::easing::Easing::Bounce);
    }
    assert_eq!(parse_single_stmt("btn.opacity = 0.5 [ease: bounce]"), bounce);

    // No easing, dotted target
    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        assignment(&["left", "badge"], "color", Expr::Ident("red".to_string()), (19, 22))
    );

    for (source, name, segments) in [
        ("let x = container.child", "x", ["container", "child"].as_slice()),
        ("let fill = left.badge.color", "fill", ["left", "badge", "color"].as_slice()),
        ("let center = scene.center", "center", ["scene", "center"].as_slice()),
    ] {
        assert_eq!(
            parse_single_stmt(source),
            Stmt::LetDecl {
                is_pub: false,
                name: name.to_string(),
                value: Expr::Path(segments.iter().map(|s| s.to_string()).collect()),
                span: None,
            },
            "path expression for {source:?}"
        );
    }
}

#[test]
fn test_sequence_parse() {
    let mut delayed = assignment(&["badge"], "color", Expr::Ident("red".to_string()), (47, 50));
    if let Stmt::Assignment { modifiers, .. } = &mut delayed {
        *modifiers = vec![modifier("250ms"), named_modifier("delay", "100ms")];
    }
    assert_eq!(
        parse_single_stmt(
            "sequence { fade-in badge [500ms] badge.color = red [250ms, delay: 100ms] }"
        ),
        Stmt::Sequence {
            body: vec![
                action("fade-in", &["badge"], vec![modifier("500ms")], (11, 32)),
                delayed
            ],
            span: None,
        }
    );
}

#[test]
fn test_stagger_parse() {
    let mut timed = assignment(&["second"], "color", Expr::Ident("red".to_string()), (55, 58));
    if let Stmt::Assignment { modifiers, .. } = &mut timed {
        *modifiers = vec![modifier("100ms")];
    }
    assert_eq!(
        parse_single_stmt("stagger [150ms] { fade-in first [200ms] second.color = red [100ms] }"),
        Stmt::Stagger {
            modifiers: vec![modifier("150ms")],
            body: vec![
                action("fade-in", &["first"], vec![modifier("200ms")], (18, 39)),
                timed
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
            modifiers: vec![named_modifier("each", "150ms")],
            body: vec![action(
                "fade-in",
                &["first"],
                vec![modifier("200ms")],
                (24, 45)
            )],
            span: None,
        }
    );
}

#[test]
fn test_actor_decl_full() {
    assert_eq!(
        parse_single_stmt("circle: Ellipse, radius: 50, color: blue [2s, ease: bounce]"),
        actor_with(
            "circle",
            "Ellipse",
            vec![
                prop("radius", Expr::Num(50.0), (25, 27)),
                prop("color", Expr::Ident("blue".to_string()), (36, 40)),
            ],
            vec![modifier("2s"), named_modifier("ease", "bounce")],
        )
    );
}

#[test]
fn test_config_parse() {
    assert_eq!(
        parse_program("config { colorscheme: \"editorial-dark\" }")[0],
        Stmt::Config {
            settings: vec![prop(
                "colorscheme",
                Expr::Str("editorial-dark".to_string()),
                (22, 38)
            )],
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
        actor(
            "badge",
            "Ellipse",
            vec![
                prop("color", Expr::Ident("auto".to_string()), (23, 27)),
                prop(
                    "stroke",
                    Expr::Path(vec!["stroke".to_string(), "default".to_string()]),
                    (37, 51),
                ),
            ]
        )
    );
}

#[test]
fn test_text_colorscheme_alias_parse() {
    assert_eq!(
        parse_single_stmt("title: Text, text: \"Animatix\", color: text.primary"),
        actor(
            "title",
            "Text",
            vec![
                prop("text", Expr::Str("Animatix".to_string()), (19, 29)),
                prop(
                    "color",
                    Expr::Path(vec!["text".to_string(), "primary".to_string()]),
                    (38, 50),
                ),
            ]
        )
    );
}

#[test]
fn test_modifier_delay_and_duplicates_parse() {
    assert_eq!(
        parse_single_stmt("badge: Ellipse, radius: 20 [delay: 250ms, ease: ease-in, ease: bounce]"),
        actor_with(
            "badge",
            "Ellipse",
            vec![prop("radius", Expr::Num(20.0), (24, 26))],
            vec![
                named_modifier("delay", "250ms"),
                named_modifier("ease", "ease-in"),
                named_modifier("ease", "bounce"),
            ],
        )
    );

    let mut delayed = assignment(&["badge"], "radius", Expr::Num(40.0), (15, 17));
    if let Stmt::Assignment { modifiers, .. } = &mut delayed {
        *modifiers = vec![named_modifier("delay", "1s"), modifier("500ms")];
    }
    assert_eq!(parse_single_stmt("badge.radius = 40 [delay: 1s, 500ms]"), delayed);

    assert_eq!(
        parse_single_stmt("left.badge.color = red"),
        assignment(&["left", "badge"], "color", Expr::Ident("red".to_string()), (19, 22))
    );

    assert_eq!(
        parse_single_stmt("card: MetricCard, title: \"Latency\""),
        actor(
            "card",
            "MetricCard",
            vec![prop("title", Expr::Str("Latency".to_string()), (25, 34))]
        )
    );
}

#[test]
fn test_code_stmt_parse() {
    assert_eq!(
        parse_single_stmt("snippet: Code, code: \"fn main() {}\", font_size: 18"),
        actor(
            "snippet",
            "Code",
            vec![
                prop("code", Expr::Str("fn main() {}".to_string()), (21, 35)),
                prop("font_size", Expr::Num(18.0), (48, 50)),
            ]
        )
    );
}

#[test]
fn test_image_stmt() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.png\", at: (100, 120), size: (240, 180) }"
        ),
        actor(
            "photo",
            "Image",
            vec![
                prop("url", Expr::Str("examples/checker.png".to_string()), (20, 42)),
                prop("at", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(120.0)]), (48, 58)),
                prop("size", Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(180.0)]), (66, 76)),
            ]
        )
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

mod actors;
mod control_flow;
