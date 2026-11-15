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
