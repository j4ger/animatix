use super::*;

fn neg(value: f64) -> Expr {
    Expr::Unary(UnaryOp::Neg, Box::new(Expr::Num(value)))
}

fn pair(x: f64, y: f64) -> Expr {
    Expr::Tuple(vec![Expr::Num(x), Expr::Num(y)])
}

fn labeled(label: &str, ty: &str, props: Vec<Property>, children: Vec<InlineItem>) -> InlineItem {
    InlineItem::Labeled {
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children,
    }
}

fn anonymous(ty: &str, props: Vec<Property>, children: Vec<InlineItem>) -> InlineItem {
    InlineItem::Anonymous {
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children,
    }
}

#[test]
fn test_svg_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "icon: Svg { url: \"examples/vector.svg\", anchor: scene.top, offset: (0, 24), scale: 1.5 }"
        ),
        actor(
            "icon",
            "Svg",
            vec![
                prop("url", Expr::Str("examples/vector.svg".to_string()), (17, 38)),
                prop("anchor", Expr::Path(vec!["scene".to_string(), "top".to_string()]), (48, 57)),
                prop("offset", pair(0.0, 24.0), (67, 74)),
                prop("scale", Expr::Num(1.5), (83, 86)),
            ]
        )
    );
}

#[test]
fn test_image_stmt_preserves_anchor_and_offset() {
    assert_eq!(
        parse_single_stmt(
            "photo: Image { url: \"examples/checker.png\", anchor: scene.bottom, offset: (0, -40), size: (240, 180) }"
        ),
        actor(
            "photo",
            "Image",
            vec![
                prop("url", Expr::Str("examples/checker.png".to_string()), (20, 42)),
                prop(
                    "anchor",
                    Expr::Path(vec!["scene".to_string(), "bottom".to_string()]),
                    (52, 64)
                ),
                prop("offset", Expr::Tuple(vec![Expr::Num(0.0), neg(40.0)]), (74, 82)),
                prop("size", pair(240.0, 180.0), (90, 100)),
            ]
        )
    );
}

#[test]
fn test_line_actor_decl() {
    assert_eq!(
        parse_single_stmt("axis: Line, from: (-40, 0), to: (40, 0), stroke: blue, stroke_width: 3"),
        actor(
            "axis",
            "Line",
            vec![
                prop("from", Expr::Tuple(vec![neg(40.0), Expr::Num(0.0)]), (18, 26)),
                prop("to", pair(40.0, 0.0), (32, 39)),
                prop("stroke", Expr::Ident("blue".to_string()), (49, 53)),
                prop("stroke_width", Expr::Num(3.0), (69, 70)),
            ]
        )
    );
}

#[test]
fn test_ellipse_actor_decl() {
    assert_eq!(
        parse_single_stmt("halo: Ellipse, radius_x: 80, radius_y: 30, color: green"),
        actor(
            "halo",
            "Ellipse",
            vec![
                prop("radius_x", Expr::Num(80.0), (25, 27)),
                prop("radius_y", Expr::Num(30.0), (39, 41)),
                prop("color", Expr::Ident("green".to_string()), (50, 55)),
            ]
        )
    );
}

#[test]
fn test_arc_actor_decl() {
    assert_eq!(
        parse_single_stmt(
            "ring: Ellipse, radius_x: 80, radius_y: 50, start_angle: 0, sweep_angle: 3.14, stroke: gold"
        ),
        actor(
            "ring",
            "Ellipse",
            vec![
                prop("radius_x", Expr::Num(80.0), (25, 27)),
                prop("radius_y", Expr::Num(50.0), (39, 41)),
                prop("start_angle", Expr::Num(0.0), (56, 57)),
                prop("sweep_angle", Expr::Num(3.14), (72, 76)),
                prop("stroke", Expr::Ident("gold".to_string()), (86, 90)),
            ]
        )
    );
}

#[test]
fn test_polygon_actor_decl() {
    let points = Expr::List(vec![
        Expr::Tuple(vec![neg(80.0), Expr::Num(0.0)]),
        Expr::Tuple(vec![Expr::Num(0.0), neg(70.0)]),
        pair(90.0, 0.0),
        pair(0.0, 80.0),
    ]);
    assert_eq!(
        parse_single_stmt(
            "badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan"
        ),
        actor(
            "badge",
            "Polygon",
            vec![
                prop("points", points, (24, 62)),
                prop("color", Expr::Ident("cyan".to_string()), (71, 75)),
            ]
        )
    );
}

#[test]
fn test_path_actor_decl() {
    let call = |name: &str, args: Vec<Expr>| Expr::Call(name.to_string(), args);
    let commands = Expr::List(vec![
        call("move_to", vec![neg(120.0), Expr::Num(0.0)]),
        call("line_to", vec![neg(40.0), neg(80.0)]),
        call(
            "curve_to",
            vec![
                Expr::Num(20.0),
                neg(120.0),
                Expr::Num(80.0),
                Expr::Num(40.0),
                Expr::Num(140.0),
                neg(10.0),
            ],
        ),
        call("close", vec![]),
    ]);
    assert_eq!(
        parse_single_stmt(
            "guide: Path, commands: {move_to(-120, 0), line_to(-40, -80), curve_to(20, -120, 80, 40, 140, -10), close()}, stroke: white"
        ),
        actor(
            "guide",
            "Path",
            vec![
                prop("commands", commands, (23, 107)),
                prop("stroke", Expr::Ident("white".to_string()), (117, 122)),
            ]
        )
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
                labeled("a", "Ellipse", vec![prop("size", Expr::Num(10.0), (33, 35))], vec![]),
                labeled("b", "Rect", vec![prop("size", Expr::Num(20.0), (52, 54))], vec![]),
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
                anonymous("Ellipse", vec![prop("size", Expr::Num(10.0), (30, 32))], vec![]),
                anonymous("Rect", vec![prop("size", Expr::Num(20.0), (46, 48))], vec![]),
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
                labeled(
                    "a",
                    "Ellipse",
                    vec![prop("size", Expr::Num(10.0), (33, 35))],
                    vec![labeled(
                        "child",
                        "Text",
                        vec![prop("text", Expr::Str("hi".to_string()), (57, 61))],
                        vec![]
                    )],
                ),
                labeled("b", "Rect", vec![prop("size", Expr::Num(20.0), (80, 82))], vec![]),
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
        action("fade-out", &["ball"], vec![modifier("1s")], (0, 18))
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
    assert_eq!(
        parse_single_stmt("always { let x = btn.x }"),
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
    let value = Expr::Conditional(
        Box::new(Expr::Ident("active".to_string())),
        Box::new(pair(120.0, 120.0)),
        Box::new(pair(180.0, 180.0)),
    );
    assert_eq!(
        parse_single_stmt("pulse.size = if active { (120, 120) } else { (180, 180) }"),
        assignment(&["pulse"], "size", value, (13, 57))
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
    assert_eq!(
        parse_single_stmt("if active { appear btn }"),
        Stmt::Conditional {
            condition: Expr::Ident("active".to_string()),
            then_branch: vec![action("appear", &["btn"], vec![], (12, 22))],
            else_branch: None,
            span: None,
        }
    );
}

#[test]
fn test_conditional_with_else() {
    assert_eq!(
        parse_single_stmt("if active { fade-in btn } else { fade-out btn }"),
        Stmt::Conditional {
            condition: Expr::Ident("active".to_string()),
            then_branch: vec![action("fade-in", &["btn"], vec![], (12, 23))],
            else_branch: Some(vec![action("fade-out", &["btn"], vec![], (33, 45))]),
            span: None,
        }
    );
}

#[test]
fn test_for_loop() {
    assert_eq!(
        parse_single_stmt("for item in buttons { appear item }"),
        Stmt::ForLoop {
            var: LoopPattern::Single("item".to_string()),
            index_var: None,
            iterable: Expr::Ident("buttons".to_string()),
            modifiers: vec![],
            body: vec![action("appear", &["item"], vec![], (22, 33))],
            span: None,
        }
    );
}
