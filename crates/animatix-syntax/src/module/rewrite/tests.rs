use super::*;

mod tests {
    use super::*;

    #[test]
    fn rewrite_media_statement_position_expressions() {
        let stmt = Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "logo".to_string(),
            array_index: None,
            ty: "Svg".to_string(),
            props: vec![
                Property {
                    name: "url".to_string(),
                    value: Expr::Str("examples/vector.svg".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Ident("badge".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "top".to_string()]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Ident("delta".to_string())]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "scale".to_string(),
                    value: Expr::Num(1.0),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        };
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
}
