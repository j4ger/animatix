//! Shared parser types and factory functions (token-level).
//!
//! The parser consumes the lossless token stream produced by [`crate::token`],
//! so all leaf combinators here match [`TokenKind`]s rather than characters.
//! Whitespace and comments are already handled by the tokenizer.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::token_parser::{self, TokErr, TokInput};
use crate::ast::*;
use crate::easing::parse_easing_name;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub(crate) type ParserExtra<'src> = TokErr<'src>;
pub(crate) type StrInput<'src> = TokInput<'src>;
pub(crate) type ExprParser<'src> = Boxed<'src, 'src, StrInput<'src>, Expr, ParserExtra<'src>>;
pub(crate) type TimeParser<'src> = Boxed<'src, 'src, StrInput<'src>, Time, ParserExtra<'src>>;
pub(crate) type IdentParser<'src> = Boxed<'src, 'src, StrInput<'src>, String, ParserExtra<'src>>;
pub(crate) type PropertyParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Property, ParserExtra<'src>>;
pub(crate) type ModifierParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Modifier, ParserExtra<'src>>;
pub(crate) type ModifiersParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Vec<Modifier>, ParserExtra<'src>>;
pub(crate) type InlineItemsParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Vec<InlineItem>, ParserExtra<'src>>;
pub(crate) type StmtParser<'src> = Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>>;

// ---------------------------------------------------------------------------
// Identifier parser
// ---------------------------------------------------------------------------

/// Parse an identifier. Reserved keywords are already classified as
/// [`crate::token::TokenKind::Keyword`] by the tokenizer, so matching an
/// `Ident` token is sufficient.
pub(crate) fn ident<'src>() -> IdentParser<'src> {
    token_parser::ident().boxed()
}

// ---------------------------------------------------------------------------
// Dotted identifier parser
// ---------------------------------------------------------------------------

/// Parse a dotted path (e.g. `scene.background`, `container.child.prop`).
pub(crate) fn dotted_ident<'src>()
-> impl Parser<'src, StrInput<'src>, Vec<String>, ParserExtra<'src>> + Clone {
    ident()
        .separated_by(token_parser::punct(crate::token::TokenKind::Dot))
        .at_least(1)
        .collect()
}

// ---------------------------------------------------------------------------
// Indexed dotted identifier parser (for targets/assignments)
// ---------------------------------------------------------------------------

/// Parse a dotted path where each segment may carry an integer array index or
/// a runtime index expression.
pub(crate) fn indexed_dotted_ident<'src>()
-> impl Parser<'src, StrInput<'src>, Vec<TargetSegment>, ParserExtra<'src>> + Clone {
    let segment = ident()
        .then(
            token_parser::punct(crate::token::TokenKind::LBracket)
                .ignore_then(token_parser::number().map(|n| n as usize))
                .then_ignore(token_parser::punct(crate::token::TokenKind::RBracket))
                .or_not(),
        )
        .map(|(name, idx)| match idx {
            Some(n) => TargetSegment::Static(format!("{name}__{n}")),
            None => TargetSegment::Static(name),
        });

    segment
        .separated_by(token_parser::punct(crate::token::TokenKind::Dot))
        .at_least(1)
        .collect()
}

/// Version of [`indexed_dotted_ident`] that accepts an expression parser for
/// runtime-indexed targets.
pub(crate) fn indexed_dotted_ident_with_expr<'src>(
    expr: ExprParser<'src>,
) -> impl Parser<'src, StrInput<'src>, Vec<TargetSegment>, ParserExtra<'src>> + Clone {
    let segment = ident()
        .then(
            token_parser::punct(crate::token::TokenKind::LBracket)
                .ignore_then(expr)
                .then_ignore(token_parser::punct(crate::token::TokenKind::RBracket))
                .or_not(),
        )
        .map(|(name, idx)| match idx {
            Some(Expr::Num(n)) if n.trunc() == n && n >= 0.0 => {
                TargetSegment::Static(format!("{}__{}", name, n as usize))
            },
            Some(e) => TargetSegment::Indexed {
                base: name,
                index: Box::new(e),
            },
            None => TargetSegment::Static(name),
        });

    segment
        .separated_by(token_parser::punct(crate::token::TokenKind::Dot))
        .at_least(1)
        .collect()
}

// ---------------------------------------------------------------------------
// Type identifier parser
// ---------------------------------------------------------------------------

/// Parse a type identifier: an identifier starting with an uppercase letter.
pub(crate) fn type_ident<'src>() -> IdentParser<'src> {
    ident()
        .filter(|s: &String| s.chars().next().is_some_and(|c| c.is_uppercase()))
        .boxed()
}

// ---------------------------------------------------------------------------
// Label expression parser
// ---------------------------------------------------------------------------

/// Parse a label expression: `name` or `name[index_expr]`.
pub(crate) fn label_expr<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> Boxed<'src, 'src, StrInput<'src>, (String, Option<Expr>), ParserExtra<'src>> {
    ident()
        .then(
            expr.clone()
                .delimited_by(
                    token_parser::punct(crate::token::TokenKind::LBracket),
                    token_parser::punct(crate::token::TokenKind::RBracket),
                )
                .or_not(),
        )
        .boxed()
}

// ---------------------------------------------------------------------------
// String literal parser
// ---------------------------------------------------------------------------

/// Parse a quoted string literal, returning `Expr::Str`.
pub(crate) fn string_literal<'src>()
-> impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone {
    token_parser::string().map(Expr::Str)
}

// ---------------------------------------------------------------------------
// Time literal parser
// ---------------------------------------------------------------------------

/// Parse a time literal: `2s`, `500ms`, or `1.5s`.
pub(crate) fn time<'src>() -> TimeParser<'src> {
    token_parser::time().boxed()
}

// ---------------------------------------------------------------------------
// Expression with span helper
// ---------------------------------------------------------------------------

/// Wrap an expression parser to capture the byte span of the parsed value.
pub(crate) fn expr_with_span<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> Boxed<'src, 'src, StrInput<'src>, (Expr, ByteSpan), ParserExtra<'src>> {
    expr.map_with(|value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
        (value, extra.span())
    })
    .boxed()
}

// ---------------------------------------------------------------------------
// Property parser
// ---------------------------------------------------------------------------

/// Parse a property assignment: `name: value`.
pub(crate) fn property<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> PropertyParser<'src> {
    let property_name = dotted_ident().map(|parts: Vec<String>| parts.join(".")).or(ident());

    property_name
        .then_ignore(token_parser::colon())
        .then(expr_with_span(expr))
        .map(|(name, (value, value_span))| Property {
            name,
            value,
            value_span: Some(value_span),
            trailing_comment: None,
        })
        .labelled("property")
        .boxed()
}

// ---------------------------------------------------------------------------
// Modifier parser
// ---------------------------------------------------------------------------

/// Parse a single modifier: `name: value`, `2s`, or any expression.
pub(crate) fn modifier<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
    time: impl Parser<'src, StrInput<'src>, Time, ParserExtra<'src>> + Clone + 'src,
) -> ModifierParser<'src> {
    choice((
        ident()
            .then_ignore(token_parser::colon())
            .then(choice((
                time.clone().map(|t| match t {
                    Time::Seconds(s) => Expr::Ident(format!("{s}s")),
                    Time::Milliseconds(ms) => Expr::Ident(format!("{ms}ms")),
                }),
                expr.clone(),
            )))
            .map(|(name, value)| Modifier {
                name: Some(name),
                value,
            }),
        time.clone().map(|t| Modifier {
            name: None,
            value: match t {
                Time::Seconds(s) => Expr::Ident(format!("{s}s")),
                Time::Milliseconds(ms) => Expr::Ident(format!("{ms}ms")),
            },
        }),
        expr.clone().map(|value| Modifier { name: None, value }),
    ))
    .boxed()
}

// ---------------------------------------------------------------------------
// Modifiers list parser
// ---------------------------------------------------------------------------

/// Parse a bracketed modifier list: `[2s, ease: bounce]`.
pub(crate) fn modifiers<'src>(
    modifier: impl Parser<'src, StrInput<'src>, Modifier, ParserExtra<'src>> + Clone + 'src,
) -> ModifiersParser<'src> {
    modifier
        .separated_by(token_parser::comma())
        .collect::<Vec<_>>()
        .delimited_by(
            token_parser::punct(crate::token::TokenKind::LBracket),
            token_parser::punct(crate::token::TokenKind::RBracket),
        )
        .or_not()
        .map(|m: Option<Vec<Modifier>>| m.unwrap_or_default())
        .labelled("modifier list")
        .as_context()
        .boxed()
}

// ---------------------------------------------------------------------------
// Easing extraction
// ---------------------------------------------------------------------------

/// Scan modifiers for `ease: ...` and extract the easing value.
pub(crate) fn extract_easing(modifiers: &mut Vec<Modifier>) -> Option<crate::easing::Easing> {
    let mut easing = None;
    modifiers.retain(|m| {
        if m.name.as_deref() == Some("ease") {
            if let Expr::Ident(raw) = &m.value {
                easing = parse_easing_name(raw);
            }
            false
        } else {
            true
        }
    });
    easing
}
