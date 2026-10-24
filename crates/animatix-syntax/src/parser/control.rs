//! Control-flow statement parsers (`if`, `match`, `for`) split out of the main
//! statement parser. Each takes the parser handles it closes over.

use chumsky::prelude::*;

use super::common::{self, ExprParser, ModifiersParser, ParserExtra, StrInput};
use super::token_parser::*;
use crate::ast::*;
use crate::occurrence::OccurrenceKind;

/// `if <expr> { … } else { … }`.
pub(crate) fn conditional_stmt<'src>(
    expr: ExprParser<'src>,
    scoped_always_body: impl Parser<'src, StrInput<'src>, Vec<Stmt>, ParserExtra<'src>> + Clone,
) -> impl Parser<'src, StrInput<'src>, Stmt, ParserExtra<'src>> + Clone {
    keyword("if")
        .ignore_then(expr.clone())
        .then(scoped_always_body.clone())
        .then(keyword("else").ignore_then(scoped_always_body.clone()).or_not())
        .map(|((condition, then_branch), else_branch)| Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span: None,
        })
        .labelled("if statement")
        .as_context()
}

/// `match <expr> { pat => { … }, … }`.
pub(crate) fn match_stmt<'src>(
    expr: ExprParser<'src>,
    scoped_always_body: impl Parser<'src, StrInput<'src>, Vec<Stmt>, ParserExtra<'src>> + Clone,
) -> impl Parser<'src, StrInput<'src>, Stmt, ParserExtra<'src>> + Clone {
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
        .then({
            let match_arm = match_pat.clone().then_ignore(arrow()).then(scoped_always_body.clone());
            match_arm
                .clone()
                .then(comma().or_not())
                .repeated()
                .at_least(1)
                .collect::<Vec<_>>()
                .map(|items| {
                    items
                        .into_iter()
                        .map(|(arm, _comma)| arm)
                        .collect::<Vec<(MatchPattern, Vec<Stmt>)>>()
                })
                .delimited_by(lbrace(), rbrace())
        })
        .map(|(scrutinee, arms)| Stmt::Match {
            scrutinee,
            arms,
            span: None,
        })
        .labelled("match statement")
        .as_context()
}

/// `for x[, i] in <expr> [modifiers] { … }`.
pub(crate) fn for_stmt<'src, B>(
    expr: ExprParser<'src>,
    modifiers: ModifiersParser<'src>,
    scoped_for_loop_body: B,
) -> impl Parser<'src, StrInput<'src>, Stmt, ParserExtra<'src>> + Clone
where
    B: Parser<'src, StrInput<'src>, Vec<Stmt>, ParserExtra<'src>> + Clone + 'src,
{
    let loop_var_pat = common::ident_decl_occ(OccurrenceKind::Variable)
        .clone()
        .map(LoopPattern::Single)
        .or(common::ident_decl_occ(OccurrenceKind::Variable)
            .clone()
            .separated_by(comma())
            .collect::<Vec<_>>()
            .delimited_by(lparen(), rparen())
            .map(LoopPattern::Tuple));

    keyword("for")
        .ignore_then(common::scoped(
            loop_var_pat
                .clone()
                .then(
                    comma()
                        .ignore_then(common::ident_decl_occ(OccurrenceKind::Variable).clone())
                        .or_not(),
                )
                .then_ignore(keyword("in"))
                .then(expr.clone())
                .then(modifiers.clone())
                .then(scoped_for_loop_body),
        ))
        .map(|((((var, index_var), iterable), modifiers), body)| Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            span: None,
        })
        .labelled("for loop")
        .as_context()
}
