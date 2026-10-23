use std::collections::HashMap;

use super::TypeEnv;
use crate::ast::{ComponentDef, Expr, Modifier, ParamDef, Property, Stmt, TypeAnnotation};
use crate::diagnostics::DiagnosticCode;
use crate::module::{ComponentEntry, FnTemplate};

fn make_env() -> TypeEnv<'static> {
    let mut components = HashMap::new();
    components.insert(
        "Button".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Button".to_string(),
                params: vec![
                    ParamDef {
                        name: "size".to_string(),
                        param_type: Some(TypeAnnotation::Vec2),
                        default: Some(Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(40.0)])),
                    },
                    ParamDef {
                        name: "color".to_string(),
                        param_type: Some(TypeAnnotation::Color),
                        default: Some(Expr::Ident("blue".to_string())),
                    },
                ],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())))
}

#[test]
fn typecheck_accepts_valid_props() {
    let env = make_env();
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "btn".to_string(),
        array_index: None,
        ty: "Button".to_string(),
        props: vec![Property {
            name: "size".to_string(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(60.0)]),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let mut env = env;
    let diagnostics = env.check_statements(&stmts);
    assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
}

#[test]
fn typecheck_rejects_mismatched_prop() {
    let env = make_env();
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "btn".to_string(),
        array_index: None,
        ty: "Button".to_string(),
        props: vec![Property {
            name: "size".to_string(),
            value: Expr::Str("too big".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let mut env = env;
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("Type mismatch"));
}

#[test]
fn strict_types_warns_on_missing_annotations() {
    let mut env = make_env();
    env.strict_types = true;
    let stmts = vec![Stmt::ComponentDef(
        ComponentDef {
            name: "Card".to_string(),
            params: vec![ParamDef {
                name: "title".to_string(),
                param_type: None,
                default: Some(Expr::Str("Untitled".to_string())),
            }],
            body: vec![],
            is_pub: false,
        },
        None,
    )];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("missing a type annotation"));
    assert!(diagnostics[0].message.contains("strict mode"));
}

#[test]
fn strict_types_no_warning_when_annotated() {
    let mut env = make_env();
    env.strict_types = true;
    let stmts = vec![Stmt::ComponentDef(
        ComponentDef {
            name: "Card".to_string(),
            params: vec![ParamDef {
                name: "title".to_string(),
                param_type: Some(TypeAnnotation::Str),
                default: Some(Expr::Str("Untitled".to_string())),
            }],
            body: vec![],
            is_pub: false,
        },
        None,
    )];
    let diagnostics = env.check_statements(&stmts);
    assert!(diagnostics.is_empty());
}

#[test]
fn typecheck_validates_action_param_types() {
    use crate::ast::Action;
    let mut components = HashMap::new();
    components.insert(
        "Button".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Button".to_string(),
                params: vec![],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: {
                let mut actions = HashMap::new();
                actions.insert(
                    "pulse".to_string(),
                    FnTemplate {
                        params: vec![ParamDef {
                            name: "scale".to_string(),
                            param_type: Some(TypeAnnotation::Num),
                            default: Some(Expr::Num(1.15)),
                        }],
                        return_type: None,
                        body: vec![],
                    },
                );
                actions
            },
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));

    let stmts = vec![
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
        Stmt::Action(
            Action {
                verb: "pulse".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: Some("scale".to_string()),
                    value: Expr::Str("too big".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ),
    ];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("Type mismatch"));
    assert!(diagnostics[0].message.contains("scale"));
}

#[test]
fn typecheck_accepts_valid_action_params() {
    use crate::ast::Action;
    let mut components = HashMap::new();
    components.insert(
        "Button".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Button".to_string(),
                params: vec![],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: {
                let mut actions = HashMap::new();
                actions.insert(
                    "pulse".to_string(),
                    FnTemplate {
                        params: vec![ParamDef {
                            name: "scale".to_string(),
                            param_type: Some(TypeAnnotation::Num),
                            default: Some(Expr::Num(1.15)),
                        }],
                        return_type: None,
                        body: vec![],
                    },
                );
                actions
            },
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));

    let stmts = vec![
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
        Stmt::Action(
            Action {
                verb: "pulse".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: Some("scale".to_string()),
                    value: Expr::Num(1.5),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ),
    ];
    let diagnostics = env.check_statements(&stmts);
    assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
}

#[test]
fn type_alias_resolves_in_component_param() {
    let def = ComponentDef {
        name: "Card".to_string(),
        params: vec![ParamDef {
            name: "value".to_string(),
            param_type: Some(TypeAnnotation::Alias("Metric".to_string())),
            default: None,
        }],
        body: vec![],
        is_pub: false,
    };
    let mut components = HashMap::new();
    components.insert(
        "Card".to_string(),
        ComponentEntry {
            definition: def.clone(),
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let empty_actions = HashMap::new();
    let mut env = TypeEnv::new(&components, &empty_actions);
    let stmts = vec![
        Stmt::TypeAlias {
            is_pub: true,
            name: "Metric".to_string(),
            annotation: TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
            span: None,
        },
        Stmt::ComponentDef(def.clone(), None),
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "card".to_string(),
            array_index: None,
            ty: "Card".to_string(),
            props: vec![Property {
                name: "value".to_string(),
                value: Expr::Str("Revenue".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
    ];
    let diagnostics = env.check_statements(&stmts);
    assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
}

#[test]
fn example_component_actions_demo_parses_and_typechecks() {
    let source = include_str!("../../../../examples/components/09_components.amx");
    let (ast, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let ast = ast.expect("parsed AST");

    // Typecheck directly without module loading (avoids path resolution)
    let mut components = HashMap::new();
    for def in crate::module::discovery::collect_component_defs(&ast) {
        let actions = crate::module::discovery::collect_component_actions(&def);
        components.insert(
            def.name.clone(),
            ComponentEntry {
                definition: def,
                source_path: std::path::PathBuf::new(),
                actions,
            },
        );
    }
    let module_fns = crate::module::ModuleGraph::collect_module_fns(&ast);
    let mut env = TypeEnv::new(&components, &module_fns);
    let type_errors = env.check_statements(&ast);
    assert!(type_errors.is_empty(), "Type errors: {:?}", type_errors);
}

#[test]
fn colorscheme_paths_infer_color_in_strict_mode() {
    use crate::ast::Expr;
    use crate::typing::{Type, TypeEnv, infer_expr_type};
    let env = TypeEnv::with_stdlib();
    // Known colorscheme namespaces with ≥2 segments → Color
    for ns in &["accent", "text", "surface", "stroke"] {
        let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
        assert_eq!(infer_expr_type(&path, &env), Type::Color, "{ns}.primary should be Color");
    }
    // scene.* stays Any (mixes colors and anchors)
    let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
    assert_eq!(infer_expr_type(&scene, &env), Type::Color);
    // single-segment stays Any
    let single = Expr::Path(vec!["accent".to_string()]);
    assert_eq!(infer_expr_type(&single, &env), Type::Any);
}

#[test]
fn named_color_list_accepted_for_list_color_param() {
    let mut components = HashMap::new();
    components.insert(
        "Swatches".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Swatches".to_string(),
                params: vec![ParamDef {
                    name: "colors".to_string(),
                    param_type: Some(TypeAnnotation::List(Box::new(TypeAnnotation::Color))),
                    default: None,
                }],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "s".to_string(),
        array_index: None,
        ty: "Swatches".to_string(),
        props: vec![Property {
            name: "colors".to_string(),
            value: Expr::List(vec![
                Expr::Ident("red".to_string()),
                Expr::Ident("blue".to_string()),
            ]),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "named color list should satisfy List<Color>, got: {:?}",
        diagnostics
    );
}

#[test]
fn rich_type_annotations_accept_matching_values() {
    let mut components = HashMap::new();
    components.insert(
        "Probe".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Probe".to_string(),
                params: vec![
                    ParamDef {
                        name: "p".to_string(),
                        param_type: Some(TypeAnnotation::Vec3),
                        default: None,
                    },
                    ParamDef {
                        name: "pair".to_string(),
                        param_type: Some(TypeAnnotation::Tuple(vec![
                            TypeAnnotation::Str,
                            TypeAnnotation::Num,
                        ])),
                        default: None,
                    },
                    ParamDef {
                        name: "mapper".to_string(),
                        param_type: Some(TypeAnnotation::Function {
                            params: vec![TypeAnnotation::Num],
                            ret: Box::new(TypeAnnotation::Num),
                        }),
                        default: None,
                    },
                ],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "probe".to_string(),
        array_index: None,
        ty: "Probe".to_string(),
        props: vec![
            Property {
                name: "p".to_string(),
                value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "pair".to_string(),
                value: Expr::Tuple(vec![Expr::Str("x".to_string()), Expr::Num(1.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "mapper".to_string(),
                value: Expr::Closure(vec!["x".to_string()], Box::new(Expr::Num(1.0))),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "matching rich annotations should typecheck, got: {:?}",
        diagnostics
    );
}

#[test]
fn rich_type_annotations_reject_mismatched_values() {
    let mut components = HashMap::new();
    components.insert(
        "Probe".to_string(),
        ComponentEntry {
            definition: ComponentDef {
                name: "Probe".to_string(),
                params: vec![
                    ParamDef {
                        name: "p".to_string(),
                        param_type: Some(TypeAnnotation::Vec3),
                        default: None,
                    },
                    ParamDef {
                        name: "pair".to_string(),
                        param_type: Some(TypeAnnotation::Tuple(vec![
                            TypeAnnotation::Str,
                            TypeAnnotation::Num,
                        ])),
                        default: None,
                    },
                    ParamDef {
                        name: "mapper".to_string(),
                        param_type: Some(TypeAnnotation::Function {
                            params: vec![TypeAnnotation::Num],
                            ret: Box::new(TypeAnnotation::Num),
                        }),
                        default: None,
                    },
                ],
                body: vec![],
                is_pub: false,
            },
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "probe".to_string(),
        array_index: None,
        ty: "Probe".to_string(),
        props: vec![
            Property {
                name: "p".to_string(),
                value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "pair".to_string(),
                value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Str("x".to_string())]),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "mapper".to_string(),
                value: Expr::Closure(vec!["x".to_string()], Box::new(Expr::Str("bad".to_string()))),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 3, "expected three type mismatches, got: {:?}", diagnostics);
    assert!(diagnostics.iter().all(|d| d.code == DiagnosticCode::TypeMismatch));
}

#[test]
fn colorscheme_path_accepted_for_color_param() {
    // accent.primary should be accepted where a Color param is expected
    let env = make_env(); // Button has a 'color: Color' param
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "btn".to_string(),
        array_index: None,
        ty: "Button".to_string(),
        props: vec![Property {
            name: "color".to_string(),
            value: Expr::Path(vec!["accent".to_string(), "primary".to_string()]),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let mut env = env;
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "accent.primary should be accepted for Color param, got: {:?}",
        diagnostics
    );
}

#[test]
fn type_alias_resolves_when_declared_after_component() {
    let def = ComponentDef {
        name: "Card".to_string(),
        params: vec![ParamDef {
            name: "value".to_string(),
            param_type: Some(TypeAnnotation::Alias("Metric".to_string())),
            default: None,
        }],
        body: vec![],
        is_pub: false,
    };
    let mut components = HashMap::new();
    components.insert(
        "Card".to_string(),
        ComponentEntry {
            definition: def.clone(),
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let empty_actions = HashMap::new();
    let mut env = TypeEnv::new(&components, &empty_actions);
    let stmts = vec![
        Stmt::ComponentDef(def.clone(), None),
        Stmt::TypeAlias {
            is_pub: true,
            name: "Metric".to_string(),
            annotation: TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
            span: None,
        },
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "card".to_string(),
            array_index: None,
            ty: "Card".to_string(),
            props: vec![Property {
                name: "value".to_string(),
                value: Expr::Str("Revenue".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
    ];
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "alias declared after the component should resolve, got: {:?}",
        diagnostics
    );
}

#[test]
fn unresolved_type_alias_is_reported() {
    let def = ComponentDef {
        name: "Card".to_string(),
        params: vec![ParamDef {
            name: "value".to_string(),
            param_type: Some(TypeAnnotation::Alias("Missing".to_string())),
            default: None,
        }],
        body: vec![],
        is_pub: false,
    };
    let mut components = HashMap::new();
    components.insert(
        "Card".to_string(),
        ComponentEntry {
            definition: def.clone(),
            source_path: std::path::PathBuf::new(),
            actions: HashMap::new(),
        },
    );
    let empty_actions = HashMap::new();
    let mut env = TypeEnv::new(&components, &empty_actions);
    let stmts = vec![Stmt::ComponentDef(def, None)];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownTypeAlias);
    assert!(diagnostics[0].message.contains("Missing"));
}

#[test]
fn unresolved_top_level_type_alias_is_reported() {
    let empty_components = HashMap::new();
    let empty_actions = HashMap::new();
    let mut env = TypeEnv::new(&empty_components, &empty_actions);
    let stmts = vec![Stmt::TypeAlias {
        is_pub: true,
        name: "Broken".to_string(),
        annotation: TypeAnnotation::Alias("Missing".to_string()),
        span: None,
    }];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownTypeAlias);
    assert!(diagnostics[0].message.contains("Missing"));
}
