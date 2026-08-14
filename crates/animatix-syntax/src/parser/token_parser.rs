//! Token-level parser primitives.
//!
//! The full parser is being migrated from char-level to token-level. This
//! module defines the input type and leaf combinators over [`Token`] slices so
//! the rest of the parser can consume the single lossless tokenizer.

// Phase 2 foundation: these primitives are wired into the parser incrementally.
#![allow(dead_code)]

use chumsky::prelude::*;

use crate::token::{Token, TokenKind};

/// Token-slice input type.
pub type TokInput<'a> = &'a [Token];
/// Token-slice parser error type.
pub type TokErr<'a> = extra::Err<Rich<'a, Token>>;

/// Match a keyword by its lowercased text.
pub fn keyword<'a>(word: &'static str) -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! {
        Token { kind: TokenKind::Keyword(k), .. } if k.as_str() == word => (),
    }
}

/// Match an identifier and return its text.
pub fn ident<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! {
        Token { kind: TokenKind::Ident(s), .. } => s,
    }
}

/// Match a number literal.
pub fn number<'a>() -> impl Parser<'a, TokInput<'a>, f64, TokErr<'a>> + Clone {
    select! {
        Token { kind: TokenKind::Number(n), .. } => n,
    }
}

/// Match a string literal.
pub fn string<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! {
        Token { kind: TokenKind::Str(s), .. } => s,
    }
}

/// Match a boolean literal.
pub fn bool_lit<'a>() -> impl Parser<'a, TokInput<'a>, bool, TokErr<'a>> + Clone {
    select! {
        Token { kind: TokenKind::Bool(b), .. } => b,
    }
}

/// `{`
pub fn lbrace<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { Token { kind: TokenKind::LBrace, .. } => () }
}

/// `}`
pub fn rbrace<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { Token { kind: TokenKind::RBrace, .. } => () }
}

/// `:`
pub fn colon<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { Token { kind: TokenKind::Colon, .. } => () }
}

/// `,`
pub fn comma<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { Token { kind: TokenKind::Comma, .. } => () }
}

/// `=`
pub fn assign<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { Token { kind: TokenKind::Assign, .. } => () }
}

#[cfg(test)]
mod tests {
    use chumsky::Parser;

    use super::*;

    #[test]
    fn parses_a_config_header_from_tokens() {
        let tokens = crate::token::tokenize("config {");
        let input: TokInput<'_> = &tokens;

        let parser = keyword("config").then(lbrace());
        assert!(parser.parse(input).into_result().is_ok(), "expected `config {{` to parse");
    }

    #[test]
    fn parses_a_let_binding_from_tokens() {
        let tokens = crate::token::tokenize("let x = 42");
        let input: TokInput<'_> = &tokens;

        let parser = keyword("let").ignore_then(ident()).then_ignore(assign()).then(number());
        assert_eq!(parser.parse(input).unwrap(), ("x".to_string(), 42.0), "expected `let x = 42`");
    }
}
