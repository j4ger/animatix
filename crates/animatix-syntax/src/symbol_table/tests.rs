use super::*;

#[test]
fn builtin_types_populated() {
    let stmts = vec![];
    let table = SymbolTable::build_from_ast(&stmts);
    assert!(table.types.contains("Text"));
    assert!(table.types.contains("Ellipse"));
    assert!(table.types.contains("Button"));
}

#[test]
fn extracts_actor_labels() {
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "btn".to_string(),
        array_index: None,
        ty: "Button".to_string(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let table = SymbolTable::build_from_ast(&stmts);
    assert!(table.labels.contains_key("btn"));
    assert_eq!(table.labels["btn"].ty.as_deref(), Some("Button"));
}

#[test]
fn extracts_let_bindings() {
    let stmts = vec![Stmt::LetDecl {
        is_pub: false,
        name: "x".to_string(),
        value: Expr::Num(42.0),
        span: None,
    }];
    let table = SymbolTable::build_from_ast(&stmts);
    assert!(table.labels.contains_key("x"));
    assert_eq!(table.labels["x"].kind, LabelKind::Let);
}

#[test]
fn extracts_component_definitions() {
    let stmts = vec![Stmt::ComponentDef(
        ComponentDef {
            is_pub: false,
            name: "MyButton".to_string(),
            params: vec![ParamDef {
                name: "text".to_string(),
                param_type: None,
                default: Some(Expr::Str("Click".to_string())),
            }],
            body: vec![],
        },
        None,
    )];
    let table = SymbolTable::build_from_ast(&stmts);
    assert!(table.components.contains_key("MyButton"));
    assert_eq!(table.components["MyButton"].params.len(), 1);
}

#[test]
fn collects_properties_from_actors() {
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "title".to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![
            Property {
                name: "content".to_string(),
                value: Expr::Str("Hello".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "font_size".to_string(),
                value: Expr::Num(24.0),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let table = SymbolTable::build_from_ast(&stmts);
    let text_props = table.properties.get("Text").unwrap();
    assert!(text_props.contains(&"text".to_string()));
    assert!(text_props.contains(&"font_size".to_string()));
}

#[test]
fn colorscheme_paths_infer_color() {
    let env = typing::TypeEnv::with_stdlib();
    // accent.*, text.*, surface.*, stroke.* with ≥2 segments → Color
    for ns in &["accent", "text", "surface", "stroke"] {
        let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
        assert_eq!(
            typing::infer_expr_type(&path, &env),
            typing::Type::Color,
            "{ns}.primary should be Color"
        );
    }
    // scene.* resolves known fields (background is a Color)
    let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
    assert_eq!(typing::infer_expr_type(&scene, &env), typing::Type::Color);
    // single-segment stays Any
    let single = Expr::Path(vec!["accent".to_string()]);
    assert_eq!(typing::infer_expr_type(&single, &env), typing::Type::Any);
}

#[test]
fn callout_target_references_count_as_usage() {
    let actor = |label: &str| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Rect".to_string(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    };
    let callout = Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "note".to_string(),
        array_index: None,
        ty: "Callout".to_string(),
        props: vec![Property {
            name: "target".to_string(),
            value: Expr::Ident("box1".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    };
    let assignment = Stmt::Assignment {
        target: vec![TargetSegment::Static("note".to_string())],
        property: "target".to_string(),
        value: Expr::Str("box2".to_string()),
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    };
    let stmts = vec![actor("box1"), actor("box2"), callout, assignment];
    let mut table = SymbolTable::build_from_ast(&stmts);
    table.collect_references(&stmts);
    assert!(table.referenced_labels.contains("box1"));
    assert!(table.referenced_labels.contains("box2"));
}

#[test]
fn indexed_assignment_marks_runtime_var_as_referenced() {
    // `always { let sel = 1; bars[sel].color = red }`
    // `sel` should be in `referenced_labels` after collect_references.
    let stmts = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "sel".to_string(),
            value: Expr::Num(1.0),
            span: None,
        },
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec![
                    TargetSegment::Indexed {
                        base: "bars".to_string(),
                        index: Box::new(Expr::Ident("sel".to_string())),
                    },
                    TargetSegment::Static("color".to_string()),
                ],
                property: "color".to_string(),
                value: Expr::Path(vec!["red".to_string()]),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];
    let mut table = SymbolTable::build_from_ast(&stmts);
    table.collect_references(&stmts);
    assert!(
        table.referenced_labels.contains("sel"),
        "'sel' should be referenced by bars[sel].color = red"
    );
    assert!(
        table.referenced_labels.contains("bars"),
        "'bars' should be referenced as the array base"
    );
}
