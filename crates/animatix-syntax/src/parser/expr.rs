//!
//! Expression parser for the Animatix DSL.
//!
//! Provides the full expression parser including atom parsers (numbers, percents,
//! strings, booleans, null) and the recursive expression parser with operator
//! precedence, tuple/array literals, calls, constructors, closures, and conditionals.

use chumsky::input::MapExtra;
use chumsky::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::common::{self, ExprParser, ParserExtra, StrInput};
use super::token_parser::*;
use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::occurrence::OccurrenceKind;
use crate::token::TokenKind;

enum PlanItem {
    Keyframe { relative: bool, time: Time },
    Statement(Stmt),
}

/// Build the expression parser.
///
/// Returns a boxed parser that parses any expression in the `.amx` DSL.
pub(crate) fn parser<'src>(
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
    source: common::SourceIndex<'src>,
) -> ExprParser<'src> {
    recursive(|expr| {
        let expr: ExprParser<'src> = expr.boxed();
        // Timeline-valued expressions contain the same statement grammar as
        // scenes. Build this nested statement parser from the recursive expr
        // handle so timeline plans can contain nested plans and normal calls.
        let property = common::property(expr.clone());
        let modifier = common::modifier(expr.clone(), common::time());
        let modifiers = common::modifiers(modifier);
        let inline = super::inline::parser(
            expr.clone(),
            property.clone(),
            modifiers.clone(),
            warnings.clone(),
            source.clone(),
        );
        let stmt =
            super::stmt::parser(expr.clone(), property, modifiers.clone(), inline, source.clone());
        let access = access_parser(
            expr.clone(),
            atom_parsers(expr.clone(), stmt.clone(), modifiers.clone()),
        );
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
fn atom_parsers<'src>(
    expr: ExprParser<'src>,
    stmt: super::common::StmtParser<'src>,
    modifiers: super::common::ModifiersParser<'src>,
) -> ExprParser<'src> {
    let variable_ident = common::ident_occ(OccurrenceKind::Variable);

    let num = number().map(Expr::Num);
    let percent = percent().map(Expr::Percent);
    let str_val = common::string_literal();
    let hex_color = hex_color_parser(expr.clone());
    let bool_val = bool_lit().map(Expr::Bool);
    let null_val = null().to(Expr::Null);

    let tuple = expr
        .clone()
        .separated_by(comma())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(lparen(), rparen())
        .map(|items| {
            if items.is_empty() {
                Expr::Unit
            } else if items.len() == 1 {
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

    let duration = select! {
        TokenKind::Time { value, ms } => Expr::Duration(if ms { value / 1000.0 } else { value }),
    }
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

    let plan_body = plan_body_parser(expr.clone(), stmt.clone());
    let timeline = hash()
        .ignore_then(plan_body.clone())
        .try_map(|body, span| {
            if body.iter().any(|stmt| {
                matches!(stmt, crate::ast::Stmt::Config { .. } | crate::ast::Stmt::Play { .. })
            }) {
                Err(Rich::custom(span, "config and play are only allowed at the scene/file level"))
            } else {
                Ok(Expr::Timeline(PlanBody::new(body)))
            }
        })
        .boxed();

    let sequence = keyword("sequence")
        .ignore_then(plan_body.clone())
        .map(|body| Expr::Sequence(PlanBody::new(body)))
        .boxed();

    let stagger = keyword("stagger")
        .ignore_then(modifiers.clone())
        .then(plan_body)
        .map(|(modifiers, body)| Expr::Stagger {
            modifiers,
            plan: PlanBody::new(body),
        })
        .boxed();

    choice((
        prop_ref,
        percent,
        duration,
        num,
        str_val,
        hex_color,
        bool_val,
        null_val,
        timeline,
        sequence,
        stagger,
        tuple,
        array,
        construct,
        variable_ident.map(Expr::Ident),
    ))
    .boxed()
}

fn hex_color_parser<'src>(expr: ExprParser<'src>) -> ExprParser<'src> {
    keyword("hex_color")
        .ignore_then(call_arguments(expr).delimited_by(lparen(), rparen()))
        .validate(|args, extra, emitter| {
            let span = args.first().and_then(|arg| arg.byte_span).unwrap_or_else(|| extra.span());
            let result = match args.as_slice() {
                [
                    CallArgument {
                        name: None,
                        value: Expr::Str(digits),
                        ..
                    },
                ] => parse_hex_color(digits),
                _ => Err("hex_color requires one quoted hex literal".into()),
            };
            match result {
                Ok(rgba) => Expr::Color(rgba),
                Err(message) => {
                    // Keep the diagnostic at the argument through parser recovery.
                    emitter.emit(Rich::custom(span, message));
                    Expr::Null
                },
            }
        })
        .boxed()
}

fn parse_hex_color(digits: &str) -> Result<[u8; 4], String> {
    if let Some(digits) = digits.strip_prefix('#') {
        let mut message = "Omit the leading '#' in hex_color.".to_string();
        if matches!(digits.len(), 6 | 8) && digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            message.push_str(&format!(r#" Write hex_color("{digits}")."#));
        }
        return Err(message);
    }
    if !matches!(digits.len(), 6 | 8) {
        return Err("hex_color requires RRGGBB or RRGGBBAA".into());
    }
    if let Some(invalid) = digits.chars().find(|digit| !digit.is_ascii_hexdigit()) {
        return Err(format!("Invalid hexadecimal digit {invalid:?} in hex_color"));
    }
    let value = u32::from_str_radix(digits, 16).map_err(|error| error.to_string())?;
    let rgba = if digits.len() == 6 {
        (value << 8) | 0xff
    } else {
        value
    };
    Ok(rgba.to_be_bytes())
}

fn plan_body_parser<'src>(
    expr: ExprParser<'src>,
    stmt: super::common::StmtParser<'src>,
) -> impl Parser<'src, StrInput<'src>, Vec<Stmt>, ParserExtra<'src>> + Clone {
    let keyframe = hash()
        .ignore_then(plus().or_not())
        .then(common::time_with_expr(expr.clone()))
        .then_ignore(colon().or_not())
        .map(|(relative, time)| PlanItem::Keyframe {
            relative: relative.is_some(),
            time,
        });
    let item = choice((keyframe, stmt.clone().map(PlanItem::Statement)));
    item.repeated()
        .collect::<Vec<_>>()
        .delimited_by(lbrace(), rbrace())
        .map(group_plan_items)
}

fn group_plan_items(items: Vec<PlanItem>) -> Vec<Stmt> {
    let mut body = Vec::new();
    let mut keyframe: Option<(bool, Time, Vec<Stmt>)> = None;

    for item in items {
        match item {
            PlanItem::Keyframe { relative, time } => {
                if let Some((was_relative, old_time, statements)) = keyframe.take() {
                    body.push(if was_relative {
                        Stmt::RelativeKeyframe {
                            offset: old_time,
                            body: statements,
                            span: None,
                        }
                    } else {
                        Stmt::Keyframe {
                            time: old_time,
                            body: statements,
                            span: None,
                        }
                    });
                }
                keyframe = Some((relative, time, Vec::new()));
            },
            PlanItem::Statement(statement) => {
                if let Some((_, _, statements)) = keyframe.as_mut() {
                    statements.push(statement);
                } else {
                    body.push(statement);
                }
            },
        }
    }

    if let Some((relative, time, statements)) = keyframe {
        body.push(if relative {
            Stmt::RelativeKeyframe {
                offset: time,
                body: statements,
                span: None,
            }
        } else {
            Stmt::Keyframe {
                time,
                body: statements,
                span: None,
            }
        });
    }
    body
}

/// Field/method/index access chain applied to `base_atom`.
fn access_parser<'src>(expr: ExprParser<'src>, base_atom: ExprParser<'src>) -> ExprParser<'src> {
    let ident = common::ident();

    #[derive(Clone)]
    enum PostfixStep {
        Field(String),
        Qualified(String),
        Call(Vec<CallArgument>, ByteSpan),
        Index(Expr),
    }
    let field_segment = ident.clone().map_with(
        |seg, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            (seg, extra.span())
        },
    );
    let postfix_step = choice((
        dot().ignore_then(field_segment.clone()).map(|(seg, span)| {
            crate::occurrence::record(OccurrenceKind::Property, seg.clone(), span);
            PostfixStep::Field(seg)
        }),
        colon_colon().ignore_then(field_segment.clone()).map(|(seg, span)| {
            crate::occurrence::record(OccurrenceKind::Variable, seg.clone(), span);
            PostfixStep::Qualified(seg)
        }),
        call_arguments(expr.clone())
            .delimited_by(lparen(), rparen())
            // A following arrow means these parentheses start a closure, not
            // call arguments. This lets a closure tail follow a let-binding
            // expression such as `let scale = x * 10\n(y) => y + scale`.
            .then_ignore(arrow().not())
            .map_with(|args, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                PostfixStep::Call(args, extra.span())
            }),
        lbracket()
            // Duration brackets following an assignment/property value are
            // animation modifiers (`opacity = 0.5 [250ms]`), not indexing.
            // A duration is never a valid collection index, so leave this
            // bracket for the enclosing statement's modifier parser.
            .ignore_then(expr.clone().filter(|index| !matches!(index, Expr::Duration(_))))
            .then_ignore(rbracket())
            .map(PostfixStep::Index),
    ));

    base_atom
        .foldl(postfix_step.repeated(), |base, step| match step {
            PostfixStep::Field(segment) => Expr::Field(Box::new(base), segment),
            PostfixStep::Qualified(segment) => match base {
                Expr::Ident(name) => Expr::QualifiedPath(vec![name, segment]),
                Expr::QualifiedPath(mut parts) => {
                    parts.push(segment);
                    Expr::QualifiedPath(parts)
                },
                other => Expr::Field(Box::new(other), segment),
            },
            PostfixStep::Call(args, span) => {
                if let Some(name) = callee_occurrence(&base) {
                    crate::occurrence::mark_call_callee(&name, span.start);
                }
                Expr::Call(Box::new(base), args)
            },
            PostfixStep::Index(index_expr) => Expr::Index(Box::new(base), Box::new(index_expr)),
        })
        .boxed()
}

fn call_arguments<'src>(
    expr: ExprParser<'src>,
) -> impl Parser<'src, StrInput<'src>, Vec<CallArgument>, ParserExtra<'src>> + Clone {
    let parameter_name = common::ident().or(select! { TokenKind::Keyword(name) => name }).map_with(
        |name, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
            crate::occurrence::record(OccurrenceKind::Parameter, name.clone(), extra.span());
            name
        },
    );
    let named = parameter_name
        .then_ignore(colon())
        .then(common::expr_with_span(expr.clone()))
        .map(|(name, (value, byte_span))| CallArgument {
            name: Some(name),
            value,
            byte_span: Some(byte_span),
        });
    let positional = common::expr_with_span(expr.clone()).map(|(value, byte_span)| CallArgument {
        name: None,
        value,
        byte_span: Some(byte_span),
    });
    choice((named, positional))
        .separated_by(comma())
        .allow_trailing()
        .collect::<Vec<_>>()
        .try_map(|args, span| {
            let mut named_seen = false;
            let mut names = std::collections::HashSet::new();
            for argument in &args {
                match &argument.name {
                    Some(name) => {
                        named_seen = true;
                        if !names.insert(name.clone()) {
                            return Err(Rich::custom(
                                span,
                                format!("duplicate named argument '{name}'"),
                            ));
                        }
                    },
                    None if named_seen => {
                        return Err(Rich::custom(
                            span,
                            "positional arguments must come before named arguments",
                        ));
                    },
                    None => {},
                }
            }
            Ok(args)
        })
}

fn callee_occurrence(callee: &Expr) -> Option<String> {
    match callee {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Field(_, name) => Some(name.clone()),
        Expr::Path(parts) | Expr::QualifiedPath(parts) => parts.last().cloned(),
        _ => None,
    }
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
