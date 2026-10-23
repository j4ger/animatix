use super::*;

#[test]
fn test_closure_parser() {
    let input = "let f = (x) => x ^ 2";
    let res = parse_simple(input).0.unwrap();

    // Find the LetDecl stmt
    if let Stmt::LetDecl {
        is_pub,
        name,
        value,
        ..
    } = &res[0]
    {
        assert!(!(*is_pub));
        assert_eq!(name, "f");
        assert_eq!(
            *value,
            Expr::Closure(
                vec!["x".to_string()],
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Pow,
                    Box::new(Expr::Num(2.0))
                ))
            )
        );
    } else {
        panic!("Expected LetDecl");
    }
}

#[test]
fn test_text_shorthand_parser() {
    let input = r#"a: "hello world""#;
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl {
        is_pub,
        label,
        ty,
        props,
        modifiers,
        children,
        ..
    } = &res[0]
    {
        assert!(!(*is_pub));
        assert_eq!(label, "a");
        assert_eq!(ty, "Text");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "text");
        assert_eq!(props[0].value, Expr::Str("hello world".to_string()));
        assert!(modifiers.is_empty());
        assert!(children.is_empty());
    } else {
        panic!("Expected ActorDecl, got {:?}", res[0]);
    }
}

#[test]
fn test_text_shorthand_with_modifiers() {
    let input = r#"title: "Slide 1" [2s, ease: ease-in-out]"#;
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl {
        label,
        ty,
        props,
        modifiers,
        ..
    } = &res[0]
    {
        assert_eq!(label, "title");
        assert_eq!(ty, "Text");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "text");
        assert_eq!(props[0].value, Expr::Str("Slide 1".to_string()));
        assert_eq!(modifiers.len(), 2);
    } else {
        panic!("Expected ActorDecl, got {:?}", res[0]);
    }
}

#[test]
fn test_typst_shorthand_parser() {
    let input = "eq: $$ x^2 + y^2 $$";
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl {
        is_pub,
        label,
        ty,
        props,
        modifiers,
        children,
        ..
    } = &res[0]
    {
        assert!(!(*is_pub));
        assert_eq!(label, "eq");
        assert_eq!(ty, "Typst");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "content");
        assert_eq!(props[0].value, Expr::Str("x^2 + y^2".to_string()));
        assert!(modifiers.is_empty());
        assert!(children.is_empty());
    } else {
        panic!("Expected ActorDecl, got {:?}", res[0]);
    }
}

#[test]
fn test_typst_shorthand_with_modifiers() {
    let input = "eq: $$ x^2 $$ [2s]";
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl {
        label,
        ty,
        props,
        modifiers,
        ..
    } = &res[0]
    {
        assert_eq!(label, "eq");
        assert_eq!(ty, "Typst");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "content");
        assert_eq!(props[0].value, Expr::Str("x^2".to_string()));
        assert_eq!(modifiers.len(), 1);
    } else {
        panic!("Expected ActorDecl, got {:?}", res[0]);
    }
}

#[test]
fn test_vec2_value_span_accuracy() {
    // Reproduce the bug: size: (2494.552, 1377.7778) should have correct span
    let input = r#"backdrop: Rect, size: (2494.552, 1377.7778), color: scene.background"#;
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl { props, .. } = &res[0] {
        let size_prop = props.iter().find(|p| p.name == "size").unwrap();
        let span = size_prop.value_span.unwrap();

        // The value in source is "(2494.552, 1377.7778)"
        // Find its actual position in the input
        let value_start = input.find("(2494.552").unwrap();
        let value_end = input.find("1377.7778)").unwrap() + "1377.7778)".len();

        assert_eq!(span.start, value_start, "span start mismatch");
        assert_eq!(span.end, value_end, "span end mismatch");

        // Verify the span extracts the correct text
        let extracted = &input[span.start..span.end];
        assert_eq!(extracted, "(2494.552, 1377.7778)", "span extracts wrong text");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_vec2_value_span_with_trailing_comma() {
    // Test with trailing comma in tuple: (2494.552, 1377.7778,)
    let input = r#"backdrop: Rect, size: (2494.552, 1377.7778,), color: scene.background"#;
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl { props, .. } = &res[0] {
        let size_prop = props.iter().find(|p| p.name == "size").unwrap();
        let span = size_prop.value_span.unwrap();

        // The value in source is "(2494.552, 1377.7778,)"
        let value_start = input.find("(2494.552").unwrap();
        let value_end = input.find("1377.7778,)").unwrap() + "1377.7778,)".len();

        assert_eq!(span.start, value_start, "span start mismatch");
        assert_eq!(span.end, value_end, "span end mismatch");

        let extracted = &input[span.start..span.end];
        assert_eq!(extracted, "(2494.552, 1377.7778,)", "span extracts wrong text");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_multiple_properties_span_independence() {
    // Test that spans for multiple properties don't overlap
    let input = r#"backdrop: Rect, size: (100, 200), color: red, anchor: center"#;
    let res = parse_simple(input).0.unwrap();

    if let Stmt::ActorDecl { props, .. } = &res[0] {
        let size_prop = props.iter().find(|p| p.name == "size").unwrap();
        let color_prop = props.iter().find(|p| p.name == "color").unwrap();
        let anchor_prop = props.iter().find(|p| p.name == "anchor").unwrap();

        let size_span = size_prop.value_span.unwrap();
        let color_span = color_prop.value_span.unwrap();
        let anchor_span = anchor_prop.value_span.unwrap();

        // Verify spans don't overlap
        assert!(size_span.end <= color_span.start, "size span overlaps color span");
        assert!(color_span.end <= anchor_span.start, "color span overlaps anchor span");

        // Verify extracted text
        let size_text = &input[size_span.start..size_span.end];
        let color_text = &input[color_span.start..color_span.end];
        let anchor_text = &input[anchor_span.start..anchor_span.end];

        assert_eq!(size_text, "(100, 200)");
        assert_eq!(color_text, "red");
        assert_eq!(anchor_text, "center");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_reactive_binding_parser() {
    let input = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
    let res = parse_simple(input).0.unwrap();
    assert_eq!(res.len(), 1);
    // Reactive bindings are not wrapped in a default keyframe
    if let Stmt::ReactiveBinding {
        target,
        property,
        value,
        ..
    } = &res[0]
    {
        assert_eq!(target, &[TargetSegment::Static("orbiter".to_string())]);
        assert_eq!(property, "at");
        // Verify it's a binary expression (tracker.at + (...))
        if let Expr::Binary(left, BinaryOp::Add, _right) = value {
            if let Expr::Path(parts) = left.as_ref() {
                assert_eq!(parts, &["tracker", "at"]);
            } else {
                panic!("Expected Path for left side");
            }
        } else {
            panic!("Expected Binary Add expression");
        }
    } else {
        panic!("Expected ReactiveBinding");
    }
}

#[test]
fn test_reactive_binding_rejects_single_segment() {
    let input = r#"at := (100, 200)"#;
    let (_, errors) = parse_source(input);
    assert!(!errors.is_empty(), "Expected parse error for single-segment reactive binding");
}

#[test]
fn test_comments_inside_delimiters() {
    let input = r#"
            circ = Circle {
                at: (0, 0), // center
                radius: 100, // size
            }
        "#;
    let (ast, errors) = parse_source(input);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let ast = ast.expect("parsed AST");
    assert_eq!(ast.len(), 1);
    if let Stmt::Assignment {
        property, value, ..
    } = &ast[0]
    {
        assert_eq!(property, "circ");
        if let Expr::Construct(name, props) = value {
            assert_eq!(name, "Circle");
            assert_eq!(props.len(), 2);
        } else {
            panic!("Expected Construct value");
        }
    } else {
        panic!("Expected Assignment, got {:?}", ast[0]);
    }
}

#[test]
fn test_comment_strip_preserves_strings() {
    let input = r#"
            label = Text {
                text: "Visit https://example.com // not a comment",
            }
        "#;
    let (ast, errors) = parse_source(input);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors);
    let ast = ast.expect("parsed AST");
    if let Stmt::Assignment { value, .. } = &ast[0] {
        if let Expr::Construct(_, props) = value {
            if let Expr::Str(text) = &props[0].value {
                assert_eq!(text, "Visit https://example.com // not a comment");
            } else {
                panic!("Expected string value");
            }
        } else {
            panic!("Expected Construct");
        }
    } else {
        panic!("Expected Assignment");
    }
}

#[test]
fn test_parse_error_includes_context() {
    // Trigger a parse error inside an actor declaration to verify context
    let input = r#"
            circ: Circle {
                at: (0, 0),
                radius: // missing value
            }
        "#;
    let (_ast, errors) = parse_source(input);
    assert!(!errors.is_empty(), "Expected parse errors");
    // The error should include context about being in an actor declaration
    let has_context =
        errors.iter().any(|e| e.context.iter().any(|c| c.contains("actor declaration")));
    assert!(
        has_context,
        "Expected error context to include 'actor declaration', got: {:?}",
        errors
    );
}

// ── Snippet tab-stop tests ──

#[test]
fn strip_tabstops_simple() {
    let input = "${1:label}: ${2:Text} {}";
    let result = strip_snippet_tabstops(input);
    assert_eq!(result, "label: Text {}");
}

#[test]
fn strip_tabstops_nested_braces() {
    let input = "${1:label}: ${2:Text} {\n    ${3:content}: \"${4:}\",\n}";
    let result = strip_snippet_tabstops(input);
    assert_eq!(result, "label: Text {\n    content: \"\",\n}");
}

#[test]
fn strip_tabstops_no_default() {
    let input = "# ${1:0s}\n${2}";
    let result = strip_snippet_tabstops(input);
    assert_eq!(result, "# 0s\n");
}

#[test]
fn strip_tabstops_preserves_dollar_signs() {
    let input = "price: \"$100\"";
    let result = strip_snippet_tabstops(input);
    assert_eq!(result, "price: \"$100\"");
}

#[test]
fn parse_snippet_actor_inline_props() {
    // AMX uses comma-separated inline properties, not braces
    let snippet = "${1:label}: ${2:Text}, ${3:content}: \"${4:Hello}\"";
    let stmts = parse_snippet(snippet);
    assert!(stmts.is_some(), "snippet should parse");
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl {
        label, ty, props, ..
    } = &stmts[0]
    {
        assert_eq!(label, "label");
        assert_eq!(ty, "Text");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "content");
    } else {
        panic!("Expected ActorDecl, got {:?}", stmts[0]);
    }
}

#[test]
fn parse_snippet_actor_with_children() {
    // AMX children (nested items) use braces
    let snippet = "${1:container}: ${2:Row} {\n    ${3:child}: ${4:Text}, content: \"${5:Hi}\"\n}";
    let stmts = parse_snippet(snippet);
    assert!(stmts.is_some(), "snippet should parse");
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl {
        label,
        ty,
        children,
        ..
    } = &stmts[0]
    {
        assert_eq!(label, "container");
        assert_eq!(ty, "Row");
        assert_eq!(children.len(), 1);
    } else {
        panic!("Expected ActorDecl with children, got {:?}", stmts[0]);
    }
}

#[test]
fn parse_snippet_keyframe() {
    let snippet = "# ${1:0s}\n${2}";
    let stmts = parse_snippet(snippet);
    assert!(stmts.is_some(), "keyframe snippet should parse");
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    assert!(
        matches!(&stmts[0], Stmt::Keyframe { .. }),
        "Expected Keyframe, got {:?}",
        stmts[0]
    );
}

#[test]
fn parse_snippet_always() {
    let snippet = "always {\n    ${1:}\n}";
    let stmts = parse_snippet(snippet);
    assert!(stmts.is_some(), "always snippet should parse");
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0], Stmt::Always { .. }), "Expected Always, got {:?}", stmts[0]);
}

#[test]
fn parse_snippet_stagger() {
    let snippet = "stagger [${1:150ms}] {\n    ${2:}\n}";
    let stmts = parse_snippet(snippet);
    assert!(stmts.is_some(), "stagger snippet should parse");
    let stmts = stmts.unwrap();
    // Parser wraps bare statements in a default keyframe (#0s)
    assert_eq!(stmts.len(), 1);
    if let Stmt::Keyframe { body, .. } = &stmts[0] {
        assert_eq!(body.len(), 1);
        assert!(
            matches!(&body[0], Stmt::Stagger { .. }),
            "Expected Stagger inside keyframe, got {:?}",
            body[0]
        );
    } else {
        // Also accept top-level stagger
        assert!(
            matches!(&stmts[0], Stmt::Stagger { .. }),
            "Expected Stagger or Keyframe, got {:?}",
            stmts[0]
        );
    }
}

#[test]
fn parse_action_with_dotted_target() {
    // Single dotted target: `highlight decomp_eq.f1 [800ms]`
    let result = parse_snippet("highlight decomp_eq.f1 [800ms]");
    assert!(result.is_some(), "dotted action target should parse");
    let stmts = result.unwrap();
    // May be wrapped in a default keyframe
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    assert!(action.is_some(), "expected Action stmt, got {:?}", stmts);
    if let Stmt::Action(action, _) = action.unwrap() {
        assert_eq!(action.verb, "highlight");
        assert_eq!(action.targets, vec!["decomp_eq.f1"]);
    }
}

#[test]
fn a_bare_action_does_not_swallow_the_next_lines_assignment() {
    // Regression: the grammar has no newline tokens, and an action statement
    // used to read *both* its target list and its argument list as unbounded
    // repeats. So `persist t` swallowed the next statement's left-hand side —
    // as a second target, or as an argument — and the `=` then failed to
    // parse. Measured before the fix: targets ["t", "t.opacity"], then
    // `expected expression, modifier list, statement, …, found '='`.
    let action_and_assignment = |source: &str| {
        let (stmts, errs) = parse_source(source);
        assert!(errs.is_empty(), "{source:?} should parse: {errs:?}");
        let stmts = stmts.expect("statements");
        let body = match &stmts[0] {
            Stmt::Keyframe { body, .. } => body,
            other => panic!("expected a keyframe, got {other:?}"),
        };
        assert_eq!(body.len(), 2, "expected the action and the assignment");
        match &body[0] {
            Stmt::Action(action, _) => {
                assert!(
                    !action.targets.iter().any(|t| t.contains('.')),
                    "the next line's left-hand side became a target: {:?}",
                    action.targets
                );
                assert!(
                    action.args.is_empty(),
                    "the next line's left-hand side became an argument: {:?}",
                    action.args
                );
            },
            other => panic!("expected an Action, got {other:?}"),
        }
        assert!(matches!(&body[1], Stmt::Assignment { .. }));
    };

    // One target, no comma; then a comma-separated list.
    action_and_assignment("#0s\npersist t\nt.opacity = 0.5 [200ms]");
    action_and_assignment("#0s\npersist a, b\na.opacity = 0.5 [200ms]");

    // The guard is on the statement boundary only. A space-separated target
    // list still runs on (`swap bar1 bar2`, pinned by
    // `test_swap_action_space_separated_targets`), so a bare particle like
    // `by` still lands in the target list rather than the argument list —
    // which is why `builtins.rs` advertises `rotate target [by: 90]`
    // instead. Pinning both halves keeps the fix from widening into a
    // language change.
    let (stmts, errs) = parse_source("#0s\nswap bar1 bar2 [500ms]");
    assert!(errs.is_empty(), "`swap` with a space-separated list: {errs:?}");
    let stmts = stmts.expect("statements");
    let body = match &stmts[0] {
        Stmt::Keyframe { body, .. } => body,
        other => panic!("expected a keyframe, got {other:?}"),
    };
    match &body[0] {
        Stmt::Action(action, _) => {
            assert_eq!(action.verb, "swap");
            assert_eq!(action.targets, vec!["bar1", "bar2"]);
            assert!(action.args.is_empty());
        },
        other => panic!("expected an Action, got {other:?}"),
    }
}

#[test]
fn test_method_call_with_args() {
    // Parse an always block with a method call: graph.map(mx, my)
    let input = "always { ball.at = descent_graph.map(mx, my) }";
    let (stmts, errs) = parse_source(input);
    assert!(errs.is_empty(), "Method call parse failed: {:?}", errs);
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Stmt::Always { body, .. } = &stmts[0] {
        // Body should have one assignment statement
        if let Stmt::Assignment { value, .. } = &body[0] {
            // The value should be a Method call: graph.map(mx, my)
            assert!(
                matches!(
                    &value,
                    Expr::Method(receiver, name, args)
                        if matches!(receiver.as_ref(), Expr::Ident(n) if n == "descent_graph")
                        && name == "map"
                        && args.len() == 2
                ),
                "Expected Method(Ident(descent_graph), 'map', [mx, my]), got {:?}",
                value
            );
        } else {
            panic!("Expected Assignment, got {:?}", body[0]);
        }
    } else {
        panic!("Expected Always, got {:?}", stmts[0]);
    }
}

#[test]
fn test_method_call_on_dot_path() {
    // Parse chained method: a.b.c(d)
    let input = "always { x = a.b.c(d) }";
    let (stmts, errs) = parse_source(input);
    assert!(errs.is_empty(), "Chained method parse failed: {:?}", errs);
    let stmts = stmts.unwrap();
    assert_eq!(stmts.len(), 1);
    if let Stmt::Always { body, .. } = &stmts[0] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert!(
                matches!(
                    &value,
                    Expr::Method(receiver, name, args)
                        if matches!(receiver.as_ref(), Expr::Path(parts) if parts == &["a", "b"])
                        && name == "c"
                        && args.len() == 1
                ),
                "Expected Method(Path([a,b]), 'c', [d]), got {:?}",
                value
            );
        } else {
            panic!("Expected Assignment, got {:?}", body[0]);
        }
    } else {
        panic!("Expected Always, got {:?}", stmts[0]);
    }
}

#[test]
fn test_field_access_still_works() {
    // Ensure bare field access (no parens) still produces Path
    let input = "always { x = descent_graph.at }";
    let (stmts, errs) = parse_source(input);
    assert!(errs.is_empty(), "Field access parse failed: {:?}", errs);
    let stmts = stmts.unwrap();
    if let Stmt::Always { body, .. } = &stmts[0] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert!(
                matches!(&value, Expr::Path(parts) if parts == &["descent_graph", "at"]),
                "Expected Path([descent_graph, at]), got {:?}",
                value
            );
        }
    }
}

#[test]
fn test_any_type_annotation_in_action_param() {
    // `Any` in a parameter type annotation should parse without error.
    let src = r#"fn transform(x: Any = 0) { fade-in x [300ms] }"#;
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::FnDecl { params, .. } = &stmts[0] {
        assert_eq!(params[0].param_type, Some(TypeAnnotation::Any));
    } else {
        panic!("expected FnDecl, got {:?}", stmts[0]);
    }
}

#[test]
fn test_union_type_annotation_in_action_param() {
    let src = r#"fn transform(x: Bool | Str = "x") { fade-in x [300ms] }"#;
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::FnDecl { params, .. } = &stmts[0] {
        assert_eq!(
            params[0].param_type,
            Some(TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str,]))
        );
    } else {
        panic!("expected FnDecl, got {:?}", stmts[0]);
    }
}

#[test]
fn test_enum_type_annotation_parses_in_action_param() {
    let src = r#"fn transform(x: Enum(left, right, top)) { fade-in x [300ms] }"#;
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::FnDecl { params, .. } = &stmts[0] {
        assert_eq!(
            params[0].param_type,
            Some(TypeAnnotation::Enum(vec![
                "left".to_string(),
                "right".to_string(),
                "top".to_string()
            ]))
        );
    } else {
        panic!("expected FnDecl, got {:?}", stmts[0]);
    }
}

#[test]
fn test_type_alias_parses_union_annotation() {
    let src = "pub type LegendMode = Bool | Str\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    match &stmts[0] {
        Stmt::TypeAlias {
            is_pub,
            name,
            annotation,
            ..
        } => {
            assert!(is_pub);
            assert_eq!(name, "LegendMode");
            assert_eq!(
                annotation,
                &TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str])
            );
        },
        other => panic!("expected TypeAlias, got {:?}", other),
    }
}

#[test]
fn test_namespaced_type_alias_reference_parses() {
    let src = "pub component Card(value: types::Metric) {}\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::ComponentDef(def, _) = &stmts[0] {
        assert_eq!(
            def.params[0].param_type,
            Some(TypeAnnotation::Alias("types::Metric".to_string()))
        );
    } else {
        panic!("expected component definition");
    }
}

#[test]
fn test_legacy_dotted_type_alias_normalizes() {
    let src = "pub component Card(value: types.Metric) {}\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::ComponentDef(def, _) = &stmts[0] {
        assert_eq!(
            def.params[0].param_type,
            Some(TypeAnnotation::Alias("types::Metric".to_string()))
        );
    } else {
        panic!("expected component definition");
    }
}

#[test]
fn test_rich_type_annotations_parse() {
    let src = "type P3 = Vec3\ntype Pair = Tuple<Str, Num>\ntype Mapper = Fn(Num, Num) => Num\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    match &stmts[0] {
        Stmt::TypeAlias {
            name, annotation, ..
        } => {
            assert_eq!(name, "P3");
            assert_eq!(annotation, &TypeAnnotation::Vec3);
        },
        other => panic!("expected Vec3 type alias, got: {:?}", other),
    }
    match &stmts[1] {
        Stmt::TypeAlias {
            name, annotation, ..
        } => {
            assert_eq!(name, "Pair");
            assert_eq!(
                annotation,
                &TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num])
            );
        },
        other => panic!("expected tuple type alias, got: {:?}", other),
    }
    match &stmts[2] {
        Stmt::TypeAlias {
            name, annotation, ..
        } => {
            assert_eq!(name, "Mapper");
            assert_eq!(
                annotation,
                &TypeAnnotation::Function {
                    params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                    ret: Box::new(TypeAnnotation::Num),
                }
            );
        },
        other => panic!("expected function type alias, got: {:?}", other),
    }
}

#[test]
fn test_rich_type_annotations_in_component_params() {
    let src = "pub component App(p: Vec3, pair: Tuple<Str, Num>, mapper: Fn(Num, Num) => Num) {}\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::ComponentDef(def, _) = &stmts[0] {
        assert_eq!(def.params[0].param_type, Some(TypeAnnotation::Vec3));
        assert_eq!(
            def.params[1].param_type,
            Some(TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num]))
        );
        assert_eq!(
            def.params[2].param_type,
            Some(TypeAnnotation::Function {
                params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                ret: Box::new(TypeAnnotation::Num),
            })
        );
    } else {
        panic!("expected component definition");
    }
}

#[test]
fn parenthesized_param_value_stays_default_expression() {
    // `(Num, Num)` is a tuple expression default, not a type annotation.
    let src = "component C(x: (Num, Num)) {}\n";
    let (stmts, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = stmts.expect("expected parse output");
    if let Stmt::ComponentDef(def, _) = &stmts[0] {
        assert_eq!(def.params[0].param_type, None);
        assert_eq!(
            def.params[0].default,
            Some(Expr::Tuple(vec![
                Expr::Ident("Num".to_string()),
                Expr::Ident("Num".to_string())
            ]))
        );
    } else {
        panic!("expected component definition");
    }
}

#[test]
fn parse_action_with_multiple_dotted_targets() {
    // Comma-separated dotted targets: `highlight eq.f1, eq.f2 [500ms]`
    let result = parse_snippet("highlight eq.f1, eq.f2 [500ms]");
    assert!(result.is_some(), "comma-separated dotted targets should parse");
    let stmts = result.unwrap();
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    assert!(action.is_some(), "expected Action stmt");
    if let Stmt::Action(action, _) = action.unwrap() {
        assert_eq!(action.verb, "highlight");
        assert_eq!(action.targets, vec!["eq.f1", "eq.f2"]);
    }
}

// -----------------------------------------------------------------------
// Gap 1a: subscript in value expressions
// -----------------------------------------------------------------------

#[test]
fn parse_subscript_expr_integer() {
    // `items[0]` in value position → Expr::Index
    let src = "always { x = items[0] }";
    let (stmts, errs) = parse_source(src);
    assert!(errs.is_empty(), "unexpected parse errors: {:?}", errs);
    let stmts = stmts.unwrap();
    if let Stmt::Always { body, .. } = &stmts[0] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert!(
                matches!(value, Expr::Index(base, idx)
                    if matches!(base.as_ref(), Expr::Ident(n) if n == "items")
                    && matches!(idx.as_ref(), Expr::Num(n) if *n == 0.0)
                ),
                "expected Index(Ident(items), Num(0)), got {:?}",
                value
            );
        } else {
            panic!("expected Assignment, got {:?}", body[0]);
        }
    } else {
        panic!("expected Always, got {:?}", stmts[0]);
    }
}

#[test]
fn parse_subscript_does_not_consume_modifier_bracket() {
    // `fade-in x [300ms]` — the `[300ms]` must be a modifier, not a subscript.
    let src = "fade-in x [300ms]";
    let result = parse_snippet(src);
    assert!(result.is_some(), "should parse without error");
    let stmts = result.unwrap();
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    assert!(action.is_some(), "expected Action stmt");
    if let Stmt::Action(a, _) = action.unwrap() {
        assert_eq!(a.targets, vec!["x"], "target should be 'x', not 'x[300ms]'");
        assert!(!a.modifiers.is_empty(), "modifier list should not be empty");
    }
}

// -----------------------------------------------------------------------
// Gap 1b: array-indexed targets
// -----------------------------------------------------------------------

#[test]
fn parse_indexed_action_target() {
    // `fade-in dots[0] [300ms]` → target resolves to "dots__0"
    let src = "fade-in dots[0] [300ms]";
    let result = parse_snippet(src);
    assert!(result.is_some(), "indexed action target should parse");
    let stmts = result.unwrap();
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    assert!(action.is_some(), "expected Action stmt");
    if let Stmt::Action(a, _) = action.unwrap() {
        assert_eq!(a.targets, vec!["dots__0"]);
        assert_eq!(a.target_index, vec![None]);
    }
}

#[test]
fn parse_indexed_action_target_with_expression() {
    // `swap bars[j], bars[j+1] [300ms]` → base projections + index exprs
    let src = "swap bars[j], bars[j+1] [300ms]";
    let result = parse_snippet(src);
    assert!(result.is_some(), "expression-indexed action target should parse");
    let stmts = result.unwrap();
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    assert!(action.is_some(), "expected Action stmt");
    if let Stmt::Action(a, _) = action.unwrap() {
        assert_eq!(a.targets, vec!["bars", "bars"]);
        assert_eq!(a.target_index.len(), 2);
        assert!(matches!(a.target_index[0], Some(Expr::Ident(_))));
        assert!(matches!(a.target_index[1], Some(Expr::Binary(..))));
    }
}

#[test]
fn parse_dotted_indexed_action_target() {
    // `swap row.b[j], row.b[j+1]` keeps the dotted base projection.
    let src = "swap row.b[j], row.b[j+1] [300ms]";
    let result = parse_snippet(src);
    assert!(result.is_some(), "dotted expression-indexed target should parse");
    let stmts = result.unwrap();
    let action = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Action(..)));
    if let Stmt::Action(a, _) = action.unwrap() {
        assert_eq!(a.targets, vec!["row.b", "row.b"]);
        assert!(a.target_index.iter().all(Option::is_some));
    }
}

#[test]
fn parse_for_loop_step_modifier() {
    // `for i in {0,1,2} [step: 250ms]` records the step modifier.
    let src = "for i in {0, 1, 2} [step: 250ms] { let x = i }";
    let result = parse_snippet(src);
    assert!(result.is_some(), "for loop with step should parse");
    let stmts = result.unwrap();
    let for_stmt = stmts.iter().find(|s| matches!(s, Stmt::ForLoop { .. }));
    assert!(for_stmt.is_some(), "expected ForLoop stmt");
    if let Stmt::ForLoop { modifiers, .. } = for_stmt.unwrap() {
        assert_eq!(modifiers.len(), 1);
        assert_eq!(modifiers[0].name.as_deref(), Some("step"));
    }
}

#[test]
fn parse_indexed_assignment_target() {
    // `dots[0].opacity = 1` → target ["dots__0"], property "opacity"
    let src = "dots[0].opacity = 1";
    let result = parse_snippet(src);
    assert!(result.is_some(), "indexed assignment target should parse");
    let stmts = result.unwrap();
    let stmt = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Assignment { .. }));
    assert!(stmt.is_some(), "expected Assignment stmt");
    if let Stmt::Assignment {
        target, property, ..
    } = stmt.unwrap()
    {
        assert_eq!(target, &vec![TargetSegment::Static("dots__0".to_string())]);
        assert_eq!(property, "opacity");
    }
}

#[test]
fn parse_indexed_dotted_assignment_target() {
    // `dots[1].at.x = 5` → target ["dots__1", "at"], property "x"
    let src = "dots[1].at.x = 5";
    let result = parse_snippet(src);
    assert!(result.is_some(), "dotted indexed assignment should parse");
    let stmts = result.unwrap();
    let stmt = stmts
        .iter()
        .flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .find(|s| matches!(s, Stmt::Assignment { .. }));
    if let Stmt::Assignment {
        target, property, ..
    } = stmt.unwrap()
    {
        assert_eq!(
            target,
            &vec![
                TargetSegment::Static("dots__1".to_string()),
                TargetSegment::Static("at".to_string()),
            ]
        );
        assert_eq!(property, "x");
    }
}

#[test]
fn parse_indexed_runtime_target() {
    // `bars[i].color = red` inside an `always` block should produce
    // TargetSegment::Indexed for the runtime index.
    let src = "always { bars[i].color = red }";
    let result = crate::parser::parse_source(src).0.unwrap();
    let always = result.iter().find(|s| matches!(s, Stmt::Always { .. })).unwrap();
    if let Stmt::Always { body, .. } = always {
        let assignment = body.iter().find(|s| matches!(s, Stmt::Assignment { .. })).unwrap();
        if let Stmt::Assignment {
            target, property, ..
        } = assignment
        {
            assert_eq!(target.len(), 1, "expected exactly one target segment");
            match &target[0] {
                TargetSegment::Indexed { base, .. } => {
                    assert_eq!(base, "bars");
                },
                other => panic!("expected Indexed segment, got {:?}", other),
            }
            assert_eq!(property, "color");
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected Always");
    }
}

#[test]
fn parse_static_indexed_target() {
    // `bars[0].opacity = 0.5` should produce TargetSegment::Static("bars__0")
    let src = "bars[0].opacity = 0.5";
    let result = crate::parser::parse_source(src).0.unwrap();
    let stmt = result.iter().find(|s| matches!(s, Stmt::Assignment { .. })).unwrap();
    if let Stmt::Assignment {
        target, property, ..
    } = stmt
    {
        assert_eq!(target.len(), 1, "expected exactly one target segment");
        match &target[0] {
            TargetSegment::Static(s) => {
                assert_eq!(s, "bars__0", "static index should resolve to '__' notation");
            },
            other => panic!("expected Static segment, got {:?}", other),
        }
        assert_eq!(property, "opacity");
    } else {
        panic!("Expected Assignment");
    }
}

#[test]
fn parse_logical_operators_with_comparison_precedence() {
    let src = "let x = t > 0 && t < 1 || flag";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    let Expr::Binary(left_or, BinaryOp::Or, right) = value else {
        panic!("expected top-level ||, got: {:?}", value);
    };
    assert_eq!(right.as_ref(), &Expr::Ident("flag".to_string()));
    let Expr::Binary(left_and, BinaryOp::And, _) = left_or.as_ref() else {
        panic!("expected left side to be &&, got: {:?}", left_or);
    };
    assert!(matches!(left_and.as_ref(), Expr::Binary(_, BinaryOp::Gt, _)));
}

#[test]
fn parse_power_binds_tighter_than_multiplication() {
    let src = "let x = a + b * c ^ d";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    let Expr::Binary(_, BinaryOp::Add, right_add) = value else {
        panic!("expected top-level +, got: {:?}", value);
    };
    let Expr::Binary(_, BinaryOp::Mul, right_mul) = right_add.as_ref() else {
        panic!("expected multiplication below addition, got: {:?}", right_add);
    };
    assert!(
        matches!(right_mul.as_ref(), Expr::Binary(_, BinaryOp::Pow, _)),
        "expected power to bind tighter than multiplication, got: {:?}",
        right_mul
    );
}

#[test]
fn parse_single_parenthesized_expression_is_not_tuple() {
    let src = "let x = (a + b)";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    assert!(
        matches!(value, Expr::Binary(_, BinaryOp::Add, _)),
        "expected parenthesized binary expression, got: {:?}",
        value
    );
}

#[test]
fn parse_inline_for_loop_index_variable() {
    let src = "#0s\nrow: Row {\n  for item, i in {1, 2, 3} {\n    box[i]: Rect, size: (10, item)\n  }\n}\n";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::Keyframe { body, .. } = &stmts[0] else {
        panic!("expected keyframe");
    };
    let Stmt::ActorDecl { children, .. } = &body[0] else {
        panic!("expected actor declaration");
    };
    match &children[0] {
        InlineItem::ForLoop { var, index_var, .. } => {
            assert_eq!(var, &LoopPattern::Single("item".to_string()));
            assert_eq!(index_var.as_deref(), Some("i"));
        },
        other => panic!("expected inline for loop, got: {:?}", other),
    }
}

#[test]
fn parse_unary_minus_on_dotted_path() {
    let src = "let y = -geo.y_hi";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    match value {
        Expr::Unary(UnaryOp::Neg, inner) => match inner.as_ref() {
            Expr::Path(parts) => {
                assert_eq!(parts, &["geo", "y_hi"]);
            },
            other => panic!("expected Expr::Path inside Unary(Neg), got: {:?}", other),
        },
        other => panic!("expected Expr::Unary(Neg, ...), got: {:?}", other),
    }
}

#[test]
fn parse_method_call_with_binary_expression_argument() {
    let src = "let pt = g.map(1.0 + 1.0, 2.0)";
    let (ast, errors) = crate::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let stmts = ast.expect("parsed AST");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    match value {
        Expr::Method(receiver, method, args) => {
            assert!(matches!(receiver.as_ref(), Expr::Ident(name) if name == "g"));
            assert_eq!(method, "map");
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0], Expr::Binary(_, BinaryOp::Add, _)));
        },
        other => panic!("expected Expr::Method, got: {:?}", other),
    }
}
