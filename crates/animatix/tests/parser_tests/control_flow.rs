use super::*;

/// Unwrap an actor declaration's own fields for structural assertions.
fn actor_parts(stmt: Stmt) -> (String, String, Vec<InlineItem>) {
    match stmt {
        Stmt::ActorDecl {
            label,
            ty,
            children,
            ..
        } => (label, ty, children),
        other => panic!("Expected ActorDecl, got {:?}", other),
    }
}

fn slot_fill(item: &InlineItem) -> (&str, &[InlineItem]) {
    match item {
        InlineItem::SlotFill { slot, items } => (slot, items),
        other => panic!("Expected SlotFill, got {:?}", other),
    }
}

fn labeled(item: &InlineItem) -> (&str, &str) {
    match item {
        InlineItem::Labeled { label, ty, .. } => (label, ty),
        other => panic!("Expected Labeled inline item, got {:?}", other),
    }
}

/// Evaluate a `list_*` builtin and compare the whole resulting list.
fn assert_evaluated_list(result: Value, expected: &[f64]) {
    match result {
        Value::List(items) => {
            let actual: Vec<f64> = items
                .iter()
                .map(|item| match item {
                    Value::Num(n) => *n,
                    other => panic!("expected numeric list element, got {other:?}"),
                })
                .collect();
            assert_eq!(actual, expected);
        },
        other => panic!("expected List, got {other:?}"),
    }
}

fn assert_parsed_with(src: &str, predicate: impl Fn(&Stmt) -> bool + Copy, want: &str) {
    let (ast, errors) = parse_source(src);
    assert!(errors.is_empty(), "parse errors: {:?}", errors);
    let ast = ast.unwrap();
    assert!(ast.iter().any(|stmt| find_stmt(stmt, predicate)), "{want}");
}

#[test]
fn test_for_loop_with_range() {
    let result = parse_single_stmt("for i in {1, 2, 3} { scale btn [0.1s] }");
    assert_eq!(
        result,
        Stmt::ForLoop {
            var: LoopPattern::Single("i".to_string()),
            index_var: None,
            iterable: Expr::List(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]),
            modifiers: vec![],
            body: vec![action("scale", &["btn"], vec![modifier("0.1s")], (21, 37))],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_destructuring_basic() {
    assert_eq!(
        parse_single_stmt("for (x, y) in points { appear dot }"),
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]),
            index_var: None,
            iterable: Expr::Ident("points".to_string()),
            modifiers: vec![],
            body: vec![action("appear", &["dot"], vec![], (23, 33))],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_three_elements() {
    assert_eq!(
        parse_single_stmt("for (r, g, b) in colors { fade-out fade }"),
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["r".to_string(), "g".to_string(), "b".to_string()]),
            index_var: None,
            iterable: Expr::Ident("colors".to_string()),
            modifiers: vec![],
            body: vec![action("fade-out", &["fade"], vec![], (26, 39))],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_with_index() {
    assert_eq!(
        parse_single_stmt("for (a, b), i in items { appear dot }"),
        Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["a".to_string(), "b".to_string()]),
            index_var: Some("i".to_string()),
            iterable: Expr::Ident("items".to_string()),
            modifiers: vec![],
            body: vec![action("appear", &["dot"], vec![], (25, 35))],
            span: None,
        }
    );
}

#[test]
fn test_for_loop_tuple_destructuring_in_inline() {
    let (label, ty, children) = actor_parts(parse_single_stmt(
        "group: Group { for (x, y) in pts { dot: Rect, at: (x, y) } }",
    ));
    assert_eq!(label, "group");
    assert_eq!(ty, "Group");
    assert_eq!(children.len(), 1);
    match &children[0] {
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => {
            assert_eq!(var, &LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
            assert_eq!(index_var, &None);
            assert_eq!(iterable, &Expr::Ident("pts".to_string()));
            assert_eq!(body.len(), 1);
            let (label, ty) = labeled(&body[0]);
            assert_eq!(label, "dot");
            assert_eq!(ty, "Rect");
            match &body[0] {
                InlineItem::Labeled { props, .. } => {
                    assert_eq!(props.len(), 1);
                    assert_eq!(props[0].name, "at");
                },
                other => panic!("Expected Labeled inline item, got {:?}", other),
            }
        },
        other => panic!("Expected InlineItem::ForLoop, got {:?}", other),
    }
}

#[test]
fn test_for_loop_tuple_inline_with_index() {
    let (label, ty, children) = actor_parts(parse_single_stmt(
        "group: Group { for (x, y), i in pts { dot[i]: Rect, at: (x, y) } }",
    ));
    assert_eq!(label, "group");
    assert_eq!(ty, "Group");
    assert_eq!(children.len(), 1);
    match &children[0] {
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => {
            assert_eq!(var, &LoopPattern::Tuple(vec!["x".to_string(), "y".to_string()]));
            assert_eq!(index_var, &Some("i".to_string()));
            assert_eq!(iterable, &Expr::Ident("pts".to_string()));
            assert_eq!(body.len(), 1);
        },
        other => panic!("Expected InlineItem::ForLoop, got {:?}", other),
    }
}

#[test]
fn test_pub_let_declaration() {
    assert_eq!(
        parse_single_stmt("pub let pi = 3.14"),
        Stmt::LetDecl {
            is_pub: true,
            name: "pi".to_string(),
            value: Expr::Num(3.14),
            span: None,
        }
    );
}

#[test]
fn test_import_with_alias() {
    let result = parse_single_stmt(r#"import "theme.amx" as theme"#);
    assert_eq!(
        result,
        Stmt::Import {
            path: "theme.amx".to_string(),
            alias: Some("theme".to_string()),
            span: None,
        }
    );
}

#[test]
fn test_import_without_alias() {
    let result = parse_single_stmt(r#"import "shared.amx""#);
    assert_eq!(
        result,
        Stmt::Import {
            path: "shared.amx".to_string(),
            alias: None,
            span: None
        }
    );
}

#[test]
fn test_slot_marker_in_container() {
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"header: Col {
  @slot
}"#,
    ));
    assert_eq!(label, "header");
    assert_eq!(ty, "Col");
    assert_eq!(children.len(), 1);
    assert!(matches!(children[0], InlineItem::SlotMarker));
}

#[test]
fn test_slot_marker_with_defaults_in_container() {
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"footer: Col {
  @slot
  Text, text: "Default"
}"#,
    ));
    assert_eq!(label, "footer");
    assert_eq!(ty, "Col");
    assert_eq!(children.len(), 2);
    assert!(matches!(children[0], InlineItem::SlotMarker));
    match &children[1] {
        InlineItem::Anonymous { ty, props, .. } => {
            assert_eq!(ty, "Text");
            assert!(!props.is_empty());
        },
        other => panic!("Expected Anonymous item, got {:?}", other),
    }
}

#[test]
fn test_slot_fill_parsing() {
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"slide: SlideLayout {
  @header {
    title: Text, text: "Hello"
  }
  @body {
    badge: Ellipse, radius: 20
  }
}"#,
    ));
    assert_eq!(label, "slide");
    assert_eq!(ty, "SlideLayout");
    assert_eq!(children.len(), 2);

    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "header");
    assert_eq!(items.len(), 1);
    assert_eq!(labeled(&items[0]), ("title", "Text"));

    let (slot, items) = slot_fill(&children[1]);
    assert_eq!(slot, "body");
    assert_eq!(items.len(), 1);
}

#[test]
fn test_mixed_slot_fill_parsing() {
    // @slot itself should also parse as a slot fill when used as @slot { items }
    let children = actor_parts(parse_single_stmt(
        r#"mycomp: MyComponent {
  @ slot {
    Text, text: "Content"
  }
}"#,
    ))
    .2;
    assert_eq!(children.len(), 1);
    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "slot");
    assert_eq!(items.len(), 1);
}

#[test]
fn test_empty_slot_fill() {
    // Parsing a slot fill with an empty body: @slotname { }
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"modal: Dialog {
  @ slot { }
}"#,
    ));
    assert_eq!(label, "modal");
    assert_eq!(ty, "Dialog");
    assert_eq!(children.len(), 1);
    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "slot");
    assert!(items.is_empty());
}

#[test]
fn test_slot_fill_with_multiple_items() {
    // A slot fill containing multiple items.
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"header: Header {
  @title {
    Text, text: "Welcome"
    Text, text: "Subtitle"
  }
}"#,
    ));
    assert_eq!(label, "header");
    assert_eq!(ty, "Header");
    assert_eq!(children.len(), 1);
    let (slot, items) = slot_fill(&children[0]);
    assert_eq!(slot, "title");
    assert_eq!(items.len(), 2);
}

#[test]
fn test_slot_marker_as_only_child() {
    // @slot as the only item in the container (no defaults).
    let (label, ty, children) = actor_parts(parse_single_stmt(
        r#"sidebar: Sidebar {
  @slot
}"#,
    ));
    assert_eq!(label, "sidebar");
    assert_eq!(ty, "Sidebar");
    assert_eq!(children.len(), 1);
    assert!(matches!(children[0], InlineItem::SlotMarker));
}

#[test]
fn test_at_slot_as_slot_fill() {
    // When @slot is followed by { }, it parses as a slot fill named "slot".
    // This is a valid-but-weird case; just ensure it parses without error.
    let children = actor_parts(parse_single_stmt(
        r#"comp: MyComponent {
  @ slot {
    Text, text: "Inline content"
  }
}"#,
    ))
    .2;
    assert!(!children.is_empty());
}

// ---------------------------------------------------------------------------
// Gap 1b integration: array-indexed targets build correctly
// ---------------------------------------------------------------------------

#[test]
fn test_indexed_target_build_property_assignment() {
    // dots__0 is declared by `dots[0]: Rect` inside a keyframe.
    // `dots[0].opacity = 1` (outside keyframe, processed at t=0) sets a
    // keyframe on the dots__0 track, verifying the indexed target resolves.
    let src = r#"
#0s
dots__0: Rect, at: (0,0), size: (10,10)
dots[0].opacity = 1
"#;
    let ast = parse_program(src);
    let timeline = AnimatixTimeline::build(&ast);
    assert!(
        timeline.tracks().contains_key("dots__0"),
        "track dots__0 should exist; tracks: {:?}",
        timeline.tracks().keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_indexed_target_build_action() {
    // dots__1 is declared as a direct track.
    // `fade-in dots[1] [300ms]` should generate an action event targeting `dots__1`.
    let ast = parse_program(
        r#"
#0s
dots__1: Rect, at: (0,0), size: (10,10)
fade-in dots[1] [300ms]
"#,
    );
    let timeline = AnimatixTimeline::build(&ast);
    let has_target =
        timeline.action_events.iter().any(|e| e.targets.iter().any(|t| t == "dots__1"));
    assert!(
        has_target,
        "expected action event targeting dots__1; events: {:?}",
        timeline.action_events
    );
}

#[test]
fn test_match_expr_parsing() {
    // Parse a match expression and verify it produces Expr::Match
    assert_parsed_with(
        r#"
config {}
#0s
r: Rect, size:(10,10), at:(0,0)
always {
  r.color = match floor(t) % 2 { 0 => red, _ => blue }
}
"#,
        stmt_contains_match_expr,
        "expected at least one Expr::Match in AST",
    );
}

#[test]
fn test_match_stmt_parsing() {
    // Parse a match statement and verify it produces Stmt::Match
    assert_parsed_with(
        r#"
config {}
#0s
match floor(t) % 3 {
  0 => { fade-in a [300ms] },
  1 => { fade-in b [300ms] },
  _ => {}
}
"#,
        |stmt| matches!(stmt, Stmt::Match { .. }),
        "expected at least one Stmt::Match in AST",
    );
}

#[test]
fn test_match_stmt_without_commas_parses() {
    assert_parsed_with(
        r#"
#0s
match floor(t) % 2 {
  0 => { box.color = red }
  _ => { box.color = blue }
}
"#,
        |stmt| matches!(stmt, Stmt::Match { .. }),
        "expected Stmt::Match without comma separators",
    );
}

#[test]
fn test_match_stmt_mixed_comma_form_parses() {
    assert_parsed_with(
        r#"
#0s
match floor(t) % 3 {
  0 => { box.color = red },
  _ => { box.color = blue }
}
"#,
        |stmt| matches!(stmt, Stmt::Match { .. }),
        "expected Stmt::Match with mixed comma separators",
    );
}

#[test]
fn test_match_expression_requires_comma_separators() {
    let src = r#"
#0s
box: Rect
always {
  box.color = match floor(t) % 2 {
    0 => red
    _ => blue
  }
}
"#;
    let (_ast, errors) = parse_source(src);
    assert!(!errors.is_empty(), "expression match should require comma-separated arms");
}

// Helper: pattern_match_test helper since `pattern_matches` is not public
fn test_pattern_matches(
    pat: &animatix_syntax::ast::MatchPattern,
    value: &animatix::timeline::Value,
) -> bool {
    match pat {
        MatchPattern::Wildcard => true,
        MatchPattern::Num(n) => matches!(value, Value::Num(v) if (*v - *n).abs() < f64::EPSILON),
        MatchPattern::Str(s) => matches!(value, Value::Str(v) if v == s),
        MatchPattern::Bool(b) => matches!(value, Value::Bool(v) if v == b),
        MatchPattern::Range(lo, hi) => {
            let lo_val = match lo.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return false,
            };
            let hi_val = match hi.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return false,
            };
            matches!(value, Value::Num(v) if *v >= lo_val && *v <= hi_val)
        },
        MatchPattern::Or(pats) => pats.iter().any(|p| test_pattern_matches(p, value)),
        MatchPattern::Tuple(pats) => match value {
            Value::List(items) => {
                items.len() == pats.len()
                    && pats.iter().zip(items.iter()).all(|(p, v)| test_pattern_matches(p, v))
            },
            _ => false,
        },
    }
}

#[test]
fn test_match_pattern_wildcard() {
    assert!(test_pattern_matches(&MatchPattern::Wildcard, &Value::Num(42.0)));
    assert!(test_pattern_matches(&MatchPattern::Wildcard, &Value::Str("hello".to_string())));
    assert!(test_pattern_matches(&MatchPattern::Wildcard, &Value::Bool(true)));
}

#[test]
fn test_match_pattern_literal() {
    assert!(test_pattern_matches(&MatchPattern::Num(0.0), &Value::Num(0.0)));
    assert!(!test_pattern_matches(&MatchPattern::Num(1.0), &Value::Num(0.0)));
    assert!(test_pattern_matches(
        &MatchPattern::Str("red".to_string()),
        &Value::Str("red".to_string())
    ));
    assert!(!test_pattern_matches(
        &MatchPattern::Str("blue".to_string()),
        &Value::Str("red".to_string())
    ));
    assert!(test_pattern_matches(&MatchPattern::Bool(true), &Value::Bool(true)));
    assert!(!test_pattern_matches(&MatchPattern::Bool(true), &Value::Bool(false)));
}

#[test]
fn test_match_pattern_range() {
    let range =
        MatchPattern::Range(Box::new(MatchPattern::Num(1.0)), Box::new(MatchPattern::Num(3.0)));
    assert!(test_pattern_matches(&range, &Value::Num(1.0)));
    assert!(test_pattern_matches(&range, &Value::Num(2.0)));
    assert!(test_pattern_matches(&range, &Value::Num(3.0)));
    assert!(!test_pattern_matches(&range, &Value::Num(0.0)));
    assert!(!test_pattern_matches(&range, &Value::Num(4.0)));
}

#[test]
fn test_match_pattern_or() {
    let or_pat = MatchPattern::Or(vec![MatchPattern::Num(0.0), MatchPattern::Num(2.0)]);
    assert!(test_pattern_matches(&or_pat, &Value::Num(0.0)));
    assert!(test_pattern_matches(&or_pat, &Value::Num(2.0)));
    assert!(!test_pattern_matches(&or_pat, &Value::Num(1.0)));
}

#[test]
fn test_match_pattern_tuple() {
    let tuple_pat = MatchPattern::Tuple(vec![
        MatchPattern::Num(0.0),
        MatchPattern::Wildcard,
        MatchPattern::Num(2.0),
    ]);
    assert!(test_pattern_matches(
        &tuple_pat,
        &Value::List(vec![Value::Num(0.0), Value::Num(1.0), Value::Num(2.0)].into())
    ));
    assert!(!test_pattern_matches(
        &tuple_pat,
        &Value::List(vec![Value::Num(0.0), Value::Num(1.0), Value::Num(3.0)].into())
    ));
}

#[test]
fn test_list_swap_builtin() {
    let list = Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)].into());
    let result = eval_builtin_fn("list_swap", &[list, Value::Num(0.0), Value::Num(2.0)]).unwrap();
    assert_evaluated_list(result, &[3.0, 2.0, 1.0]);
}

#[test]
fn test_list_set_builtin() {
    let list = Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)].into());
    let result = eval_builtin_fn("list_set", &[list, Value::Num(1.0), Value::Num(99.0)]).unwrap();
    assert_evaluated_list(result, &[1.0, 99.0, 3.0]);
}

#[test]
fn test_list_swap_out_of_range() {
    let list = Value::List(vec![Value::Num(1.0), Value::Num(2.0)].into());
    let result = eval_builtin_fn("list_swap", &[list, Value::Num(0.0), Value::Num(99.0)]).unwrap();
    assert_evaluated_list(result, &[1.0, 2.0]);
}

// Helper: check if a statement or any substatement contains Expr::Match
fn stmt_contains_match_expr(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            body.iter().any(stmt_contains_match_expr)
        },
        Stmt::Always { body, .. } => body.iter().any(stmt_contains_match_expr),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch.iter().any(stmt_contains_match_expr)
                || else_branch.as_ref().is_some_and(|b| b.iter().any(stmt_contains_match_expr))
        },
        Stmt::ForLoop { body, .. } => body.iter().any(stmt_contains_match_expr),
        Stmt::Match { .. } => true,
        Stmt::Assignment { value, .. } => {
            let mut found = false;
            animatix_syntax::walk::walk_expr(value, &mut |e| {
                if matches!(e, Expr::Match(..)) {
                    found = true;
                }
            });
            found
        },
        _ => false,
    }
}

// Helper: find a statement matching a predicate recursively
fn find_stmt<F>(stmt: &Stmt, f: F) -> bool
where
    F: Fn(&Stmt) -> bool + Copy,
{
    if f(stmt) {
        return true;
    }
    match stmt {
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ForLoop { body, .. } => body.iter().any(|s| find_stmt(s, f)),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            then_branch.iter().any(|s| find_stmt(s, f))
                || else_branch.as_ref().is_some_and(|b| b.iter().any(|s| find_stmt(s, f)))
        },
        Stmt::Match { arms, .. } => {
            arms.iter().any(|(_, body)| body.iter().any(|s| find_stmt(s, f)))
        },
        _ => false,
    }
}
