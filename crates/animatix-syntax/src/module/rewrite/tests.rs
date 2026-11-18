use super::*;

use crate::test_support::actor_with_props;

#[test]
fn rewrite_media_statement_position_expressions() {
    let stmt = actor_with_props(
        "logo",
        "Svg",
        vec![
            Property::new("url", Expr::Str("examples/vector.svg".to_string())),
            Property::new("at", Expr::Ident("badge".to_string())),
            Property::new("anchor", Expr::Path(vec!["scene".to_string(), "top".to_string()])),
            Property::new(
                "offset",
                Expr::Tuple(vec![Expr::Num(0.0), Expr::Ident("delta".to_string())]),
            ),
            Property::new("scale", Expr::Num(1.0)),
        ],
    );
    let known_labels = HashSet::from(["logo".to_string(), "badge".to_string()]);
    let bindings = HashMap::from([("delta".to_string(), Expr::Num(48.0))]);

    let rewritten = rewrite_stmt(&stmt, "hero", None, &known_labels, &bindings);

    match rewritten {
        Stmt::ActorDecl {
            label, ty, props, ..
        } => {
            assert_eq!(label, "hero.logo");
            assert_eq!(ty, "Svg");
            let get_prop = |name: &str| -> Option<&Expr> {
                props.iter().find(|p| p.name == name).map(|p| &p.value)
            };
            assert_eq!(get_prop("at"), Some(&Expr::Ident("hero.badge".to_string())));
            assert_eq!(
                get_prop("anchor"),
                Some(&Expr::Path(vec!["scene".to_string(), "top".to_string()]))
            );
            assert_eq!(
                get_prop("offset"),
                Some(&Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]))
            );
        },
        other => unreachable!("expected rewritten actor decl statement, got {other:?}"),
    }
}

#[test]
fn self_member_paths_add_the_component_prefix_once() {
    let expression = Expr::Field(
        Box::new(Expr::Field(Box::new(Expr::Ident("self".to_string())), "badge".to_string())),
        "read".to_string(),
    );
    let rewritten = rewrite_expr(
        &expression,
        "cards",
        Some("background"),
        &HashSet::from(["badge".to_string()]),
        &HashMap::new(),
    );
    assert_eq!(
        rewritten,
        Expr::Path(vec!["cards".to_string(), "badge".to_string(), "read".to_string(),])
    );
}

// Exercise the source parser as well as the rewrite, since literal numeric
// indexes are normalized to Static labels while runtime indexes stay Indexed.
#[test]
fn self_root_targets_preserve_all_descendant_segments() {
    fn static_segment(name: &str) -> TargetSegment {
        TargetSegment::Static(name.to_string())
    }
    fn indexed(base: &str, name: &str) -> TargetSegment {
        TargetSegment::Indexed {
            base: base.to_string(),
            index: Box::new(Expr::Ident(name.to_string())),
        }
    }
    let cases = [
        (
            "self.root.cells[i].arm",
            vec![
                static_segment("self"),
                static_segment("root"),
                indexed("cells", "i"),
                static_segment("arm"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                indexed("cells", "i"),
                static_segment("arm"),
            ],
        ),
        (
            "self.cells[i].arm",
            vec![
                static_segment("self"),
                indexed("cells", "i"),
                static_segment("arm"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                indexed("cells", "i"),
                static_segment("arm"),
            ],
        ),
        (
            "self.root.rows[i].cells[j].arm",
            vec![
                static_segment("self"),
                static_segment("root"),
                indexed("rows", "i"),
                indexed("cells", "j"),
                static_segment("arm"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                indexed("rows", "i"),
                indexed("cells", "j"),
                static_segment("arm"),
            ],
        ),
        (
            "self.root.cells[0].arm",
            vec![
                static_segment("self"),
                static_segment("root"),
                static_segment("cells__0"),
                static_segment("arm"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                static_segment("cells__0"),
                static_segment("arm"),
            ],
        ),
        (
            "self.root.cells.arm",
            vec![
                static_segment("self"),
                static_segment("root"),
                static_segment("cells"),
                static_segment("arm"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                static_segment("cells"),
                static_segment("arm"),
            ],
        ),
        // Compatibility guard, not an endorsement of the existing descendant
        // root-name rewrite rule: this patch changes only index preservation.
        (
            "self.root.cells.root",
            vec![
                static_segment("self"),
                static_segment("root"),
                static_segment("cells"),
                static_segment("root"),
            ],
            vec![
                static_segment("deck"),
                static_segment("card"),
                static_segment("cells"),
                static_segment("deck.card"),
            ],
        ),
    ];
    for (path, parsed_target, expected) in cases {
        for operator in ["=", ":="] {
            let source = format!("{path}.opacity {operator} 0.5");
            let (statements, errors) = crate::parser::parse_source(&source);
            assert!(errors.is_empty(), "{source}: {errors:?}");
            let statements = statements.expect("source parses");
            let mut assignments = Vec::new();
            crate::walk::walk_stmts(&statements, &mut |stmt| {
                if matches!(stmt, Stmt::Assignment { .. } | Stmt::ReactiveBinding { .. }) {
                    assignments.push(stmt.clone());
                }
            });
            assert_eq!(assignments.len(), 1, "{source}");
            let original = &assignments[0];
            let (target, property) = match original {
                Stmt::Assignment { target, property, .. }
                | Stmt::ReactiveBinding { target, property, .. } => (target, property),
                _ => unreachable!(),
            };
            assert_eq!(target, &parsed_target, "{source}");
            assert_eq!(property, "opacity", "{source}");
            assert_eq!(matches!(original, Stmt::ReactiveBinding { .. }), operator == ":=");
            let rewritten = rewrite_stmt(
                original,
                "deck.card",
                Some("root"),
                &HashSet::new(),
                &HashMap::new(),
            );
            let (target, property) = match rewritten {
                Stmt::Assignment { target, property, .. }
                | Stmt::ReactiveBinding { target, property, .. } => (target, property),
                _ => unreachable!(),
            };
            assert_eq!(target, expected, "{source}");
            assert_eq!(property, "opacity", "{source}");
        }
    }
}
