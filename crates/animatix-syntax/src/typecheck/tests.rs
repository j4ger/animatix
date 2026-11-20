use std::collections::HashMap;
use std::path::PathBuf;

use super::TypeEnv;
use crate::ast::{ComponentDef, Expr, ParamDef, Property, Stmt, TypeAnnotation};
use crate::diagnostics::DiagnosticCode;
use crate::module::{ComponentEntry, FnTemplate, ModuleGraph, SourceAccess};
use crate::test_support::{actor, actor_with_props};

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
            methods: HashMap::new(),
        },
    );
    TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())))
}

fn make_rich_probe_env() -> TypeEnv<'static> {
    let components = HashMap::from([(
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
            methods: HashMap::new(),
        },
    )]);
    TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())))
}

fn typecheck_source(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let path = PathBuf::from("/virtual/typecheck.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(path.clone(), source.to_string());
    let mut program = graph.load_program(&path).expect("source should parse and load");
    program.typecheck()
}

#[test]
fn hex_color_is_color_but_quoted_digits_are_str() {
    let source = r#"
fn same(ink: Color) -> Color { return ink }
let ink = hex_color("fff1d6")
let accepted = same(ink)
let rejected = same("fff1d6")
"#;
    let diagnostics = typecheck_source(source);
    let errors = diagnostics.iter().filter(|error| error.is_error()).collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "{diagnostics:?}");
    assert_eq!(errors[0].code, DiagnosticCode::TypeMismatch);
    assert!(errors[0].message.contains("expects Color, got Str"), "{errors:?}");
}

#[test]
fn typecheck_accepts_valid_props() {
    let env = make_env();
    let stmts = vec![actor_with_props(
        "btn",
        "Button",
        vec![Property::new(
            "size",
            Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(60.0)]),
        )],
    )];
    let mut env = env;
    let diagnostics = env.check_statements(&stmts);
    assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
}

#[test]
fn typecheck_rejects_mismatched_prop() {
    let env = make_env();
    let stmts = vec![actor_with_props(
        "btn",
        "Button",
        vec![Property::new("size", Expr::Str("too big".to_string()))],
    )];
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
fn local_ticker_example_typechecks_with_keyframes_and_always_body() {
    let source = include_str!("../../../../examples/animation/local_tickers.amx");
    let diagnostics = typecheck_source(source);
    assert!(
        diagnostics.is_empty(),
        "local keyframes and `always` are valid in a timeline function: {diagnostics:?}"
    );
}

#[test]
fn ordinary_functions_infer_tail_results_and_check_optional_annotations() {
    let diagnostics = typecheck_source(
        r#"
fn add(a: Num, b: Num) { a + b }
fn enter(dot: Actor) { fade_in(target: dot, duration: 100ms) }
fn typed_enter(dot: Actor) -> Timeline { fade_in(target: dot, duration: 100ms) }
fn invalid_result(dot: Actor) -> Num { fade_in(target: dot, duration: 100ms) }
"#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected only the declared return mismatch: {diagnostics:?}"
    );
    assert!(
        diagnostics[0]
            .message
            .contains("declares return type Num, but returns Timeline")
    );
}

#[test]
fn incompatible_branch_results_require_an_explicit_union_annotation() {
    let missing_annotation =
        typecheck_source("fn choose(flag: Bool) { if flag { return 1 } else { return \"one\" } }");
    assert!(
        missing_annotation.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::TypeMismatch
                && diagnostic.message.contains("declare an explicit union return type")
        }),
        "incompatible inferred branches need a declared union: {missing_annotation:?}"
    );

    let explicit_union = typecheck_source(
        "fn choose(flag: Bool) -> Num | Str { if flag { return 1 } else { return \"one\" } }",
    );
    assert!(
        explicit_union.is_empty(),
        "a matching union annotation should accept both branches: {explicit_union:?}"
    );
}

#[test]
fn function_fallthrough_contributes_unit_to_inferred_return_union() {
    let missing_annotation = typecheck_source("fn maybe(flag: Bool) { if flag { return 1 } }");
    assert!(
        missing_annotation.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::TypeMismatch
                && diagnostic.message.contains("declare an explicit union return type")
        }),
        "the fallthrough path contributes Unit beside Num: {missing_annotation:?}"
    );

    let compatible = typecheck_source(
        "fn maybe(flag: Bool) -> Num | Unit { if flag { return 1 } }\nfn no_op() { let value = 1 }\nfn also_no_op() { return }",
    );
    assert!(
        compatible.is_empty(),
        "an explicit Unit union and ordinary Unit-only functions should typecheck: {compatible:?}"
    );
}

#[test]
fn gradual_any_branch_does_not_require_an_explicit_union() {
    let diagnostics = typecheck_source(
        "fn choose(value: Any, flag: Bool) { if flag { return value } else { return 1 } }",
    );
    assert!(
        diagnostics.is_empty(),
        "Any remains a gradual supertype for inferred branches: {diagnostics:?}"
    );
}

#[test]
fn local_function_shadows_std_callable_for_direct_and_aliased_calls() {
    let diagnostics = typecheck_source(
        "fn sweep(value: Num) { value }\nlet direct = sweep(1)\nlet op = sweep\nlet aliased = op(2)\n",
    );
    assert!(
        diagnostics.is_empty(),
        "local function calls should use the declared signature: {diagnostics:?}"
    );
}

#[test]
fn explicitly_qualified_std_callable_uses_standard_signature() {
    let diagnostics = typecheck_source("let result = std::sin(value: 0)\n");
    assert!(
        diagnostics.is_empty(),
        "`std::` should resolve the standard descriptor: {diagnostics:?}"
    );
}

#[test]
fn function_alias_preserves_named_and_default_parameters() {
    let diagnostics = typecheck_source(
        "fn add(a: Num, b: Num = 2) { a + b }\nlet op = add\nlet result = op(b: 4, a: 3)\n",
    );
    assert!(
        diagnostics.is_empty(),
        "valid named/default alias call should typecheck: {diagnostics:?}"
    );
}

#[test]
fn function_alias_rejects_unknown_and_duplicate_named_arguments() {
    let diagnostics = typecheck_source(
        "fn add(a: Num, b: Num = 2) { a + b }\nlet op = add\nlet result = op(a: 3, extra: 4)\n",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("has no parameter named 'extra'")),
        "unknown named parameter should fail through the callable binder: {diagnostics:?}"
    );

    let path = PathBuf::from("/virtual/duplicate.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(path.clone(), "fn add(a: Num, b: Num = 2) { a + b }\nlet op = add\n");
    let mut program = graph.load_program(&path).expect("source should load");
    program.statements.push(Stmt::LetDecl {
        is_pub: false,
        name: "result".into(),
        value: Expr::Call(
            Box::new(Expr::Ident("op".into())),
            vec![
                crate::ast::CallArgument::named("a", Expr::Num(3.0)),
                crate::ast::CallArgument::named("a", Expr::Num(4.0)),
            ],
        ),
        span: None,
    });
    let diagnostics = program.typecheck();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("was supplied more than once")),
        "duplicate names should fail through the shared callable binder: {diagnostics:?}"
    );
}

#[test]
fn imported_function_alias_preserves_named_and_default_parameters() {
    let main_path = PathBuf::from("/virtual/main.amx");
    let library_path = PathBuf::from("/virtual/math.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        main_path.clone(),
        "import \"./math.amx\" as math\nlet op = math::add\nlet result = op(b: 4, a: 3)\n",
    );
    graph.upsert_source(library_path, "pub fn add(a: Num, b: Num = 2) { a + b }\n");
    let mut program = graph.load_program(&main_path).expect("sources should load");
    let diagnostics = program.typecheck();
    assert!(
        diagnostics.is_empty(),
        "valid imported alias call should typecheck: {diagnostics:?}"
    );
}

#[test]
fn discarded_timeline_call_in_a_function_is_diagnosed() {
    let diagnostics = typecheck_source(
        "fn invalid(dot: Actor) { fade_in(target: dot, duration: 100ms)\n let x = 1 }",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::DiscardedTimelineValue),
        "a non-tail timeline expression must not be silently discarded: {diagnostics:?}"
    );
}

#[test]
fn ordinary_functions_reject_unwrapped_timeline_statements() {
    let diagnostics = typecheck_source("fn invalid() { box: Rect }");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::InvalidFunctionBodyStatement
                && diagnostic.message.contains("inside `#{ ... }`")
        }),
        "a direct actor declaration must not silently disappear from an ordinary function: {diagnostics:?}"
    );
}

#[test]
fn ordinary_functions_keep_pure_for_loops() {
    let diagnostics = typecheck_source("fn inspect() { for item in {1, 2} { let copied = item } }");
    assert!(diagnostics.is_empty(), "pure loop should remain legal: {diagnostics:?}");
}

#[test]
fn timeline_values_are_composable_return_values() {
    let diagnostics = typecheck_source(
        "fn composition(dot: Actor) -> Timeline { #{ fade_in(target: dot, duration: 100ms) } }",
    );
    assert!(
        diagnostics.is_empty(),
        "expected a valid reusable timeline plan: {diagnostics:?}"
    );
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
fn typecheck_component_method_param_types() {
    for (value, expected_mismatch) in [
        (Expr::Str("too big".to_string()), true),
        (Expr::Num(1.5), false),
    ] {
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
                methods: {
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
                            is_pub: false,
                            definition_path: PathBuf::new(),
                            span: None,
                            instance_index: None,
                            component_captures: Vec::new(),
                        },
                    );
                    actions
                },
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));

        let stmts = vec![
            actor("btn", "Button"),
            Stmt::Expr(
                Expr::Call(
                    Box::new(Expr::Field(Box::new(Expr::Ident("btn".into())), "pulse".into())),
                    vec![crate::ast::CallArgument::named("scale", value)],
                ),
                None,
            ),
        ];
        let diagnostics = env.check_statements(&stmts);
        if expected_mismatch {
            assert_eq!(diagnostics.len(), 1);
            assert!(diagnostics[0].message.contains("Type mismatch"));
            assert!(diagnostics[0].message.contains("scale"));
        } else {
            assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
        }
    }
}

#[test]
fn type_alias_resolves_in_component_param() {
    // Cases:
    // 1) alias declared before the component definition
    // 2) alias declared after the component definition
    for alias_first in [true, false] {
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
                methods: HashMap::new(),
            },
        );
        let empty_actions = HashMap::new();
        let mut env = TypeEnv::new(&components, &empty_actions);
        let alias_stmt = Stmt::TypeAlias {
            is_pub: true,
            name: "Metric".to_string(),
            annotation: TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
            span: None,
        };
        let comp_stmt = Stmt::ComponentDef(def.clone(), None);
        let actor_stmt = actor_with_props(
            "card",
            "Card",
            vec![Property::new("value", Expr::Str("Revenue".to_string()))],
        );
        let stmts = if alias_first {
            vec![alias_stmt, comp_stmt, actor_stmt]
        } else {
            vec![comp_stmt, alias_stmt, actor_stmt]
        };
        let diagnostics = env.check_statements(&stmts);
        assert!(
            diagnostics.is_empty(),
            "Expected no errors (alias_first: {alias_first}), got: {:?}",
            diagnostics
        );
    }
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
        let methods = crate::module::discovery::collect_component_functions(&def, &PathBuf::new());
        components.insert(
            def.name.clone(),
            ComponentEntry {
                definition: def,
                source_path: std::path::PathBuf::new(),
                methods,
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
            methods: HashMap::new(),
        },
    );
    let mut env =
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
    let stmts = vec![actor_with_props(
        "s",
        "Swatches",
        vec![Property::new(
            "colors",
            Expr::List(vec![
                Expr::Ident("red".to_string()),
                Expr::Ident("blue".to_string()),
            ]),
        )],
    )];
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "named color list should satisfy List<Color>, got: {:?}",
        diagnostics
    );
}

#[test]
fn rich_type_annotations_accept_matching_values() {
    let mut env = make_rich_probe_env();
    let stmts = vec![actor_with_props(
        "probe",
        "Probe",
        vec![
            Property::new("p", Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)])),
            Property::new("pair", Expr::Tuple(vec![Expr::Str("x".to_string()), Expr::Num(1.0)])),
            Property::new("mapper", Expr::Closure(vec!["x".to_string()], Box::new(Expr::Num(1.0)))),
        ],
    )];
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "matching rich annotations should typecheck, got: {:?}",
        diagnostics
    );
}

#[test]
fn rich_type_annotations_reject_mismatched_values() {
    let mut env = make_rich_probe_env();
    let stmts = vec![actor_with_props(
        "probe",
        "Probe",
        vec![
            Property::new("p", Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)])),
            Property::new("pair", Expr::Tuple(vec![Expr::Num(1.0), Expr::Str("x".to_string())])),
            Property::new(
                "mapper",
                Expr::Closure(vec!["x".to_string()], Box::new(Expr::Str("bad".to_string()))),
            ),
        ],
    )];
    let diagnostics = env.check_statements(&stmts);
    assert_eq!(diagnostics.len(), 3, "expected three type mismatches, got: {:?}", diagnostics);
    assert!(diagnostics.iter().all(|d| d.code == DiagnosticCode::TypeMismatch));
}

#[test]
fn colorscheme_path_accepted_for_color_param() {
    // accent.primary should be accepted where a Color param is expected
    let env = make_env(); // Button has a 'color: Color' param
    let stmts = vec![actor_with_props(
        "btn",
        "Button",
        vec![Property::new(
            "color",
            Expr::Path(vec!["accent".to_string(), "primary".to_string()]),
        )],
    )];
    let mut env = env;
    let diagnostics = env.check_statements(&stmts);
    assert!(
        diagnostics.is_empty(),
        "accent.primary should be accepted for Color param, got: {:?}",
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
            methods: HashMap::new(),
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
