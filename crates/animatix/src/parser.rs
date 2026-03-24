use crate::ast::{Expr, Property, Stmt, Time};
use chumsky::prelude::*;

pub fn parser<'src>() -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let ident = text::ident().map(String::from).padded();

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

    let expr = recursive(|expr| {
        let tuple = expr
            .separated_by(just(',').padded())
            .collect::<Vec<_>>()
            .delimited_by(just('(').padded(), just(')').padded())
            .map(Expr::Tuple);

        choice((num, str_val, ident.clone().map(Expr::Ident), tuple))
    });

    let keyframe = just('#')
        .ignore_then(
            text::int(10)
                .then(just('.').ignore_then(text::digits(10)).or_not())
                .to_slice()
                .from_str()
                .unwrapped(),
        )
        .then_ignore(just('s'))
        .map(|v: f64| Stmt::Keyframe {
            time: Time::Seconds(v),
            body: vec![],
        })
        .padded();

    let let_decl = text::keyword("let")
        .ignore_then(ident.clone())
        .then_ignore(just('=').padded())
        .then(expr.clone())
        .map(|(name, value)| Stmt::LetDecl { name, value })
        .padded();

    let property = ident
        .clone()
        .then_ignore(just(':').padded())
        .then(expr.clone())
        .map(|(name, value)| Property { name, value });

    let actor_decl = ident
        .clone()
        .then_ignore(just(':').padded())
        .then(ident.clone())
        .then(
            just(',')
                .padded()
                .ignore_then(
                    property
                        .separated_by(just(',').padded())
                        .collect::<Vec<_>>(),
                )
                .or_not()
                .map(|p| p.unwrap_or_default()),
        )
        .map(|((label, ty), props)| Stmt::ActorDecl {
            label,
            ty,
            props,
            children: vec![],
        })
        .padded();

    let assignment = ident
        .clone()
        .then_ignore(just('.'))
        .then(ident.clone())
        .then_ignore(just('=').padded())
        .then(expr)
        .map(|((target, property), value)| Stmt::Assignment {
            target,
            property,
            value,
        })
        .padded();

    let stmt = choice((keyframe, let_decl, actor_decl, assignment));

    stmt.repeated().collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyframe() {
        let src = "#1.5s";
        let ast = parser().parse(src).into_result().unwrap();
        assert_eq!(
            ast,
            vec![Stmt::Keyframe {
                time: Time::Seconds(1.5),
                body: vec![]
            }]
        );
    }

    #[test]
    fn test_let_decl() {
        let src = "let x = 42";
        let ast = parser().parse(src).into_result().unwrap();
        assert_eq!(
            ast,
            vec![Stmt::LetDecl {
                name: "x".to_string(),
                value: Expr::Num(42.0),
            }]
        );
    }

    #[test]
    fn test_actor_decl() {
        let src = "circle: Circle, radius: 50";
        let ast = parser().parse(src).into_result().unwrap();
        assert_eq!(
            ast,
            vec![Stmt::ActorDecl {
                label: "circle".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(50.0),
                }],
                children: vec![],
            }]
        );
    }
}
