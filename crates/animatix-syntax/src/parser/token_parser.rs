//! Token-level parser primitives.
//!
//! The full parser is being migrated from char-level to token-level. This
//! module defines the input type and leaf combinators over [`Token`] slices so
//! the rest of the parser can consume the single lossless tokenizer. The input
//! is mapped so each token is a [`TokenKind`] and each span is the original
//! byte [`ByteSpan`], preserving the char-level parser's span semantics.

// Phase 2 foundation: these primitives are wired into the parser incrementally.
#![allow(dead_code)]

use chumsky::prelude::*;

use crate::ast::ByteSpan;
use crate::token::{Token, TokenKind};

/// A spanned token: kind plus byte range.
pub type SpannedToken = (TokenKind, ByteSpan);
/// Token-slice input mapped to `TokenKind` tokens and byte spans.
pub type TokInput<'a> = chumsky::input::MappedInput<'a, TokenKind, ByteSpan, &'a [SpannedToken]>;
/// Token-slice parser error type.
pub type TokErr<'a> = extra::Err<Rich<'a, TokenKind, ByteSpan>>;

/// Convert a token stream into spanned tokens.
pub fn spanned(tokens: &[Token]) -> Vec<SpannedToken> {
    tokens.iter().map(|t| (t.kind.clone(), t.span)).collect()
}

/// Build a token-slice parser input from spanned tokens.
pub fn as_input(spanned: &[SpannedToken]) -> TokInput<'_> {
    use chumsky::input::Input as _;
    let eoi = spanned.last().map(|(_, s)| s.end).unwrap_or(0);
    spanned.split_token_span(ByteSpan {
        start: eoi,
        end: eoi,
    })
}

/// Match a keyword by its lowercased text.
pub fn keyword<'a>(word: &'static str) -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::Keyword(k) if k.as_str() == word => () }
}

/// Match an identifier and return its text.
pub fn ident<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! { TokenKind::Ident(s) => s }
}

/// Match an identifier and return its text plus byte span.
pub fn ident_span<'a>() -> impl Parser<'a, TokInput<'a>, (String, ByteSpan), TokErr<'a>> + Clone {
    use chumsky::input::MapExtra;
    ident().map_with(|name, extra: &mut MapExtra<'a, '_, TokInput<'a>, TokErr<'a>>| {
        (name, extra.span())
    })
}

/// Match a number literal.
pub fn number<'a>() -> impl Parser<'a, TokInput<'a>, f64, TokErr<'a>> + Clone {
    select! { TokenKind::Number(n) => n }
}

/// Match a string literal.
pub fn string<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! { TokenKind::Str(s) => s }
}

/// Match a boolean literal.
pub fn bool_lit<'a>() -> impl Parser<'a, TokInput<'a>, bool, TokErr<'a>> + Clone {
    select! { TokenKind::Bool(b) => b }
}

/// `{`
pub fn lbrace<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::LBrace => () }
}

/// `}`
pub fn rbrace<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::RBrace => () }
}

/// `:`
pub fn colon<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::Colon => () }
}

/// `,`
pub fn comma<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::Comma => () }
}

/// `=`
pub fn assign<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::Assign => () }
}

#[cfg(test)]
mod tests {
    use chumsky::Parser;

    use super::*;

    #[test]
    fn parses_a_config_header_from_tokens() {
        let tokens = crate::token::tokenize("config {");
        let spanned = spanned(&tokens);
        let input = as_input(&spanned);

        let parser = keyword("config").then(lbrace());
        assert!(parser.parse(input).into_result().is_ok(), "expected `config {{` to parse");
    }

    #[test]
    fn parses_a_let_binding_from_tokens() {
        let tokens = crate::token::tokenize("let x = 42");
        let spanned = spanned(&tokens);
        let input = as_input(&spanned);

        let parser = keyword("let").ignore_then(ident()).then_ignore(assign()).then(number());
        assert_eq!(
            parser.parse(input).into_result().unwrap(),
            ("x".to_string(), 42.0),
            "expected `let x = 42`"
        );
    }

    #[test]
    fn captures_byte_spans_from_tokens() {
        // `let x`: `x` occupies bytes 4..5.
        let tokens = crate::token::tokenize("let x");
        let spanned = spanned(&tokens);
        let input = as_input(&spanned);

        let parser = keyword("let").ignore_then(ident_span());
        let (name, span) = parser.parse(input).into_result().unwrap();
        assert_eq!(name, "x");
        assert_eq!(span.start, 4);
        assert_eq!(span.end, 5);
    }
}
