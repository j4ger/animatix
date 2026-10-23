use super::*;

mod tests {
    use super::*;
    use crate::ast::{ParamDef, TypeAnnotation};

    fn make_action(verb: &str, target: &str, modifiers: Vec<Modifier>) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec![target.to_string()],
                target_index: vec![None],
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
}
