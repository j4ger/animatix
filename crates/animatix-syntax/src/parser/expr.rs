//!
//! Expression parser for the Animatix DSL.
//!
//! Provides the full expression parser including atom parsers (numbers, percents,
//! strings, booleans, null) and the recursive expression parser with operator
//! precedence, tuple/array literals, calls, constructors, closures, and conditionals.

use chumsky::prelude::*;

use super::common::{self, ExprParser};
use crate::ast::*;

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
            .map(Expr::List)
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
                        Property {
                            name,
                            value,
                            value_span: None,
                            trailing_comment: None,
                        }
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
            .map(|(ops, expr)| ops.into_iter().fold(expr, |acc, op| Expr::Unary(op, Box::new(acc))))
            .padded();

        // Postfix fold: field access, method calls, and subscript indexing.
        // Subscript `[` has no leading whitespace so `x[0]` indexes but
        // `fade-in x [300ms]` still parses the modifier list separately.
        #[derive(Clone)]
        enum PostfixStep {
            Field(String, Option<Vec<Expr>>),
            Index(Expr),
        }
        let postfix_step = choice((
            just('.')
                .padded()
                .ignore_then(ident.clone())
                .then(
                    atom.clone()
                        .separated_by(just(',').padded())
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .delimited_by(just('(').padded(), just(')').padded())
                        .or_not(),
                )
                .map(|(seg, args)| PostfixStep::Field(seg, args)),
            just('[')
                .ignore_then(expr.clone().padded())
                .then_ignore(just(']').padded())
                .map(PostfixStep::Index),
        ));

        let access = atom.clone().foldl(postfix_step.repeated(), |base, step| match step {
            PostfixStep::Field(segment, args) => {
                if let Some(args) = args {
                    Expr::Method(Box::new(base), segment, args)
                } else {
                    match base {
                        Expr::Ident(name) => Expr::Path(vec![name, segment]),
                        Expr::Path(mut parts) => {
                            parts.push(segment);
                            Expr::Path(parts)
                        },
                        other => Expr::Method(Box::new(other), segment, Vec::new()),
                    }
                }
            },
            PostfixStep::Index(index_expr) => Expr::Index(Box::new(base), Box::new(index_expr)),
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

        let comparison = sum
            .clone()
            .foldl(compare_op.padded().then(sum.clone()).repeated(), |lhs, (op, rhs)| {
                Expr::Binary(Box::new(lhs), op, Box::new(rhs))
            });

        let conditional_expr = text::keyword("if")
            .ignore_then(expr.clone())
            .then(expr.clone().delimited_by(just('{').padded(), just('}').padded()))
            .then(
                text::keyword("else")
                    .ignore_then(expr.clone().delimited_by(just('{').padded(), just('}').padded())),
            )
            .map(|((condition, then_branch), else_branch)| {
                Expr::Conditional(Box::new(condition), Box::new(then_branch), Box::new(else_branch))
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

        // Match pattern parser (for match arms)
        // Supports: wildcard `_`, literals (Num/Str/Bool), ranges `1..=3`,
        // or-patterns `0 | 2`, and tuple patterns `(a, b)`.
        let match_pat = recursive(|match_pat| {
            let wildcard = just('_').to(MatchPattern::Wildcard).padded();

            let num_pat = text::int(10)
                .then(just('.').ignore_then(text::digits(10)).or_not())
                .to_slice()
                .from_str()
                .unwrapped()
                .map(MatchPattern::Num)
                .padded();

            let str_pat = super::common::string_literal()
                .map(|e| match e {
                    Expr::Str(s) => MatchPattern::Str(s),
                    _ => unreachable!(),
                })
                .padded();

            let bool_pat = text::keyword("true")
                .to(MatchPattern::Bool(true))
                .or(text::keyword("false").to(MatchPattern::Bool(false)))
                .padded();

            let literal_pat = choice((num_pat, str_pat, bool_pat)).boxed();

            // Range: pat ..= pat (endpoints must be literals)
            let range_pat = literal_pat
                .clone()
                .then_ignore(just("..=").padded())
                .then(literal_pat.clone())
                .map(|(lo, hi)| MatchPattern::Range(Box::new(lo), Box::new(hi)))
                .boxed();

            // Tuple: (pat, pat, ...)
            let tuple_pat = match_pat
                .clone()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just('(').padded(), just(')').padded())
                .map(MatchPattern::Tuple)
                .boxed();

            // Atom: wildcard | literal | range | tuple
            let atom = choice((wildcard.clone(), range_pat, tuple_pat, literal_pat)).boxed();

            // Or-pattern: atom (| atom)*
            atom.clone()
                .foldl(just('|').padded().ignore_then(atom.clone()).repeated(), |left, right| {
                    match left {
                        MatchPattern::Or(mut items) => {
                            items.push(right);
                            MatchPattern::Or(items)
                        },
                        other => MatchPattern::Or(vec![other, right]),
                    }
                })
                .boxed()
        });

        // Match expression: match <expr> { <pat> => <expr> , ... , _ => <expr> }
        let match_expr = text::keyword("match")
            .ignore_then(expr.clone().padded())
            .then(
                match_pat
                    .clone()
                    .then_ignore(just("=>").padded())
                    .then(expr.clone())
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<(MatchPattern, _)>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(scrutinee, arms)| {
                Expr::Match(
                    Box::new(scrutinee),
                    arms.into_iter().map(|(p, e)| (p, Box::new(e))).collect(),
                )
            })
            .boxed();

        choice((closure, conditional_expr, match_expr, comparison))
            .labelled("expression")
            .as_context()
            .boxed()
    })
    .boxed()
}
