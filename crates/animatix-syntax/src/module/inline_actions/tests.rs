use super::*;

use crate::ast::{ParamDef, TypeAnnotation};
use std::path::PathBuf;

fn make_action(verb: &str, target: &str, modifiers: Vec<Modifier>) -> Stmt {
    Stmt::Action(
        Action {
            verb: verb.to_string(),
            targets: vec![target.to_string()],
            target_index: vec![None],
            target_index_position: vec![],
            args: vec![],
            modifiers,
            byte_span: None,
        },
        None,
    )
}

fn make_arg_call(verb: &str, args: Vec<Expr>) -> Stmt {
    Stmt::Action(
        Action {
            verb: verb.to_string(),
            targets: vec![],
            target_index: vec![],
            target_index_position: vec![],
            args,
            modifiers: vec![],
            byte_span: None,
        },
        None,
    )
}

fn make_assignment(target: &str, property: &str, value: Expr) -> Stmt {
    Stmt::Assignment {
        target: vec![crate::ast::TargetSegment::Static(target.to_string())],
        property: property.to_string(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    }
}

fn template_with(params: Vec<ParamDef>, body: Vec<Stmt>) -> FnTemplate {
    FnTemplate {
        params,
        return_type: None,
        body,
        instance_index: None,
    }
}

#[test]
fn target_call_expands_to_block_with_param_binding() {
    let template = template_with(
        vec![ParamDef {
            name: "scale".to_string(),
            param_type: Some(TypeAnnotation::Num),
            default: Some(Expr::Num(1.2)),
        }],
        vec![make_assignment(
            "self",
            "scale",
            Expr::Ident("scale".to_string()),
        )],
    );
    let registry: InstanceFnRegistry =
        [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
            .into_iter()
            .collect();
    let invocation = make_action(
        "pulse",
        "btn",
        vec![Modifier {
            name: Some("scale".to_string()),
            value: Expr::Num(1.5),
        }],
    );
    let module_fns: HashMap<String, FnTemplate> = HashMap::new();
    let result = expand_stmt_list(
        vec![invocation],
        &registry,
        &module_fns,
        &std::collections::HashSet::new(),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .0;
    assert_eq!(result.len(), 1);
    match &result[0] {
        Stmt::Block { body, .. } => {
            // `scale` is a value parameter bound by `let`; `self` is
            // rewritten to the concrete target.
            assert!(matches!(&body[0], Stmt::LetDecl { name, .. } if name == "scale"));
            assert!(matches!(
                &body[1],
                Stmt::Assignment { target, property, value, .. }
                    if target == &[TargetSegment::Static("btn".to_string())]
                        && property == "scale"
                        && value == &Expr::Ident("scale".to_string())
            ));
        },
        other => panic!("expected Block, got {:?}", other),
    }
}

#[test]
fn arg_call_expands_positional_params() {
    let template = template_with(
        vec![
            ParamDef {
                name: "bars".to_string(),
                param_type: None,
                default: None,
            },
            ParamDef {
                name: "key".to_string(),
                param_type: None,
                default: None,
            },
        ],
        vec![make_assignment(
            "note",
            "target",
            Expr::Path(vec!["bars".to_string(), "bar".to_string()]),
        )],
    );
    let module_fns: HashMap<String, FnTemplate> =
        [("highlight_key".to_string(), template)].into_iter().collect();
    let invocation =
        make_arg_call("highlight_key", vec![Expr::Ident("bars".to_string()), Expr::Num(2.0)]);
    let registry: InstanceFnRegistry = HashMap::new();
    let result = expand_stmt_list(
        vec![invocation],
        &registry,
        &module_fns,
        &std::collections::HashSet::new(),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .0;
    assert_eq!(result.len(), 1);
    match &result[0] {
        Stmt::Block { body, .. } => {
            // Positional args bind `bars` and `key` as value parameters.
            assert!(matches!(&body[0], Stmt::LetDecl { name, .. } if name == "bars"));
            assert!(matches!(&body[1], Stmt::LetDecl { name, .. } if name == "key"));
            assert!(matches!(
                &body[2],
                Stmt::Assignment { target, property, .. }
                    if target == &[TargetSegment::Static("note".to_string())]
                        && property == "target"
            ));
        },
        other => panic!("expected Block, got {:?}", other),
    }
}

#[test]
fn actor_parameter_targets_substitute_recursively_inside_local_keyframes() {
    let template = template_with(
        vec![ParamDef {
            name: "dot".to_string(),
            param_type: None,
            default: None,
        }],
        vec![Stmt::Keyframe {
            time: crate::ast::Time::Milliseconds(0),
            body: vec![
                make_assignment("dot", "at", Expr::Num(1.0)),
                Stmt::Always {
                    body: vec![make_assignment("dot", "at", Expr::Num(2.0))],
                    span: None,
                },
            ],
            span: None,
        }],
    );
    let module_fns: HashMap<String, FnTemplate> =
        [("ticker".to_string(), template)].into_iter().collect();
    let result = expand_fn_calls(
        vec![make_arg_call("ticker", vec![Expr::Ident("needle".into())])],
        &HashMap::new(),
        &module_fns,
        &mut Vec::new(),
    );

    let [Stmt::Block { body, .. }] = result.as_slice() else {
        panic!("expected one expanded function block, got {result:?}")
    };
    let [
        Stmt::Keyframe {
            body: frame_body, ..
        },
    ] = body.as_slice()
    else {
        panic!("expected one local keyframe, got {body:?}")
    };
    assert!(matches!(
        &frame_body[0],
        Stmt::Assignment { target, .. }
            if target == &[TargetSegment::Static("needle".to_string())]
    ));
    let Stmt::Always {
        body: always_body, ..
    } = &frame_body[1]
    else {
        panic!("expected Always inside the local keyframe")
    };
    assert!(matches!(
        &always_body[0],
        Stmt::Assignment { target, .. }
            if target == &[TargetSegment::Static("needle".to_string())]
    ));

    let nested_result = expand_fn_calls(
        vec![make_arg_call(
            "ticker",
            vec![Expr::Path(vec!["graph".into(), "needle".into()])],
        )],
        &HashMap::new(),
        &module_fns,
        &mut Vec::new(),
    );
    let [Stmt::Block { body, .. }] = nested_result.as_slice() else {
        panic!("expected one expanded nested-label call, got {nested_result:?}")
    };
    let [Stmt::Keyframe { body, .. }] = body.as_slice() else {
        panic!("expected one local keyframe, got {body:?}")
    };
    assert!(matches!(
        &body[0],
        Stmt::Assignment { target, .. }
            if target == &[
                TargetSegment::Static("graph".to_string()),
                TargetSegment::Static("needle".to_string()),
            ]
    ));
}

#[test]
fn actor_parameter_substitution_only_rewrites_the_root_target_segment() {
    let stmt = Stmt::Assignment {
        target: vec![
            TargetSegment::Static("other".into()),
            TargetSegment::Static("dot".into()),
        ],
        property: "at".into(),
        value: Expr::Num(1.0),
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    };
    let bindings = HashMap::from([("dot".to_string(), Expr::Ident("needle".into()))]);

    let rewritten = substitute_params_in_stmt(&stmt, &bindings);
    assert!(matches!(
        rewritten,
        Stmt::Assignment { target, .. }
            if target.as_slice() == [
                TargetSegment::Static("other".into()),
                TargetSegment::Static("dot".into()),
            ]
    ));
}

#[test]
fn real_local_ticker_expansion_has_no_unsubstituted_actor_parameter() {
    let path = PathBuf::from("/virtual/local_tickers.amx");
    let mut graph = crate::module::ModuleGraph::new()
        .with_source_access(crate::module::SourceAccess::SourcesOnly);
    graph.upsert_source(
        path.clone(),
        include_str!("../../../../../examples/animation/local_tickers.amx"),
    );
    let program = graph.load_program(&path).expect("local ticker example loads");
    let mut diagnostics = Vec::new();
    let expanded = program.expand_components(&mut diagnostics);
    assert!(diagnostics.is_empty(), "unexpected expansion diagnostics: {diagnostics:?}");

    fn collect_unresolved_targets(stmt: &Stmt, unresolved: &mut Vec<String>) {
        // The declaration intentionally retains its formal `dot` parameter;
        // inspect only the expanded call sites.
        if matches!(stmt, Stmt::FnDecl { .. }) {
            return;
        }
        match stmt {
            Stmt::Assignment { target, .. } | Stmt::ReactiveBinding { target, .. }
                if target.iter().any(|segment| segment.label_str() == "dot") =>
            {
                unresolved.push(format!("{stmt:?}"));
            },
            Stmt::Action(action, _)
                if action
                    .targets
                    .iter()
                    .any(|target| target == "dot" || target.starts_with("dot.")) =>
            {
                unresolved.push(format!("{stmt:?}"));
            },
            _ => {},
        }
        crate::walk::recurse_stmt_bodies(stmt, &mut |body| {
            for child in body {
                collect_unresolved_targets(child, unresolved);
            }
        });
    }
    let mut unresolved = Vec::new();
    for stmt in &expanded {
        collect_unresolved_targets(stmt, &mut unresolved);
    }
    assert!(unresolved.is_empty(), "actor parameter survived expansion: {unresolved:?}");
}

#[test]
fn recursion_cycle_is_diagnosed_and_dropped() {
    let template = template_with(vec![], vec![make_arg_call("b", Vec::new())]);
    let module_fns: HashMap<String, FnTemplate> = [
        ("a".to_string(), template.clone()),
        ("b".to_string(), template),
    ]
    .into_iter()
    .collect();
    let invocation = make_arg_call("a", Vec::new());
    let registry: InstanceFnRegistry = HashMap::new();
    // The cycle pre-pass reports the recursion and the expander drops the
    // calls, so the run must terminate without infinite recursion or a
    // surviving "unknown action".
    let mut diagnostics = Vec::new();
    let result = expand_fn_calls(vec![invocation], &registry, &module_fns, &mut diagnostics);
    assert!(
        diagnostics.iter().any(|d| d.contains("recursive")),
        "expected a recursion diagnostic, got: {diagnostics:?}"
    );
    assert!(
        !result.iter().any(|s| matches!(s, Stmt::Action(..))),
        "cycle calls must be dropped, got: {result:?}"
    );
}

#[test]
fn pure_function_call_is_not_expanded() {
    let template = FnTemplate {
        params: vec![],
        return_type: Some(TypeAnnotation::Num),
        body: vec![],
        instance_index: None,
    };
    let module_fns: HashMap<String, FnTemplate> =
        [("dnf".to_string(), template)].into_iter().collect();
    let invocation = make_arg_call("dnf", vec![Expr::Ident("arr".to_string())]);
    let registry: InstanceFnRegistry = HashMap::new();
    let mut diagnostics = Vec::new();
    let result = expand_stmt_list(
        vec![invocation],
        &registry,
        &module_fns,
        &std::collections::HashSet::new(),
        &mut Vec::new(),
        &mut diagnostics,
    )
    .0;
    // The pure call is diagnosed and dropped (no "unknown action" at runtime).
    assert!(
        diagnostics.iter().any(|d| d.contains("must be called in an expression")),
        "expected a pure-function statement diagnostic, got: {diagnostics:?}"
    );
    assert!(result.is_empty(), "pure statement call must be dropped");
}

#[test]
fn lexical_index_rebinding_respects_params_local_lets_and_nested_blocks() {
    let body = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "local".into(),
            value: Expr::Num(1.0),
            span: None,
        },
        Stmt::Block {
            body: vec![
                Stmt::LetDecl {
                    is_pub: false,
                    name: "i".into(),
                    value: Expr::Num(0.5),
                    span: None,
                },
                make_assignment("cards", "opacity", Expr::Ident("i".into())),
            ],
            instance_scope: None,
            time_scope: Some(crate::ast::TimeScopeSpec {
                time_scale: Some(Expr::Ident("i".into())),
                clip_duration: None,
                end_limit_duration_ms: None,
            }),
            span: None,
        },
        make_assignment("cards", "rotation", Expr::Ident("i".into())),
    ];
    let rebound = super::indexed::rebind_template_index(
        body,
        "cards",
        &Expr::Ident("i".into()),
        &Expr::Num(2.0),
        Some(0),
        None,
        &[],
    );
    let Stmt::Block {
        time_scope: Some(scope),
        ..
    } = &rebound[1]
    else {
        panic!("expected the nested time-scope Block to be preserved")
    };
    assert_eq!(
        scope.time_scale,
        Some(Expr::Ident("i".into())),
        "caller-scoped retiming expressions must not be rebound to the component index"
    );
    let Stmt::Block { body: inner, .. } = &rebound[1] else {
        panic!("expected nested lexical block");
    };
    assert!(matches!(
        &inner[1],
        Stmt::Assignment { value: Expr::Ident(name), .. } if name == "i"
    ));
    assert!(matches!(
        &rebound[2],
        Stmt::Assignment { value: Expr::Num(index), .. } if *index == 2.0
    ));

    let param_body = vec![make_assignment("cards", "opacity", Expr::Ident("i".into()))];
    let param_rebound = super::indexed::rebind_template_index(
        param_body,
        "cards",
        &Expr::Ident("i".into()),
        &Expr::Num(2.0),
        Some(0),
        None,
        &[ParamDef {
            name: "i".into(),
            param_type: None,
            default: None,
        }],
    );
    assert!(matches!(
        &param_rebound[0],
        Stmt::Assignment { value: Expr::Ident(name), .. } if name == "i"
    ));
}

#[test]
fn literal_instance_index_does_not_rewrite_equal_numeric_values() {
    let body = vec![make_assignment("cards", "opacity", Expr::Num(2.0))];
    let rebound = super::indexed::rebind_template_index(
        body,
        "cards",
        &Expr::Num(2.0),
        &Expr::Num(5.0),
        Some(0),
        None,
        &[],
    );
    assert!(matches!(
        &rebound[0],
        Stmt::Assignment { value: Expr::Num(value), target, .. }
            if *value == 2.0
                && matches!(&target[0], TargetSegment::Indexed { index, .. } if index.as_ref() == &Expr::Num(5.0))
    ));
}

#[test]
fn indexed_instance_read_specialization_respects_scope_and_nested_instance_blocks() {
    let source_read = Expr::Path(vec!["cards".into(), "heading".into(), "color".into()]);
    let nested_read = Expr::Path(vec!["cards".into(), "heading".into(), "opacity".into()]);
    let nested_body_read = Expr::Path(vec![
        "cards".into(),
        "badge".into(),
        "shape".into(),
        "color".into(),
    ]);
    let body = vec![
        make_assignment("cards.bar", "color", source_read.clone()),
        Stmt::Block {
            body: vec![
                make_assignment("cards.badge", "opacity", nested_read.clone()),
                make_assignment("cards.badge.shape", "color", nested_body_read.clone()),
            ],
            instance_scope: Some(crate::ast::InstanceScope {
                label: "cards.badge".into(),
                index: nested_read.clone(),
                index_position: 0,
                prologue_len: 1,
            }),
            time_scope: None,
            span: None,
        },
    ];

    let specialized =
        super::indexed::specialize_instance_property_reads(body, "cards", "cards__1", &[]);
    assert!(matches!(
        &specialized[0],
        Stmt::Assignment { value: Expr::Path(parts), .. }
            if parts.iter().map(String::as_str).eq(["cards__1", "heading", "color"])
    ));
    let Stmt::Block {
        body: nested,
        instance_scope: Some(scope),
        ..
    } = &specialized[1]
    else {
        panic!("expected nested indexed method block")
    };
    assert!(matches!(
        &nested[0],
        Stmt::Assignment { value: Expr::Path(parts), .. }
            if parts.iter().map(String::as_str).eq(["cards__1", "heading", "opacity"])
    ));
    assert!(matches!(
        &nested[1],
        Stmt::Assignment { value: Expr::Path(parts), .. }
            if parts.iter().map(String::as_str).eq(["cards", "badge", "shape", "color"])
    ));
    assert_eq!(
        scope.index,
        Expr::Path(vec!["cards__1".into(), "heading".into(), "opacity".into()])
    );
}

#[test]
fn indexed_instance_read_specialization_honors_shadowing() {
    let body = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "cards".into(),
            value: Expr::Num(1.0),
            span: None,
        },
        make_assignment(
            "target",
            "color",
            Expr::Path(vec!["cards".into(), "heading".into(), "color".into()]),
        ),
    ];
    let specialized =
        super::indexed::specialize_instance_property_reads(body, "cards", "cards__1", &[]);
    assert!(matches!(
        &specialized[1],
        Stmt::Assignment { value: Expr::Path(parts), .. }
            if parts.iter().map(String::as_str).eq(["cards", "heading", "color"])
    ));

    let bound_names = vec!["cards".to_string()];
    let specialized = super::indexed::specialize_instance_property_reads(
        vec![make_assignment(
            "target",
            "color",
            Expr::Path(vec!["cards".into(), "heading".into(), "color".into()]),
        )],
        "cards",
        "cards__1",
        &bound_names,
    );
    assert!(matches!(
        &specialized[0],
        Stmt::Assignment { value: Expr::Path(parts), .. }
            if parts.iter().map(String::as_str).eq(["cards", "heading", "color"])
    ));
}
