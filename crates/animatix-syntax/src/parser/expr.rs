//!
//! Expression parser for the Animatix DSL.
//!
//! Provides the full expression parser including atom parsers (numbers, percents,
//! strings, booleans, null) and the recursive expression parser with operator
//! precedence, tuple/array literals, calls, constructors, closures, and conditionals.
//!

use crate::ast::*;
use chumsky::prelude::*;
use super::common::{self, ExprParser};

/// Build the expression parser.
///
/// Returns a boxed parser that parses any expression in the `.amx` DSL.
pub(crate) fn parser<'src>() -> ExprParser<'src> {
    let ident = common::ident();
    let dotted_ident = common::dotted_ident();
    let str_val = common::string_literal();

    let num = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .map(Expr::Num)
        .padded();

    let percent = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .then_ignore(just('%'))
        .map(Expr::Percent)
        .padded();

    let bool_val = text::keyword("true")
        .to(Expr::Bool(true))
        .or(text::keyword("false").to(Expr::Bool(false)))
        .padded();

    let null_val = text::keyword("null").to(Expr::Null).padded();

    recursive(|expr| {
        let tuple = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('(').padded(), just(')').padded())
            .map(|items| {
                if items.len() == 1 {
                    items.into_iter().next().expect("tuple with len==1 has one item")
                } else {
                    Expr::Tuple(items)
                }
            })
            .boxed();

        let array = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded())
            .map(Expr::Tuple) // Using Tuple for arrays as well per AST
            .boxed();

        let call = ident
            .clone()
            .then(
                expr.clone()
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just('(').padded(), just(')').padded()),
            )
            .map(|(name, args)| Expr::Call(name, args))
            .boxed();

        // Type construction expression: TypeName { prop1: val1, prop2: val2 }
        let construct = ident
            .clone()
            .filter(|s: &String| s.chars().next().is_some_and(|c| c.is_uppercase()))
            .then(
                dotted_ident
                    .clone()
                    .then_ignore(just(':').padded())
                    .then(expr.clone())
                    .map(|(parts, value)| {
                        let name = parts.join(".");
                        Property { name, value, value_span: None, trailing_comment: None }
                    })
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(name, props)| Expr::Construct(name, props))
            .labelled("type constructor")
            .as_context()
            .boxed();

        // Prefix operators for unary negation and logical NOT
        let prefix_op = just('-').to(UnaryOp::Neg).or(just('!').to(UnaryOp::Not));

        let base_atom = choice((
            percent,
            num,
            str_val,
            bool_val,
            null_val,
            call,
            tuple,
            array,
            construct,
            ident.clone().map(Expr::Ident),
        ));

        // Prefix expressions: fold multiple prefix ops around an atom
        let atom = prefix_op
            .padded()
            .repeated()
            .collect::<Vec<_>>()
            .then(base_atom)
            .map(|(ops, expr)| {
                ops.into_iter()
                    .fold(expr, |acc, op| Expr::Unary(op, Box::new(acc)))
            })
            .padded();

        let access = atom
            .clone()
            .then(
                just('.')
                    .padded()
                    .ignore_then(ident.clone())
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map(|(base, segments)| {
                if segments.is_empty() {
                    base
                } else {
                    match base {
                        Expr::Ident(name) => {
                            let mut parts = Vec::with_capacity(segments.len() + 1);
                            parts.push(name);
                            parts.extend(segments);
                            Expr::Path(parts)
                        }
                        Expr::Path(mut parts) => {
                            parts.extend(segments);
                            Expr::Path(parts)
                        }
                        other => {
                            // Field access on non-ident/path base: build Method chain
                            segments.into_iter().fold(other, |acc, segment| {
                                Expr::Method(Box::new(acc), segment, Vec::new())
                            })
                        }
                    }
                }
            });

        // Mathematical and logical operators precedence
        let pow = recursive(|pow| {
            access
                .clone()
                .then(just('^').padded().to(BinaryOp::Pow).then(pow).or_not())
                .map(|(lhs, rhs)| {
                    if let Some((op, rhs)) = rhs {
                        Expr::Binary(Box::new(lhs), op, Box::new(rhs))
                    } else {
                        lhs
                    }
                })
        });

        let product = pow.clone().foldl(
            choice((
                just('*').to(BinaryOp::Mul),
                just('/').to(BinaryOp::Div),
                just('%').to(BinaryOp::Mod),
            ))
            .padded()
            .then(pow.clone())
            .repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let sum = product.clone().foldl(
            choice((just('+').to(BinaryOp::Add), just('-').to(BinaryOp::Sub)))
                .padded()
                .then(product.clone())
                .repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let compare_op = choice((
            just(">=").to(BinaryOp::Gte),
            just("<=").to(BinaryOp::Lte),
            just("==").to(BinaryOp::Eq),
            just("!=").to(BinaryOp::Neq),
            just('>').to(BinaryOp::Gt),
            just('<').to(BinaryOp::Lt),
        ));

        let comparison = sum.clone().foldl(
            compare_op.padded().then(sum.clone()).repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let conditional_expr = text::keyword("if")
            .ignore_then(expr.clone())
            .then(
                expr.clone()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .then(
                text::keyword("else").ignore_then(
                    expr.clone()
                        .delimited_by(just('{').padded(), just('}').padded()),
                ),
            )
            .map(|((condition, then_branch), else_branch)| {
                Expr::Conditional(
                    Box::new(condition),
                    Box::new(then_branch),
                    Box::new(else_branch),
                )
            })
            .boxed();

        let closure = choice((
            ident
                .clone()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just('(').padded(), just(')').padded()),
            ident.clone().map(|i| vec![i]),
        ))
        .then_ignore(just("=>").padded())
        .then(expr.clone())
        .map(|(args, body)| Expr::Closure(args, Box::new(body)))
        .boxed();

        choice((closure, conditional_expr, comparison))
            .labelled("expression")
            .as_context()
            .boxed()
    })
    .boxed()
}
