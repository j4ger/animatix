use crate::ast::*;
use chumsky::prelude::*;

pub fn parser<'src>() -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let ident = text::ident()
        .then(just('-').then(text::ident()).repeated())
        .to_slice()
        .map(String::from)
        .padded();

    let num = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .map(Expr::Num)
        .padded();

    let str_val = just('"')
        .ignore_then(none_of('"').repeated().collect::<String>())
        .then_ignore(just('"'))
        .map(Expr::Str)
        .padded();

    let bool_val = text::keyword("true")
        .to(Expr::Bool(true))
        .or(text::keyword("false").to(Expr::Bool(false)))
        .padded();

    let null_val = text::keyword("null").to(Expr::Null).padded();

    let time = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str::<f64>()
        .unwrapped()
        .then(choice((just("ms").to(true), just("s").to(false))))
        .map(|(v, is_ms)| {
            if is_ms {
                Time::Milliseconds(v as u64)
            } else {
                Time::Seconds(v)
            }
        })
        .padded();

    let expr = recursive(|expr| {
        let tuple = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('(').padded(), just(')').padded())
            .map(Expr::Tuple);

        let array = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded())
            .map(Expr::Tuple); // Using Tuple for arrays as well per AST

        let atom = choice((
            num,
            str_val,
            bool_val,
            null_val,
            tuple,
            array,
            ident.clone().map(Expr::Ident),
        ))
        .padded();

        // Method calls and indexing
        let access = atom.foldl(
            choice((just('.')
                .ignore_then(ident.clone())
                .then(
                    expr.clone()
                        .separated_by(just(',').padded())
                        .collect::<Vec<_>>()
                        .delimited_by(just('(').padded(), just(')').padded())
                        .or_not(),
                )
                .map(|(name, args)| {
                    if let Some(_args) = args {
                        // Method call: obj.method(args)
                        vec![name, "()".to_string()] // A bit of a hack to pass through foldl
                    } else {
                        // Path access: obj.field
                        vec![name]
                    }
                }),))
            .repeated(),
            |acc, parts| {
                // Simplified Path construction for AST matching
                if parts.len() == 1 {
                    Expr::Path(vec![
                        match acc {
                            Expr::Ident(s) => s,
                            _ => "".to_string(), // Simplified
                        },
                        parts[0].clone(),
                    ])
                } else {
                    acc
                }
            },
        );

        // We can add operators here, but for brevity we stick to the basic atoms and paths
        access
    });

    let property = ident
        .clone()
        .then_ignore(just(':').padded())
        .then(expr.clone())
        .map(|(name, value)| Property { name, value });

    let modifier = choice((
        // named modifier: ease: bounce
        ident
            .clone()
            .then_ignore(just(':').padded())
            .then(expr.clone())
            .map(|(name, value)| Modifier {
                name: Some(name),
                value,
            }),
        // positional modifier: 2s
        time.clone().map(|t| Modifier {
            name: None,
            value: match t {
                Time::Seconds(s) => Expr::Ident(format!("{}s", s)),
                Time::Milliseconds(ms) => Expr::Ident(format!("{}ms", ms)),
            },
        }),
        expr.clone().map(|value| Modifier { name: None, value }),
    ));

    let modifiers = modifier
        .separated_by(just(',').padded())
        .collect::<Vec<_>>()
        .delimited_by(just('[').padded(), just(']').padded())
        .or_not()
        .map(|m| m.unwrap_or_default());

    let stmt = recursive(|_stmt| {
        let let_decl = text::keyword("let")
            .ignore_then(ident.clone())
            .then_ignore(just('=').padded())
            .then(expr.clone())
            .map(|(name, value)| Stmt::LetDecl { name, value })
            .padded();

        let assignment = ident
            .clone()
            .then_ignore(just('.'))
            .then(ident.clone())
            .then_ignore(just('=').padded())
            .then(expr.clone())
            .map(|((target, property), value)| Stmt::Assignment {
                target,
                property,
                value,
            })
            .padded();

        let actor_decl = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(ident.clone())
            .then(
                just(',')
                    .padded()
                    .ignore_then(
                        property
                            .clone()
                            .separated_by(just(',').padded())
                            .collect::<Vec<_>>(),
                    )
                    .or_not()
                    .map(|p| p.unwrap_or_default()),
            )
            .then(modifiers.clone())
            .map(|(((label, ty), props), modifiers)| Stmt::ActorDecl {
                label,
                ty,
                props,
                modifiers,
                children: vec![], // Inline children left out for brevity in 0.25 mode
            })
            .padded();

        let action = ident
            .clone()
            .then(ident.clone().repeated().collect::<Vec<_>>()) // Simplified targets
            .then(modifiers.clone())
            .map(|((verb, targets), modifiers)| {
                Stmt::Action(Action {
                    verb,
                    targets,
                    args: vec![],
                    modifiers,
                })
            })
            .padded();

        let comment = just("//")
            .ignore_then(none_of("\r\n").repeated().to_slice().map(String::from))
            .map(Stmt::Comment)
            .padded();

        choice((let_decl, assignment, actor_decl, action, comment))
    });

    let keyframe = just('#')
        .ignore_then(time.clone())
        .then(stmt.clone().repeated().collect::<Vec<_>>())
        .map(|(t, body)| Stmt::Keyframe { time: t, body })
        .padded();

    // Top-level can be keyframes or standalone statements
    choice((
        keyframe,
        stmt.map(|s| Stmt::Keyframe {
            time: Time::Seconds(0.0), // default timeline wrapper
            body: vec![s],
        }),
    ))
    .repeated()
    .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyframe_with_stmts() {
        let src = r#"
            #1.5s
            let x = 42
            circle: Circle, radius: 50 [2s]
        "#;
        let ast = parser().parse(src).into_result().unwrap();
        assert_eq!(ast.len(), 1);
        if let Stmt::Keyframe { time, body } = &ast[0] {
            assert_eq!(*time, Time::Seconds(1.5));
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_let_decl() {
        let src = "let x = 42";
        let ast = parser().parse(src).into_result().unwrap();
        if let Stmt::Keyframe { body, .. } = &ast[0] {
            assert_eq!(
                body[0],
                Stmt::LetDecl {
                    name: "x".to_string(),
                    value: Expr::Num(42.0),
                }
            );
        }
    }

    #[test]
    fn test_actor_decl_modifiers() {
        let src = "circle: Circle, radius: 50 [ease: bounce]";
        let ast = parser().parse(src).into_result().unwrap();
        if let Stmt::Keyframe { body, .. } = &ast[0] {
            assert_eq!(
                body[0],
                Stmt::ActorDecl {
                    label: "circle".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(50.0),
                    }],
                    modifiers: vec![Modifier {
                        name: Some("ease".to_string()),
                        value: Expr::Ident("bounce".to_string())
                    }],
                    children: vec![],
                }
            );
        }
    }
}
