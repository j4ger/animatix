//!
//! Expression parser for the Animatix DSL.
//!
//! Provides the full expression parser including atom parsers (numbers, percents,
//! strings, booleans, null) and the recursive expression parser with operator
//! precedence, tuple/array literals, calls, constructors, closures, and conditionals.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{self, ExprParser, ParserExtra, StrInput};
use super::token_parser::*;
use crate::ast::*;
use crate::occurrence::OccurrenceKind;

/// Build the expression parser.
///
/// Returns a boxed parser that parses any expression in the `.amx` DSL.
pub(crate) fn parser<'src>() -> ExprParser<'src> {
    recursive(|expr| {
        let expr: ExprParser<'src> = expr.boxed();
        let access = access_parser(&expr, atom_parsers(&expr));
        let logical = binary_chain(unary_parser(access));
        let closure = closure_parser(&expr);
        let conditional_expr = conditional_parser(&expr);
        let match_expr = match_parser(&expr);

        choice((closure, conditional_expr, match_expr, logical))
            .labelled("expression")
            .as_context()
            .boxed()
    })
    .boxed()
}

/// Literal, reference and collection atom parsers.
///
/// Covers numbers, percents, strings, booleans, null, `&actor.prop` references,
/// calls, tuple/array literals and type constructors. Every recursive position
/// uses `expr`.
fn atom_parsers<'src>(expr: &ExprParser<'src>) -> ExprParser<'src> {
    let variable_ident = common::ident_occ(OccurrenceKind::Variable);

    let num = number().map(Expr::Num);
    let percent = percent().map(Expr::Percent);
    let str_val = common::string_literal();
    let bool_val = bool_lit().map(Expr::Bool);
    let null_val = null().to(Expr::Null);

    let tuple = expr
        .clone()
        .separated_by(comma())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(lparen(), rparen())
        .map(|items| {
            if items.len() == 1 {
                items.into_iter().next().unwrap_or(Expr::Tuple(Vec::new()))
            } else {
                Expr::Tuple(items)
            }
        })
        .boxed();

    let array = expr
        .clone()
        .separated_by(comma())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(lbrace(), rbrace())
        .map(Expr::List)
        .boxed();

    let call = common::ident()
        .map_with(|name, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (name, extra.span())
        })
        .then(
            expr.clone()
                .separated_by(comma())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen()),
        )
        .map(|((name, span), args)| {
            crate::occurrence::record(OccurrenceKind::Function, name.clone(), span);
            Expr::Call(name, args)
        })
        .boxed();

    let construct = common::ident()
        .map_with(|name, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (name, extra.span())
        })
        .filter(|(name, _): &(String, ByteSpan)| {
            name.chars().next().is_some_and(|c| c.is_uppercase())
        })
        .map_with(|(name, span), _: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            crate::occurrence::record(OccurrenceKind::Type, name.clone(), span);
            name
        })
        .then(
            common::dotted_ident_occ(OccurrenceKind::Property)
                .then_ignore(colon())
                .then(expr.clone())
                .map(|(parts, value)| Property {
                    name: parts.join("."),
                    value,
                    value_span: None,
                    trailing_comment: None,
                })
                .separated_by(comma())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(lbrace(), rbrace()),
        )
        .map(|(name, props)| Expr::Construct(name, props))
        .labelled("type constructor")
        .as_context()
        .boxed();

    // Property reference: `&actor.prop` — strictly a two-segment
    // `label.property` path. Components have no independent animation
    // state, so `&a.b.c` is a parse error rather than a silent misread.
    let prop_ref = amp()
        .ignore_then(common::ident())
        .then(dot().ignore_then(common::ident()))
        .map(|(label, prop)| Expr::Unary(UnaryOp::Ref, Box::new(Expr::Path(vec![label, prop]))))
        .labelled("property reference");

    choice((
        prop_ref,
        percent,
        num,
        str_val,
        bool_val,
        null_val,
        call,
        tuple,
        array,
        construct,
        variable_ident.map(Expr::Ident),
    ))
    .boxed()
}

/// Field/method/index access chain applied to `base_atom`.
fn access_parser<'src>(expr: &ExprParser<'src>, base_atom: ExprParser<'src>) -> ExprParser<'src> {
    let ident = common::ident();

    #[derive(Clone)]
    enum PostfixStep {
        Field(String, Option<Vec<Expr>>),
        Index(Expr),
    }
    let field_segment = ident.clone().map_with(
        |seg, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (seg, extra.span())
        },
    );
    let postfix_step = choice((
        dot()
            .ignore_then(field_segment)
            .then(
                expr.clone()
                    .separated_by(comma())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(lparen(), rparen())
                    .or_not(),
            )
            .map(|((seg, span), args)| {
                crate::occurrence::record(
                    if args.is_some() {
                        OccurrenceKind::Function
                    } else {
                        OccurrenceKind::Property
                    },
                    seg.clone(),
                    span,
                );
                PostfixStep::Field(seg, args)
            }),
        lbracket()
            .ignore_then(expr.clone())
            .then_ignore(rbracket())
            .map(PostfixStep::Index),
    ));

    base_atom
        .foldl(postfix_step.repeated(), |base, step| match step {
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
        })
        .boxed()
}

/// Prefix operators (`-`, `!`) applied to an access chain.
fn unary_parser<'src>(access: ExprParser<'src>) -> ExprParser<'src> {
    let prefix_op = minus().to(UnaryOp::Neg).or(not().to(UnaryOp::Not));

    prefix_op
        .repeated()
        .collect::<Vec<_>>()
        .then(access)
        .map(|(ops, expr)| {
            ops.into_iter().rev().fold(expr, |acc, op| Expr::Unary(op, Box::new(acc)))
        })
        .boxed()
}

/// Binary operator precedence chain, from `^` (tightest) down to `&&`/`||`.
fn binary_chain<'src>(unary: ExprParser<'src>) -> ExprParser<'src> {
    let pow = recursive(|pow| {
        unary
            .clone()
            .then(caret().to(BinaryOp::Pow).then(pow).or_not())
            .map(|(lhs, rhs)| {
                if let Some((op, rhs)) = rhs {
                    Expr::Binary(Box::new(lhs), op, Box::new(rhs))
                } else {
                    lhs
                }
            })
    })
    .boxed();

    let product = pow.clone().foldl(
        choice((
            star().to(BinaryOp::Mul),
            slash().to(BinaryOp::Div),
            percent_op().to(BinaryOp::Mod),
        ))
        .then(pow.clone())
        .repeated(),
        |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
    );

    let sum = product.clone().foldl(
        choice((plus().to(BinaryOp::Add), minus().to(BinaryOp::Sub)))
            .then(product.clone())
            .repeated(),
        |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
    );

    let compare_op = choice((
        ge().to(BinaryOp::Gte),
        le().to(BinaryOp::Lte),
        eq().to(BinaryOp::Eq),
        neq().to(BinaryOp::Neq),
        gt().to(BinaryOp::Gt),
        lt().to(BinaryOp::Lt),
    ));

    let comparison =
        sum.clone().foldl(compare_op.then(sum.clone()).repeated(), |lhs, (op, rhs)| {
            Expr::Binary(Box::new(lhs), op, Box::new(rhs))
        });

    comparison
        .clone()
        .foldl(
            choice((and().to(BinaryOp::And), or().to(BinaryOp::Or)))
                .then(comparison.clone())
                .repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        )
        .boxed()
}

/// `if <expr> { … } else { … }` conditional expression.
fn conditional_parser<'src>(expr: &ExprParser<'src>) -> ExprParser<'src> {
    keyword("if")
        .ignore_then(expr.clone())
        .then(expr.clone().delimited_by(lbrace(), rbrace()))
        .then(keyword("else").ignore_then(expr.clone().delimited_by(lbrace(), rbrace())))
        .map(|((condition, then_branch), else_branch)| {
            Expr::Conditional(Box::new(condition), Box::new(then_branch), Box::new(else_branch))
        })
        .boxed()
}

/// Closure expression: `(a, b) => body`, `a => body`, with optional block body.
fn closure_parser<'src>(expr: &ExprParser<'src>) -> ExprParser<'src> {
    let ident = common::ident();
    let spanned_ident = ident.clone().map_with(
        |name, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (name, extra.span())
        },
    );

    choice((
        spanned_ident
            .clone()
            .separated_by(comma())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(lparen(), rparen()),
        spanned_ident.clone().map(|i| vec![i]),
    ))
    .then_ignore(arrow())
    .map_with(|params, _: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
        crate::occurrence::push_scope();
        for (name, span) in &params {
            crate::occurrence::record_declaration(OccurrenceKind::Parameter, name.clone(), *span);
        }
        params
    })
    .then(
        // Block body: `{ let a = …; let b = …; tail }`. Only `{ let` opens
        // a block — `{1, 2}` remains a list literal (zero compat break).
        closure_let_chain(expr.clone())
            .or(expr.clone())
            .then(empty::<StrInput<'src>, ParserExtra<'src>>().map_with(
                |(), _: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                    crate::occurrence::pop_scope();
                },
            ))
            .map(|(body, ())| body),
    )
    .map(|(params, body)| {
        Expr::Closure(params.into_iter().map(|(name, _)| name).collect(), Box::new(body))
    })
    .boxed()
}

/// `match <expr> { pattern => expr, … }` expression, including pattern grammar.
fn match_parser<'src>(expr: &ExprParser<'src>) -> ExprParser<'src> {
    let match_pat = recursive(|match_pat| {
        let wildcard = underscore().to(MatchPattern::Wildcard);
        let num_pat = number().map(MatchPattern::Num);
        let str_pat = super::common::string_literal().map(|e| match e {
            Expr::Str(s) => MatchPattern::Str(s),
            _ => unreachable!(),
        });
        let bool_pat = bool_lit().map(MatchPattern::Bool);

        let literal_pat = choice((num_pat, str_pat, bool_pat)).boxed();

        let range_pat = literal_pat
            .clone()
            .then_ignore(range_inclusive())
            .then(literal_pat.clone())
            .map(|(lo, hi)| MatchPattern::Range(Box::new(lo), Box::new(hi)))
            .boxed();

        let tuple_pat = match_pat
            .clone()
            .separated_by(comma())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(lparen(), rparen())
            .map(MatchPattern::Tuple)
            .boxed();

        let atom = choice((wildcard.clone(), range_pat, tuple_pat, literal_pat)).boxed();

        atom.clone()
            .foldl(pipe().ignore_then(atom.clone()).repeated(), |left, right| match left {
                MatchPattern::Or(mut items) => {
                    items.push(right);
                    MatchPattern::Or(items)
                },
                other => MatchPattern::Or(vec![other, right]),
            })
            .boxed()
    });

    keyword("match")
        .ignore_then(expr.clone())
        .then(
            match_pat
                .clone()
                .then_ignore(arrow())
                .then(expr.clone())
                .separated_by(comma())
                .allow_trailing()
                .collect::<Vec<(MatchPattern, _)>>()
                .delimited_by(lbrace(), rbrace()),
        )
        .map(|(scrutinee, arms)| {
            Expr::Match(
                Box::new(scrutinee),
                arms.into_iter().map(|(p, e)| (p, Box::new(e))).collect(),
            )
        })
        .boxed()
}

/// Parse a closure block body: `{ let a = …; let b = …; tail }`.
///
/// One or more `let` bindings followed by the tail expression whose value is
/// the block's value. Statements are newline-separated (the `let` keyword
/// terminates the previous expression, mirroring pure-fn bodies). Statements
/// other than `let` are not supported here — pure `fn` bodies cover those.
fn closure_let_chain<'src, P>(
    expr: P,
) -> impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone
where
    P: Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone,
{
    let binding = keyword("let")
        .ignore_then(common::ident_decl_occ(OccurrenceKind::Variable).clone())
        .then_ignore(assign())
        .then(expr.clone())
        .map(|(name, value)| (name, value))
        .labelled("let binding");

    binding
        .repeated()
        .at_least(1)
        .collect::<Vec<_>>()
        .then(expr)
        .delimited_by(lbrace(), rbrace())
        .map(|(bindings, tail)| Expr::LetChain(bindings, Box::new(tail)))
        .labelled("closure let block")
}
