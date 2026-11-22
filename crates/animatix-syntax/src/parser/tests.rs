use super::*;
use crate::test_support::parse as parse_ok;

/// Parse a snippet that must succeed.
fn snippet_stmts(snippet: &str) -> Vec<Stmt> {
    parse_snippet(snippet).expect("snippet should parse")
}

/// The snippet's statements with any implicit `#0s` keyframe body inlined.
fn snippet_body(snippet: &str) -> Vec<Stmt> {
    snippet_stmts(snippet)
        .into_iter()
        .flat_map(|stmt| match stmt {
            Stmt::Keyframe { body, .. } => body,
            other => vec![other],
        })
        .collect()
}

fn snippet_call(snippet: &str) -> Expr {
    snippet_body(snippet)
        .into_iter()
        .find_map(|stmt| match stmt {
            Stmt::Expr(expr @ Expr::Call(..), _) => Some(expr),
            _ => None,
        })
        .expect("expected a function call statement")
}

fn snippet_assignment(snippet: &str) -> Stmt {
    snippet_body(snippet)
        .into_iter()
        .find(|stmt| matches!(stmt, Stmt::Assignment { .. }))
        .expect("expected an Assignment statement")
}

fn fn_param_type(src: &str) -> Option<TypeAnnotation> {
    let stmts = parse_ok(src);
    match &stmts[0] {
        Stmt::FnDecl { params, .. } => params[0].param_type.clone(),
        other => panic!("expected FnDecl, got {other:?}"),
    }
}

fn component_param_types(src: &str) -> Vec<Option<TypeAnnotation>> {
    let stmts = parse_ok(src);
    match &stmts[0] {
        Stmt::ComponentDef(def, _) => {
            def.params.iter().map(|param| param.param_type.clone()).collect()
        },
        other => panic!("expected component definition, got {other:?}"),
    }
}

fn type_alias(stmts: &[Stmt], index: usize) -> (&str, &TypeAnnotation) {
    match &stmts[index] {
        Stmt::TypeAlias {
            name, annotation, ..
        } => (name, annotation),
        other => panic!("expected a type alias, got {other:?}"),
    }
}

fn always_body(src: &str) -> Vec<Stmt> {
    let stmts = parse_ok(src);
    match &stmts[0] {
        Stmt::Always { body, .. } => body.clone(),
        other => panic!("expected Always, got {other:?}"),
    }
}

fn assignment_value(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Assignment { value, .. } => value,
        other => panic!("expected Assignment, got {other:?}"),
    }
}

fn parse_single_stmt(src: &str) -> Stmt {
    let stmts = parse_ok(src);
    match &stmts[0] {
        Stmt::Keyframe { body, .. } if !body.is_empty() => body[0].clone(),
        other => other.clone(),
    }
}

fn parse_err(src: &str) -> bool {
    let (_, errors) = parse_source(src);
    !errors.is_empty()
}

#[test]
fn hex_color_is_a_typed_literal_with_rgb_and_rgba_byte_order() {
    let source = r#"
let ink = hex_color("FfF1d6")
let translucent = hex_color("00a1ff80")
let text = "fff1d6"
"#;
    let ast = parse_ok(source);
    for (stmt, expected) in ast.iter().zip([
        Expr::Color([255, 241, 214, 255]),
        Expr::Color([0, 161, 255, 128]),
        Expr::Str("fff1d6".into()),
    ]) {
        let Stmt::LetDecl { value, .. } = stmt else {
            panic!("expected a let declaration");
        };
        assert_eq!(*value, expected);
    }
    assert_eq!(ast.len(), 3);
}

#[test]
fn hex_color_rejects_invalid_literals_and_expressions_inside_always() {
    for (argument, expected) in [
        (r##""#fff1d6""##, r#"Write hex_color("fff1d6")."#),
        (r#""fff1dg""#, "Invalid hexadecimal digit 'g'"),
        (r#""abc""#, "requires RRGGBB or RRGGBBAA"),
        (r#""+ff1d6""#, "Invalid hexadecimal digit '+'"),
        ("palette", "requires one quoted hex literal"),
        (r#""fff" + "1d6""#, "requires one quoted hex literal"),
    ] {
        let source = format!("always {{\n  box.color = hex_color({argument})\n}}");
        let (_, errors) = parse_source(&source);
        assert!(
            errors.iter().any(|error| error.message.contains(expected)
                && source.get(error.span.clone()) == Some(argument)),
            "expected an error on {argument:?}, got {errors:?}",
        );
    }
}

fn actor_parts(stmt: Stmt) -> (String, String, Vec<InlineItem>) {
    match stmt {
        Stmt::ActorDecl {
            label,
            ty,
            children,
            ..
        } => (label, ty, children),
        other => panic!("expected ActorDecl, got {other:?}"),
    }
}

fn slot_fill(item: &InlineItem) -> (&str, &[InlineItem]) {
    match item {
        InlineItem::SlotFill { slot, items } => (slot, items),
        other => panic!("expected SlotFill, got {other:?}"),
    }
}

fn labeled_item(item: &InlineItem) -> (&str, &str) {
    match item {
        InlineItem::Labeled { label, ty, .. } => (label, ty),
        other => panic!("expected Labeled inline item, got {other:?}"),
    }
}

#[test]
fn test_closure_parser() {
    let res = parse_ok("let f = (x) => x ^ 2");

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
fn closure_block_can_capture_outer_let_and_shadow_inside() {
    let stmts = parse_ok(
        "let base = 4\nlet step = (v) => { let base = base * 10 let out = base + v out }\n",
    );
    match &stmts[1] {
        Stmt::LetDecl { value, .. } => match value {
            Expr::Closure(params, body) => {
                assert_eq!(params, &["v"]);
                assert!(matches!(body.as_ref(), Expr::LetChain(bindings, tail)
                    if bindings.len() == 2 && matches!(tail.as_ref(), Expr::Ident(name) if name == "out")));
            },
            other => panic!("expected closure, got {other:?}"),
        },
        other => panic!("expected let declaration, got {other:?}"),
    }
}

#[test]
fn closure_let_chain_parses_after_a_keyframe() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade_in(target: label, duration: 100ms)

#0.5s
let base = 4
let step = (v) => {
  let base = base * 10
  let out = base + v
  out
}
label.text = format("step(2)={} base={}", step(2), base)
"#;
    assert!(!parse_ok(source).is_empty());
}

#[test]
fn closure_let_chain_accepts_nested_closure_tail() {
    parse_ok("let inner = (y) => y + a\n");
    parse_ok("let outer = (x) => { (y) => y + x }\n");
    let source = r#"
#0.5s
let maker = (x) => {
  let a = x * 10
  (y) => y + a
}
let add20 = maker(2)
label.text = format("add20(4) = {}", add20(4))
"#;
    let (stmts, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "nested closure parse errors: {errors:?}");
    let stmts = stmts.expect("nested closure should produce an AST");
    assert!(stmts.iter().any(|stmt| matches!(stmt, Stmt::Keyframe { body, .. }
        if body.iter().any(|stmt| matches!(stmt, Stmt::LetDecl { name, value: Expr::Closure(_, body), .. }
            if name == "maker" && matches!(body.as_ref(), Expr::LetChain(bindings, tail)
                if bindings.len() == 1 && matches!(tail.as_ref(), Expr::Closure(_, _))))))));
}

#[test]
fn test_shorthand_parser() {
    // Cases: (1) Text shorthand string literal, (2) Typst shorthand math delimiters ($$).
    for (input, expected_label, expected_ty, expected_prop, expected_val) in [
        (r#"a: "hello world""#, "a", "Text", "text", "hello world"),
        ("eq: $$ x^2 + y^2 $$", "eq", "Typst", "content", "x^2 + y^2"),
    ] {
        let res = parse_ok(input);

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
            assert_eq!(label, expected_label);
            assert_eq!(ty, expected_ty);
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, expected_prop);
            assert_eq!(props[0].value, Expr::Str(expected_val.to_string()));
            assert!(modifiers.is_empty());
            assert!(children.is_empty());
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }
}

#[test]
fn test_shorthand_with_modifiers() {
    // Cases: (1) Text shorthand with duration and ease, (2) Typst shorthand with duration.
    for (input, expected_label, expected_ty, expected_prop, expected_val, expected_mod_count) in [
        (
            r#"title: "Slide 1" [2s, ease: ease_in_out]"#,
            "title",
            "Text",
            "text",
            "Slide 1",
            2,
        ),
        ("eq: $$ x^2 $$ [2s]", "eq", "Typst", "content", "x^2", 1),
    ] {
        let res = parse_ok(input);

        if let Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers,
            ..
        } = &res[0]
        {
            assert_eq!(label, expected_label);
            assert_eq!(ty, expected_ty);
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, expected_prop);
            assert_eq!(props[0].value, Expr::Str(expected_val.to_string()));
            assert_eq!(modifiers.len(), expected_mod_count);
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }
}

/// Assert the value span of `property` covers exactly `expected` in `input`.
fn assert_value_span(input: &str, property: &str, expected: &str) {
    let res = parse_ok(input);
    if let Stmt::ActorDecl { props, .. } = &res[0] {
        let size_prop = props.iter().find(|p| p.name == property).unwrap();
        let span = size_prop.value_span.unwrap();

        let value_start = input.find(expected).unwrap();
        let value_end = value_start + expected.len();

        assert_eq!(span.start, value_start, "span start mismatch");
        assert_eq!(span.end, value_end, "span end mismatch");
        assert_eq!(&input[span.start..span.end], expected, "span extracts wrong text");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_vec2_value_span_accuracy() {
    // Cases: (1) standard tuple without trailing comma, (2) tuple with trailing comma.
    for (input, expected_span) in [
        (
            r#"backdrop: Rect, size: (2494.552, 1377.7778), color: scene.background"#,
            "(2494.552, 1377.7778)",
        ),
        (
            r#"backdrop: Rect, size: (2494.552, 1377.7778,), color: scene.background"#,
            "(2494.552, 1377.7778,)",
        ),
    ] {
        assert_value_span(input, "size", expected_span);
    }
}

#[test]
fn expression_statement_and_inline_expression_capture_source_spans() {
    let source = "fn f() {\n  first()\n  7\n}\ngroup: Group { second() }\n";
    let statements = parse_ok(source);
    let Stmt::FnDecl { body, .. } = &statements[0] else {
        panic!("expected function declaration");
    };
    let Stmt::Expr(Expr::Call(..), Some(call_span)) = &body[0] else {
        panic!("non-tail call should retain its source span: {:?}", body[0]);
    };
    assert_eq!((call_span.start_line, call_span.start_col), (2, 3));
    assert_eq!((call_span.end_line, call_span.end_col), (2, 10));
    let Stmt::Return {
        span: Some(tail_span),
        ..
    } = &body[1]
    else {
        panic!("tail expression should retain its source span: {:?}", body[1]);
    };
    assert_eq!((tail_span.start_line, tail_span.start_col), (3, 3));

    let Stmt::ActorDecl { children, .. } = &statements[1] else {
        panic!("expected Group declaration");
    };
    assert!(
        matches!(
            &children[0],
            InlineItem::Expr(Expr::Call(..), Some(span))
                if (span.start_line, span.start_col) == (5, 16)
        ),
        "inline expression should retain its source span: {:?}",
        children[0]
    );

    let keyframe_statements = parse_ok("#0s\n  cards[1.5].mark()\n");
    let Stmt::Keyframe {
        body,
        span: Some(keyframe_span),
        ..
    } = &keyframe_statements[0]
    else {
        panic!("expected keyframe");
    };
    assert_eq!((keyframe_span.start_line, keyframe_span.start_col), (1, 1));
    assert!(matches!(
        &body[0],
        Stmt::Expr(Expr::Call(..), Some(span))
            if (span.start_line, span.start_col) == (2, 3)
    ));

    let relative_keyframe_statements = parse_ok("#+0.5s\n  cards[1].opacity = 1\n");
    assert!(matches!(
        &relative_keyframe_statements[0],
        Stmt::RelativeKeyframe { span: Some(span), .. }
            if (span.start_line, span.start_col) == (1, 1)
    ));
}

#[test]
fn test_multiple_properties_span_independence() {
    // Test that spans for multiple properties don't overlap
    let input = r#"backdrop: Rect, size: (100, 200), color: red, anchor: center"#;
    let res = parse_ok(input);

    if let Stmt::ActorDecl { props, .. } = &res[0] {
        let size_span = props.iter().find(|p| p.name == "size").unwrap().value_span.unwrap();
        let color_span = props.iter().find(|p| p.name == "color").unwrap().value_span.unwrap();
        let anchor_span = props.iter().find(|p| p.name == "anchor").unwrap().value_span.unwrap();

        // Verify spans don't overlap
        assert!(size_span.end <= color_span.start, "size span overlaps color span");
        assert!(color_span.end <= anchor_span.start, "color span overlaps anchor span");

        // Verify extracted text
        assert_eq!(&input[size_span.start..size_span.end], "(100, 200)");
        assert_eq!(&input[color_span.start..color_span.end], "red");
        assert_eq!(&input[anchor_span.start..anchor_span.end], "center");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_reactive_binding_parser() {
    let res = parse_ok(r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#);
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
            if let Expr::Field(receiver, field) = left.as_ref() {
                assert!(matches!(receiver.as_ref(), Expr::Ident(name) if name == "tracker"));
                assert_eq!(field, "at");
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
    let (_, errors) = parse_source(r#"at := (100, 200)"#);
    assert!(!errors.is_empty(), "Expected parse error for single-segment reactive binding");
}

#[test]
fn test_comments_inside_delimiters() {
    let ast = parse_ok(
        r#"
            circ = Circle {
                at: (0, 0), // center
                radius: 100, // size
            }
        "#,
    );
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
    let ast = parse_ok(
        r#"
            label = Text {
                text: "Visit https://example.com // not a comment",
            }
        "#,
    );
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

macro_rules! tabstop_case {
    ($name:ident, $input:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(strip_snippet_tabstops($input), $expected);
        }
    };
}

tabstop_case!(strip_tabstops_simple, "${1:label}: ${2:Text} {}", "label: Text {}");
tabstop_case!(
    strip_tabstops_nested_braces,
    "${1:label}: ${2:Text} {\n    ${3:content}: \"${4:}\",\n}",
    "label: Text {\n    content: \"\",\n}"
);
tabstop_case!(strip_tabstops_no_default, "# ${1:0s}\n${2}", "# 0s\n");
tabstop_case!(strip_tabstops_preserves_dollar_signs, "price: \"$100\"", "price: \"$100\"");

#[test]
fn parse_snippet_actor_inline_props() {
    // AMX uses comma-separated inline properties, not braces
    let stmts = snippet_stmts("${1:label}: ${2:Text}, ${3:content}: \"${4:Hello}\"");
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
    let stmts = snippet_stmts(
        "${1:container}: ${2:Row} {\n    ${3:child}: ${4:Text}, content: \"${5:Hi}\"\n}",
    );
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
    let stmts = snippet_stmts("# ${1:0s}\n${2}");
    assert_eq!(stmts.len(), 1);
    assert!(
        matches!(&stmts[0], Stmt::Keyframe { .. }),
        "Expected Keyframe, got {:?}",
        stmts[0]
    );
}

#[test]
fn parse_snippet_always() {
    let stmts = snippet_stmts("always {\n    ${1:}\n}");
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0], Stmt::Always { .. }), "Expected Always, got {:?}", stmts[0]);
}

#[test]
fn parse_snippet_stagger() {
    let stmts = snippet_stmts("stagger [${1:150ms}] {\n    ${2:}\n}");
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
fn parse_function_call_with_dotted_actor_argument() {
    let call = snippet_call("highlight(target: decomp_eq.f1, duration: 800ms)");
    let Expr::Call(callee, args) = call else {
        panic!("expected call expression");
    };
    assert_eq!(callee.callable_name().as_deref(), Some("highlight"));
    assert_eq!(args[0].name.as_deref(), Some("target"));
    assert!(matches!(&args[0].value, Expr::Field(_, field) if field == "f1"));
    assert_eq!(args[1].name.as_deref(), Some("duration"));
    assert_eq!(args[1].value, Expr::Duration(0.8));
}

#[test]
fn call_named_arguments_accept_reserved_keyword_names() {
    let call = snippet_call(
        "draw_in(target: title, duration: 1s, by: word, offset_y: 30, mask: baseline, stagger: 0.1s)",
    );
    let Expr::Call(_, args) = call else {
        panic!("expected call expression");
    };
    assert_eq!(args.last().and_then(|argument| argument.name.as_deref()), Some("stagger"));
    assert_eq!(args.last().map(|argument| &argument.value), Some(&Expr::Duration(0.1)));
}

#[test]
fn function_call_arguments_are_parenthesized_before_next_assignment() {
    let stmts = parse_ok("#0s\npersist(target: t)\nt.opacity = 0.5 [200ms]");
    let Stmt::Keyframe { body, .. } = &stmts[0] else {
        panic!("expected keyframe");
    };
    assert_eq!(body.len(), 2);
    assert!(matches!(&body[0], Stmt::Expr(Expr::Call(..), _)));
    assert!(matches!(&body[1], Stmt::Assignment { .. }));
}

#[test]
fn parse_function_call_with_multiple_dotted_targets() {
    let call = snippet_call("highlight(targets: {eq.f1, eq.f2}, duration: 500ms)");
    let Expr::Call(callee, args) = call else {
        panic!("expected call expression");
    };
    assert_eq!(callee.callable_name().as_deref(), Some("highlight"));
    assert_eq!(args[0].name.as_deref(), Some("targets"));
    assert!(matches!(&args[0].value, Expr::List(items) if items.len() == 2));
}

#[test]
fn parse_indexed_actor_arguments_as_normal_expressions() {
    let call = snippet_call("fade_in(target: dots[0], duration: 300ms)");
    let Expr::Call(_, args) = call else {
        panic!("expected call expression");
    };
    assert!(matches!(&args[0].value, Expr::Index(base, index)
        if matches!(base.as_ref(), Expr::Ident(name) if name == "dots")
        && matches!(index.as_ref(), Expr::Num(n) if *n == 0.0)));
}

#[test]
fn parse_indexed_actor_arguments_with_field_access() {
    let call = snippet_call("fade_in(target: cards[j].title_text, duration: 300ms)");
    let Expr::Call(_, args) = call else {
        panic!("expected call expression");
    };
    assert!(matches!(&args[0].value, Expr::Field(receiver, field)
        if field == "title_text" && matches!(receiver.as_ref(), Expr::Index(..))));
    assert_eq!(
        crate::format_core::format_expr(&Expr::Call(Box::new(Expr::Ident("fade_in".into())), args)),
        "fade_in(target: cards[j].title_text, duration: 0.3s)"
    );
}

#[test]
fn test_method_call_with_args() {
    // Parse an always block with a method call: graph.map(mx, my)
    let body = always_body("always { ball.at = descent_graph.map(mx, my) }");
    // Body should have one assignment statement
    let value = assignment_value(&body[0]);
    // The value should be a Method call: graph.map(mx, my)
    assert!(
        matches!(value, Expr::Call(callee, args)
            if callee.callable_name().as_deref() == Some("descent_graph.map") && args.len() == 2),
        "Expected Call(Field(descent_graph, map), [mx, my]), got {:?}",
        value
    );
}

#[test]
fn test_method_call_on_dot_path() {
    // Parse chained method: a.b.c(d)
    let body = always_body("always { x = a.b.c(d) }");
    let value = assignment_value(&body[0]);
    assert!(
        matches!(value, Expr::Call(callee, args)
            if callee.callable_name().as_deref() == Some("a.b.c") && args.len() == 1),
        "Expected Call(Field(Field(a,b),c), [d]), got {:?}",
        value
    );
}

#[test]
fn test_field_access_still_works() {
    // Bare member access is represented by a Field expression.
    let body = always_body("always { x = descent_graph.at }");
    let value = assignment_value(&body[0]);
    assert!(
        matches!(value, Expr::Field(receiver, field)
            if matches!(receiver.as_ref(), Expr::Ident(name) if name == "descent_graph") && field == "at"),
        "Expected Field(descent_graph, at), got {:?}",
        value
    );
}

#[test]
fn test_any_type_annotation_in_action_param() {
    // `Any` in a parameter type annotation should parse without error.
    assert_eq!(
        fn_param_type(r#"fn transform(x: Any = 0) { fade_in(target: x, duration: 300ms) }"#),
        Some(TypeAnnotation::Any)
    );
}

#[test]
fn test_union_type_annotation_in_action_param() {
    assert_eq!(
        fn_param_type(
            r#"fn transform(x: Bool | Str = "x") { fade_in(target: x, duration: 300ms) }"#
        ),
        Some(TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]))
    );
}

#[test]
fn test_enum_type_annotation_parses_in_action_param() {
    assert_eq!(
        fn_param_type(
            r#"fn transform(x: Enum(left, right, top)) { fade_in(target: x, duration: 300ms) }"#
        ),
        Some(TypeAnnotation::Enum(vec![
            "left".to_string(),
            "right".to_string(),
            "top".to_string()
        ]))
    );
}

#[test]
fn test_type_alias_parses_union_annotation() {
    let stmts = parse_ok("pub type LegendMode = Bool | Str\n");
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
    assert_eq!(
        component_param_types("pub component Card(value: types::Metric) {}\n"),
        vec![Some(TypeAnnotation::Alias("types::Metric".to_string()))]
    );
}

#[test]
fn dotted_type_alias_is_rejected() {
    assert!(parse_err("pub component Card(value: types.Metric) {}\n"));
}

#[test]
fn test_rich_type_annotations_parse() {
    let stmts = parse_ok(
        "type P3 = Vec3\ntype Pair = Tuple<Str, Num>\ntype Mapper = Fn(Num, Num) => Num\n",
    );
    assert_eq!(type_alias(&stmts, 0), ("P3", &TypeAnnotation::Vec3));
    assert_eq!(
        type_alias(&stmts, 1),
        ("Pair", &TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num]))
    );
    assert_eq!(
        type_alias(&stmts, 2),
        (
            "Mapper",
            &TypeAnnotation::Function {
                params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                ret: Box::new(TypeAnnotation::Num),
            }
        )
    );
}

#[test]
fn test_rich_type_annotations_in_component_params() {
    assert_eq!(
        component_param_types(
            "pub component App(p: Vec3, pair: Tuple<Str, Num>, mapper: Fn(Num, Num) => Num) {}\n"
        ),
        vec![
            Some(TypeAnnotation::Vec3),
            Some(TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num])),
            Some(TypeAnnotation::Function {
                params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                ret: Box::new(TypeAnnotation::Num),
            }),
        ]
    );
}

#[test]
fn parenthesized_param_value_stays_default_expression() {
    // `:` introduces a type and `=` introduces a default expression.
    let stmts = parse_ok("component C(x = (Num, Num)) {}\n");
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
fn parse_call_with_multiple_dotted_targets() {
    let call = snippet_call("highlight(targets: {eq.f1, eq.f2}, duration: 500ms)");
    let Expr::Call(callee, args) = call else {
        panic!("expected call expression");
    };
    assert_eq!(callee.callable_name().as_deref(), Some("highlight"));
    assert!(matches!(&args[0].value, Expr::List(items) if items.len() == 2));
    assert_eq!(args[1].name.as_deref(), Some("duration"));
}

// Gap 1a: subscript in value expressions

#[test]
fn parse_subscript_expr_integer() {
    // `items[0]` in value position → Expr::Index
    let body = always_body("always { x = items[0] }");
    let value = assignment_value(&body[0]);
    assert!(
        matches!(value, Expr::Index(base, idx)
            if matches!(base.as_ref(), Expr::Ident(n) if n == "items")
            && matches!(idx.as_ref(), Expr::Num(n) if *n == 0.0)
        ),
        "expected Index(Ident(items), Num(0)), got {:?}",
        value
    );
}

#[test]
fn parse_for_loop_step_modifier() {
    // `for i in {0,1,2} [step: 250ms]` records the step modifier.
    let stmts = snippet_stmts("for i in {0, 1, 2} [step: 250ms] { let x = i }");
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
    let stmt = snippet_assignment("dots[0].opacity = 1");
    if let Stmt::Assignment {
        target, property, ..
    } = stmt
    {
        assert_eq!(target, vec![TargetSegment::Static("dots__0".to_string())]);
        assert_eq!(property, "opacity");
    }
}

#[test]
fn parse_indexed_dotted_assignment_target() {
    // `dots[1].at.x = 5` → target ["dots__1", "at"], property "x"
    let stmt = snippet_assignment("dots[1].at.x = 5");
    if let Stmt::Assignment {
        target, property, ..
    } = stmt
    {
        assert_eq!(
            target,
            vec![
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
    let stmts = parse_ok("always { bars[i].color = red }");
    let always = stmts.iter().find(|s| matches!(s, Stmt::Always { .. })).unwrap();
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
    let stmts = parse_ok("bars[0].opacity = 0.5");
    let stmt = stmts.iter().find(|s| matches!(s, Stmt::Assignment { .. })).unwrap();
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
    let stmts = parse_ok("let x = t > 0 && t < 1 || flag");
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
    let stmts = parse_ok("let x = a + b * c ^ d");
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
    let stmts = parse_ok("let x = (a + b)");
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
    let stmts = parse_ok(
        "#0s\nrow: Row {\n  for item, i in {1, 2, 3} {\n    box[i]: Rect, size: (10, item)\n  }\n}\n",
    );
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
    let stmts = parse_ok("let y = -geo.y_hi");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    match value {
        Expr::Unary(UnaryOp::Neg, inner) => match inner.as_ref() {
            Expr::Field(_, field) => assert_eq!(field, "y_hi"),
            other => panic!("expected Expr::Field inside Unary(Neg), got: {:?}", other),
        },
        other => panic!("expected Expr::Unary(Neg, ...), got: {:?}", other),
    }
}

#[test]
fn parse_method_call_with_binary_expression_argument() {
    let stmts = parse_ok("let pt = g.map(1.0 + 1.0, 2.0)");
    let Stmt::LetDecl { value, .. } = &stmts[0] else {
        panic!("expected let declaration");
    };
    match value {
        Expr::Call(callee, args) => {
            assert_eq!(callee.callable_name().as_deref(), Some("g.map"));
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0].value, Expr::Binary(_, BinaryOp::Add, _)));
        },
        other => panic!("expected Expr::Call, got: {:?}", other),
    }
}

#[test]
fn test_let_decl_types() {
    for (source, name, value) in [
        ("let a = 42", "a", Expr::Num(42.0)),
        ("let b = 3.25", "b", Expr::Num(3.25)),
        ("let c = true", "c", Expr::Bool(true)),
        ("let d = false", "d", Expr::Bool(false)),
        ("let e = null", "e", Expr::Null),
        ("let f = \"string\"", "f", Expr::Str("string".to_string())),
    ] {
        let stmt = parse_single_stmt(source);
        assert_eq!(
            stmt,
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
fn test_pub_let_declaration() {
    assert_eq!(
        parse_single_stmt("pub let rate = 3.25"),
        Stmt::LetDecl {
            is_pub: true,
            name: "rate".to_string(),
            value: Expr::Num(3.25),
            span: None,
        }
    );
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
    let stmt = parse_single_stmt("morpher.size = (100, 100) [2s, ease: ease_out]");
    if let Stmt::Assignment {
        target,
        property,
        value,
        modifiers,
        easing,
        ..
    } = stmt
    {
        assert_eq!(target, vec![TargetSegment::Static("morpher".to_string())]);
        assert_eq!(property, "size");
        assert_eq!(value, Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]));
        assert_eq!(modifiers.len(), 1);
        assert_eq!(modifiers[0].value, Expr::Duration(2.0));
        assert_eq!(easing, Some(crate::easing::Easing::EaseOut));
    } else {
        panic!("expected Assignment");
    }

    let stmt = parse_single_stmt("btn.at = (100, 100) [ease: linear]");
    if let Stmt::Assignment { easing, .. } = stmt {
        assert_eq!(easing, Some(crate::easing::Easing::Linear));
    } else {
        panic!("expected Assignment");
    }

    let stmt = parse_single_stmt("btn.opacity = 0.5 [ease: bounce_in]");
    if let Stmt::Assignment { easing, .. } = stmt {
        assert_eq!(easing, Some(crate::easing::Easing::Bounce));
    } else {
        panic!("expected Assignment");
    }

    let stmt = parse_single_stmt("left.badge.color = red");
    if let Stmt::Assignment {
        target,
        property,
        value,
        ..
    } = stmt
    {
        assert_eq!(
            target,
            vec![
                TargetSegment::Static("left".to_string()),
                TargetSegment::Static("badge".to_string())
            ]
        );
        assert_eq!(property, "color");
        assert_eq!(value, Expr::Ident("red".to_string()));
    } else {
        panic!("expected Assignment");
    }

    for (source, name, segments) in [
        ("let x = container.child", "x", &["container", "child"][..]),
        ("let fill = left.badge.color", "fill", &["left", "badge", "color"][..]),
        ("let center = scene.center", "center", &["scene", "center"][..]),
    ] {
        assert_eq!(
            parse_single_stmt(source),
            Stmt::LetDecl {
                is_pub: false,
                name: name.to_string(),
                value: segments
                    .iter()
                    .skip(1)
                    .fold(Expr::Ident(segments[0].to_string()), |receiver, field| {
                        Expr::Field(Box::new(receiver), (*field).to_string())
                    }),
                span: None,
            },
            "path expression for {source:?}"
        );
    }
}

#[test]
fn test_sequence_parse() {
    let stmt = parse_single_stmt(
        "sequence { fade_in(target: badge, duration: 500ms) badge.color = red [250ms, delay: 100ms] }",
    );
    if let Stmt::Sequence { body, .. } = stmt {
        assert_eq!(body.len(), 2);
        assert!(matches!(&body[0], Stmt::Expr(Expr::Call(..), _)));
        assert!(matches!(&body[1], Stmt::Assignment { .. }));
    } else {
        panic!("expected Sequence, got {stmt:?}");
    }
}

#[test]
fn duration_suffix_is_not_parsed_as_collection_index() {
    let assignment = parse_single_stmt("badge.opacity = 0.25[500ms]");
    match assignment {
        Stmt::Assignment {
            value, modifiers, ..
        } => {
            assert_eq!(value, Expr::Num(0.25));
            assert_eq!(modifiers.len(), 1);
            assert_eq!(modifiers[0].value, Expr::Duration(0.5));
        },
        other => panic!("expected Assignment, got {other:?}"),
    }

    let actor = parse_single_stmt("badge: Ellipse, opacity: 0.25[500ms]");
    match actor {
        Stmt::ActorDecl {
            props, modifiers, ..
        } => {
            assert_eq!(props[0].value, Expr::Num(0.25));
            assert_eq!(modifiers.len(), 1);
            assert_eq!(modifiers[0].value, Expr::Duration(0.5));
        },
        other => panic!("expected ActorDecl, got {other:?}"),
    }
}

#[test]
fn test_stagger_parse() {
    let stmt = parse_single_stmt(
        "stagger [150ms] { fade_in(target: first, duration: 200ms) second.color = red [100ms] }",
    );
    if let Stmt::Stagger {
        modifiers, body, ..
    } = stmt
    {
        assert_eq!(modifiers.len(), 1);
        assert_eq!(modifiers[0].value, Expr::Duration(0.15));
        assert_eq!(body.len(), 2);
    } else {
        panic!("expected Stagger, got {stmt:?}");
    }

    let stmt =
        parse_single_stmt("stagger [each: 150ms] { fade_in(target: first, duration: 200ms) }");
    if let Stmt::Stagger {
        modifiers, body, ..
    } = stmt
    {
        assert_eq!(modifiers.len(), 1);
        assert_eq!(modifiers[0].name.as_deref(), Some("each"));
        assert_eq!(modifiers[0].value, Expr::Duration(0.15));
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected Stagger, got {stmt:?}");
    }
}

#[test]
fn test_config_parse() {
    let stmts = parse_ok("config { colorscheme: \"editorial-dark\" }");
    if let Stmt::Config { settings, .. } = &stmts[0] {
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].name, "colorscheme");
        assert_eq!(settings[0].value, Expr::Str("editorial-dark".to_string()));
    } else {
        panic!("expected Config");
    }
}

#[test]
fn test_qualified_play_parse() {
    let stmts = parse_ok("play module::SceneName [fade, 300ms]");
    if let Stmt::Play { scene_name, .. } = &stmts[0] {
        assert_eq!(scene_name, "module::SceneName");
    } else {
        panic!("expected Play");
    }
}

#[test]
fn test_actor_decl_full() {
    let stmt = parse_single_stmt("circle: Ellipse, radius: 50, color: blue [2s, ease: bounce]");
    if let Stmt::ActorDecl {
        label,
        ty,
        props,
        modifiers,
        ..
    } = stmt
    {
        assert_eq!(label, "circle");
        assert_eq!(ty, "Ellipse");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "radius");
        assert_eq!(props[0].value, Expr::Num(50.0));
        assert_eq!(props[1].name, "color");
        assert_eq!(props[1].value, Expr::Ident("blue".to_string()));
        assert_eq!(modifiers.len(), 2);
    } else {
        panic!("expected ActorDecl");
    }
}

#[test]
fn test_actor_decl_modifiers_and_paths() {
    let stmt = parse_single_stmt(
        "badge: Ellipse, radius: 20 [delay: 250ms, ease: ease_in, ease: bounce_in]",
    );
    if let Stmt::ActorDecl { modifiers, .. } = stmt {
        assert_eq!(modifiers.len(), 3);
        assert_eq!(modifiers[0].name.as_deref(), Some("delay"));
        assert_eq!(modifiers[1].name.as_deref(), Some("ease"));
        assert_eq!(modifiers[2].name.as_deref(), Some("ease"));
    } else {
        panic!("expected ActorDecl");
    }

    let stmt = parse_single_stmt("badge: Ellipse, color: auto, stroke: stroke.default");
    if let Stmt::ActorDecl { props, .. } = stmt {
        assert_eq!(props[1].name, "stroke");
        assert_eq!(
            props[1].value,
            Expr::Field(Box::new(Expr::Ident("stroke".to_string())), "default".to_string())
        );
    } else {
        panic!("expected ActorDecl");
    }

    let stmt =
        parse_single_stmt("icon: Svg { url: \"vector.svg\", anchor: scene.top, offset: (0, 24) }");
    if let Stmt::ActorDecl { ty, props, .. } = stmt {
        assert_eq!(ty, "Svg");
        assert_eq!(props.len(), 3);
        assert_eq!(props[1].name, "anchor");
        assert_eq!(
            props[1].value,
            Expr::Field(Box::new(Expr::Ident("scene".to_string())), "top".to_string())
        );
    } else {
        panic!("expected Svg ActorDecl");
    }
}

#[test]
fn test_callout_block_props_parse_as_actor_props() {
    let stmt =
        parse_single_stmt("note: Callout { target: bar[2], place: right, label: \"Look here\" }");
    if let Stmt::ActorDecl {
        ty,
        props,
        children,
        ..
    } = stmt
    {
        assert_eq!(ty, "Callout");
        assert_eq!(props.len(), 3);
        let target = props.iter().find(|p| p.name == "target").expect("target prop");
        assert!(matches!(target.value, Expr::Index(_, _)));
        assert!(props.iter().any(|p| p.name == "place"));
        assert!(children.is_empty());
    } else {
        panic!("expected Callout actor declaration, got {:?}", stmt);
    }
}

#[test]
fn test_actor_decl_nested_and_anonymous() {
    let stmt = parse_single_stmt("group: Group { a: Ellipse, size: 10, b: Rect, size: 20 }");
    let (label, ty, children) = actor_parts(stmt);
    assert_eq!(label, "group");
    assert_eq!(ty, "Group");
    assert_eq!(children.len(), 2);
    assert_eq!(labeled_item(&children[0]), ("a", "Ellipse"));
    assert_eq!(labeled_item(&children[1]), ("b", "Rect"));

    let stmt = parse_single_stmt("group: Group { Ellipse, size: 10, Rect, size: 20 }");
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 2);
    assert!(matches!(&children[0], InlineItem::Anonymous { ty, .. } if ty == "Ellipse"));
    assert!(matches!(&children[1], InlineItem::Anonymous { ty, .. } if ty == "Rect"));

    let stmt = parse_single_stmt(
        "group: Group { a: Ellipse, size: 10 { child: Text, text: \"hi\" }, b: Rect, size: 20 }",
    );
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 2);
    if let InlineItem::Labeled {
        children: subchildren,
        ..
    } = &children[0]
    {
        assert_eq!(subchildren.len(), 1);
        assert_eq!(labeled_item(&subchildren[0]), ("child", "Text"));
    } else {
        panic!("expected Labeled inline item with children");
    }
}

#[test]
fn test_compound_property_expressions() {
    let stmt = parse_single_stmt("axis: Line, from: (-40, 0), to: (40, 0), stroke_width: 3");
    if let Stmt::ActorDecl { props, .. } = stmt {
        assert_eq!(props.len(), 3);
        assert!(
            matches!(props[0].value, Expr::Tuple(ref v) if matches!(v[0], Expr::Unary(UnaryOp::Neg, _)))
        );
    } else {
        panic!("expected Line ActorDecl");
    }

    let stmt = parse_single_stmt(
        "badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan",
    );
    if let Stmt::ActorDecl { props, .. } = stmt {
        assert!(matches!(props[0].value, Expr::List(ref v) if v.len() == 4));
    } else {
        panic!("expected Polygon ActorDecl");
    }

    let stmt = parse_single_stmt(
        "guide: Path, commands: {move_to(-120, 0), line_to(-40, -80), close()}, stroke: white",
    );
    if let Stmt::ActorDecl { props, .. } = stmt {
        if let Expr::List(calls) = &props[0].value {
            assert_eq!(calls.len(), 3);
            assert!(
                matches!(&calls[0], Expr::Call(callee, _) if callee.callable_name().as_deref() == Some("move_to"))
            );
        } else {
            panic!("expected List of calls");
        }
    } else {
        panic!("expected Path ActorDecl");
    }
}

#[test]
fn test_keyframes() {
    let src = "#500ms\nlet x = 1\n#2.5s\nlet y = 2";
    let ast = parse_ok(src);
    assert_eq!(ast.len(), 2);
    if let Stmt::Keyframe { time, body, .. } = &ast[0] {
        assert_eq!(time, &Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected Keyframe");
    }
    if let Stmt::Keyframe { time, body, .. } = &ast[1] {
        assert_eq!(time, &Time::Seconds(2.5));
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected Keyframe");
    }
}

#[test]
fn test_relative_keyframes() {
    let src = "#0s\nlet x = 1\n#+500ms\nlet y = 2\n#+1.5s\nlet z = 3";
    let ast = parse_ok(src);
    assert_eq!(ast.len(), 3);
    assert!(matches!(&ast[0], Stmt::Keyframe { time: Time::Seconds(s), .. } if *s == 0.0));
    assert!(matches!(
        &ast[1],
        Stmt::RelativeKeyframe {
            offset: Time::Milliseconds(500),
            ..
        }
    ));
    assert!(
        matches!(&ast[2], Stmt::RelativeKeyframe { offset: Time::Seconds(s), .. } if (*s - 1.5).abs() < 1e-6)
    );
}

#[test]
fn function_returns_a_reusable_timeline_expression() {
    let ast = parse_ok(
        r#"
fn ticker(dot, lane_y: Num) -> Timeline {
  #{
    #0ms
    dot.at = (140, lane_y)
    #+250ms
    dot.at = (380, lane_y)
    #500ms
    dot.at = (620, lane_y)
  }
}
"#,
    );
    let Stmt::FnDecl {
        return_type: Some(TypeAnnotation::Timeline),
        body,
        ..
    } = &ast[0]
    else {
        panic!("expected a typed timeline function, got {:?}", ast[0]);
    };
    let [
        Stmt::Return {
            value: Some(Expr::Timeline(plan)),
            ..
        },
    ] = body.as_slice()
    else {
        panic!("expected final timeline expression to be the return value: {body:?}");
    };
    assert!(matches!(
        plan.statements.as_slice(),
        [
            Stmt::Keyframe {
                time: Time::Milliseconds(0),
                ..
            },
            Stmt::RelativeKeyframe {
                offset: Time::Milliseconds(250),
                ..
            },
            Stmt::Keyframe {
                time: Time::Milliseconds(500),
                ..
            },
        ]
    ));
}

#[test]
fn local_keyframes_parse_in_the_real_ticker_example() {
    let source = include_str!("../../tests/fixtures/amx/animation/local_tickers.amx");
    let ast = parse_ok(source);
    let Stmt::FnDecl { body, .. } = ast
        .iter()
        .find(|stmt| matches!(stmt, Stmt::FnDecl { name, .. } if name == "ticker"))
        .expect("example declares ticker")
    else {
        panic!("expected ticker function")
    };
    assert!(matches!(
        body.as_slice(),
        [Stmt::Return {
            value: Some(Expr::Timeline(_)),
            ..
        }]
    ));
}

#[test]
fn pure_function_tail_expression_and_component_keyframe_restriction_remain() {
    let ast = parse_ok("fn twice(x: Num) -> Num { x * 2 }");
    assert!(matches!(
        &ast[0],
        Stmt::FnDecl {
            return_type: Some(_),
            body,
            ..
        } if matches!(body.as_slice(), [Stmt::Return { value: Some(Expr::Binary(_, BinaryOp::Mul, _)), .. }])
    ));

    assert!(parse_err("fn invalid() -> Num { #0ms\n return 1 }"));
    assert!(parse_err("component Lane { #0ms\n dot: Ellipse, at: (0, 0) }"));
}

#[test]
fn test_negative_relative_keyframes_rejected() {
    assert!(parse_err("#-1s\nlet x = 1"));
    assert!(parse_err("#+-1s\nlet x = 1"));
}

#[test]
fn test_conditionals() {
    let stmt = parse_single_stmt("if active { fade_in(target: btn, duration: 300ms) }");
    if let Stmt::Conditional {
        condition,
        then_branch,
        else_branch,
        ..
    } = stmt
    {
        assert_eq!(condition, Expr::Ident("active".to_string()));
        assert_eq!(then_branch.len(), 1);
        assert!(else_branch.is_none());
    } else {
        panic!("expected Conditional");
    }

    let stmt = parse_single_stmt(
        "if active { fade_in(target: btn, duration: 300ms) } else { fade_out(target: btn, duration: 300ms) }",
    );
    if let Stmt::Conditional { else_branch, .. } = stmt {
        assert!(else_branch.is_some());
    } else {
        panic!("expected Conditional with else");
    }

    let stmt = parse_single_stmt("pulse.size = if active { (120, 120) } else { (180, 180) }");
    if let Stmt::Assignment { value, .. } = stmt {
        assert!(matches!(value, Expr::Conditional(..)));
    } else {
        panic!("expected Assignment with Conditional Expr");
    }
}

#[test]
fn test_for_loops() {
    let stmt = parse_single_stmt("for (x, y) in points { fade_in(target: dot, duration: 300ms) }");
    if let Stmt::ForLoop {
        var,
        index_var,
        iterable,
        body,
        ..
    } = stmt
    {
        assert_eq!(var, LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
        assert!(index_var.is_none());
        assert_eq!(iterable, Expr::Ident("points".to_string()));
        assert_eq!(body.len(), 1);
    } else {
        panic!("expected ForLoop");
    }

    let stmt =
        parse_single_stmt("for (a, b), i in items { fade_in(target: dot, duration: 300ms) }");
    if let Stmt::ForLoop { var, index_var, .. } = stmt {
        assert_eq!(var, LoopPattern::Tuple(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(index_var.as_deref(), Some("i"));
    } else {
        panic!("expected ForLoop");
    }

    let stmt =
        parse_single_stmt("group: Group { for (x, y), i in pts { dot[i]: Rect, at: (x, y) } }");
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 1);
    match &children[0] {
        InlineItem::ForLoop {
            var,
            index_var,
            body,
            ..
        } => {
            assert_eq!(var, &LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
            assert_eq!(index_var.as_deref(), Some("i"));
            assert_eq!(body.len(), 1);
        },
        other => panic!("expected InlineItem::ForLoop, got {other:?}"),
    }
}

#[test]
fn test_imports() {
    let stmt = parse_single_stmt(r#"import "theme.amx" as theme"#);
    assert_eq!(
        stmt,
        Stmt::Import {
            path: "theme.amx".to_string(),
            alias: Some("theme".to_string()),
            span: None,
        }
    );

    let stmt = parse_single_stmt(r#"import "shared.amx""#);
    assert_eq!(
        stmt,
        Stmt::Import {
            path: "shared.amx".to_string(),
            alias: None,
            span: None,
        }
    );
}

#[test]
fn std_namespace_alias_is_reserved_for_standard_callables() {
    let (_, errors) = parse_source(r#"import "local.amx" as std"#);
    assert!(
        !errors.is_empty(),
        "the canonical `std::` namespace must not be shadowed: {errors:?}"
    );

    let (_, errors) = parse_source("#0s\n  let value = std::sin(0)");
    assert!(errors.is_empty(), "explicit standard calls should parse: {errors:?}");
}

#[test]
fn test_slot_marker_and_fill() {
    let stmt = parse_single_stmt("header: Col { @slot }");
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 1);
    assert!(matches!(children[0], InlineItem::SlotMarker));

    let stmt = parse_single_stmt("footer: Col { @slot \n Text, text: \"Default\" }");
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 2);
    assert!(matches!(children[0], InlineItem::SlotMarker));
    assert!(matches!(children[1], InlineItem::Anonymous { .. }));

    let stmt = parse_single_stmt(
        "slide: SlideLayout { @header { title: Text, text: \"Hello\" } @body { badge: Ellipse } }",
    );
    let (_, _, children) = actor_parts(stmt);
    assert_eq!(children.len(), 2);
    let (slot0, items0) = slot_fill(&children[0]);
    assert_eq!(slot0, "header");
    assert_eq!(items0.len(), 1);
    let (slot1, items1) = slot_fill(&children[1]);
    assert_eq!(slot1, "body");
    assert_eq!(items1.len(), 1);

    let stmt = parse_single_stmt("modal: Dialog { @content { } }");
    let (_, _, children) = actor_parts(stmt);
    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "content");
    assert!(items.is_empty());

    let stmt = parse_single_stmt("modal: Dialog { @ slot { } }");
    let (_, _, children) = actor_parts(stmt);
    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "slot");
    assert!(items.is_empty());
}

#[test]
fn test_match_stmt_and_expr() {
    let stmts = parse_ok(
        r#"
#0s
match floor(t) % 3 {
  0 => { fade_in(target: a, duration: 300ms) },
  1 => { fade_in(target: b, duration: 300ms) },
  _ => {}
}
"#,
    );
    let body = match &stmts[0] {
        Stmt::Keyframe { body, .. } => body,
        other => panic!("expected Keyframe, got {other:?}"),
    };
    if let Stmt::Match { arms, .. } = &body[0] {
        assert_eq!(arms.len(), 3);
    } else {
        panic!("expected Stmt::Match, got {:?}", body[0]);
    }

    let stmts = parse_ok(
        r#"
#0s
match floor(t) % 2 {
  0 => { box.color = red }
  _ => { box.color = blue }
}
"#,
    );
    let body = match &stmts[0] {
        Stmt::Keyframe { body, .. } => body,
        _other => panic!("expected Keyframe"),
    };
    assert!(matches!(&body[0], Stmt::Match { .. }));

    let stmts = parse_ok(
        r#"
always {
  r.color = match floor(t) % 2 { 0 => red, _ => blue }
}
"#,
    );
    if let Stmt::Always { body, .. } = &stmts[0] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            if let Expr::Match(_, arms) = value {
                assert_eq!(arms.len(), 2);
            } else {
                panic!("expected Expr::Match, got {:?}", value);
            }
        } else {
            panic!("expected Assignment");
        }
    } else {
        panic!("expected Always");
    }

    assert!(parse_err("always { r.color = match floor(t) % 2 { 0 => red \n _ => blue } }"));
}

#[test]
fn test_rejected_syntax() {
    assert!(parse_err("on appear { fade_in(target: btn, duration: 1s) }"));
    assert!(parse_err("on disappear { fade_out(target: btn, duration: 1s) }"));
    assert!(parse_err("loop { move(target: btn, to: (10, 20), duration: 1s) }"));
    assert!(parse_err("loop 5s { fade(target: btn, duration: 1s) }"));
    assert!(parse_err("yield"));
    assert!(parse_err("stop job"));
    assert!(parse_err("pause job"));
    assert!(parse_err("resume job"));
}

#[test]
fn parser_recovers_each_invalid_run_and_keeps_following_declarations() {
    let source = "title: Text, text: \"before\"\n\n@@@ total garbage @@@\n\nsubtitle: Text, text: \"after\"\n#0s\nfade_in(target: title, duration: 1s)\n";
    let parsed = parse_source_complete(source);
    let error_positions: Vec<(usize, usize)> =
        parsed.errors.iter().map(|error| (error.line - 1, error.column - 1)).collect();
    assert_eq!(error_positions, vec![(2, 0), (2, 18)]);

    let statements = parsed.ast.expect("recovery should retain later statements");
    assert!(statements.iter().any(|statement| matches!(
        statement,
        Stmt::ActorDecl { label, .. } if label == "subtitle"
    )));
}
