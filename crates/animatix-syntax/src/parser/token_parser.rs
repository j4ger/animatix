//! Token-level parser primitives.
//!
//! The full parser consumes the single lossless tokenizer. The input is mapped
//! so each token is a [`TokenKind`] and each span is the original byte
//! [`ByteSpan`], preserving the char-level parser's span semantics.

use chumsky::prelude::*;

use crate::ast::ByteSpan;
use crate::token::{Token, TokenKind};

/// A spanned token: kind plus byte range.
pub type SpannedToken = (TokenKind, ByteSpan);
/// Token-slice input mapped to `TokenKind` tokens and byte spans.
pub type TokInput<'a> = chumsky::input::MappedInput<'a, TokenKind, ByteSpan, &'a [SpannedToken]>;
/// Token-slice parser error type.
pub type TokErr<'a> = extra::Err<Rich<'a, TokenKind, ByteSpan>>;

/// Convert a token stream into spanned tokens for the parser.
///
/// Line comments are filtered out here: the tokenizer keeps them for
/// lossless highlighting, but the parser ignores them. Block comments are not
/// lexed as comments (`/` and `*` arrive as operators) so the parser can still
/// reject them explicitly.
pub fn spanned(tokens: &[Token]) -> Vec<SpannedToken> {
    tokens
        .iter()
        .filter(|t| !matches!(t.kind, TokenKind::Comment(_)))
        .map(|t| (t.kind.clone(), t.span))
        .collect()
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

/// A Typst shorthand body (`$$ ... $$`).
pub fn typst<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! { TokenKind::Typst(s) => s }
}

/// Match a number literal.
pub fn number<'a>() -> impl Parser<'a, TokInput<'a>, f64, TokErr<'a>> + Clone {
    select! { TokenKind::Number(n) => n }
}

/// Match an identifier and return its text plus byte span.
#[cfg(test)]
pub fn ident_span<'a>() -> impl Parser<'a, TokInput<'a>, (String, ByteSpan), TokErr<'a>> + Clone {
    use chumsky::input::MapExtra;
    ident().map_with(|name, extra: &mut MapExtra<'a, '_, TokInput<'a>, TokErr<'a>>| {
        (name, extra.span())
    })
}

/// Match a string literal.
pub fn string<'a>() -> impl Parser<'a, TokInput<'a>, String, TokErr<'a>> + Clone {
    select! { TokenKind::Str(s) => s }
}

/// Match a boolean literal.
pub fn bool_lit<'a>() -> impl Parser<'a, TokInput<'a>, bool, TokErr<'a>> + Clone {
    select! { TokenKind::Bool(b) => b }
}

/// A time literal or speech-cue timestamp, returned as an AST `Time`.
pub fn time<'a>() -> impl Parser<'a, TokInput<'a>, crate::ast::Time, TokErr<'a>> + Clone {
    let standard_time = select! {
        TokenKind::Time { value, ms } if ms => crate::ast::Time::Milliseconds(value as u64),
        TokenKind::Time { value, ms: false } => crate::ast::Time::Seconds(value),
        TokenKind::BeatTime { value } => crate::ast::Time::Beats(value),
    };

    let offset_time = choice((
        plus().ignore_then(standard_time).map(|t| match t {
            crate::ast::Time::Milliseconds(ms) => ms as i64,
            crate::ast::Time::Seconds(s) => (s * 1000.0) as i64,
            _ => 0,
        }),
        minus().ignore_then(standard_time).map(|t| match t {
            crate::ast::Time::Milliseconds(ms) => -(ms as i64),
            crate::ast::Time::Seconds(s) => -(s * 1000.0) as i64,
            _ => 0,
        }),
    ))
    .or_not();

    let marker_target = at().ignore_then(ident()).map(|m| (m, None));

    let word_target = lbracket()
        .ignore_then(string())
        .then(comma().ignore_then(number()).or_not())
        .then_ignore(rbracket())
        .map(|(w, occ)| (w, Some(occ.map(|n| n as usize).unwrap_or(1))));

    let speech_time = ident().then(choice((marker_target, word_target))).then(offset_time).map(
        |((track, target), offset)| {
            let offset_ms = offset.unwrap_or(0);
            match target {
                (marker, None) => crate::ast::Time::SpeechMarker {
                    track,
                    marker,
                    offset_ms,
                },
                (word, Some(occurrence)) => crate::ast::Time::SpeechWord {
                    track,
                    word,
                    occurrence,
                    offset_ms,
                },
            }
        },
    );

    choice((standard_time, speech_time))
}

/// A percentage literal.
pub fn percent<'a>() -> impl Parser<'a, TokInput<'a>, f64, TokErr<'a>> + Clone {
    select! { TokenKind::Percent(n) => n }
}

/// The `null` literal.
pub fn null<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { TokenKind::Null => () }
}

/// Match an arbitrary unit-variant punctuation or operator token.
pub fn punct<'a>(kind: TokenKind) -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
    select! { tok if tok == kind => () }
}

macro_rules! unit_parsers {
    ($(($name:ident, $variant:ident)),* $(,)?) => {
        $(
            pub fn $name<'a>() -> impl Parser<'a, TokInput<'a>, (), TokErr<'a>> + Clone {
                select! { TokenKind::$variant => () }
            }
        )*
    };
}

unit_parsers!(
    (lparen, LParen),
    (rparen, RParen),
    (lbracket, LBracket),
    (rbracket, RBracket),
    (lbrace, LBrace),
    (rbrace, RBrace),
    (colon, Colon),
    (comma, Comma),
    (dot, Dot),
    (plus, Plus),
    (minus, Minus),
    (star, Star),
    (slash, Slash),
    (percent_op, PercentOp),
    (caret, Caret),
    (eq, Eq),
    (neq, Neq),
    (lt, Lt),
    (gt, Gt),
    (le, Le),
    (ge, Ge),
    (and, And),
    (or, Or),
    (not, Not),
    (assign, Assign),
    (reactive_assign, ReactiveAssign),
    (arrow, Arrow),
    (thin_arrow, ThinArrow),
    (range_inclusive, RangeInclusive),
    (pipe, Pipe),
    (amp, Amp),
    (colon_colon, ColonColon),
    (hash, Hash),
    (at, At),
    (at_slot, AtSlot),
    (underscore, Underscore),
);

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
        let tokens = crate::token::tokenize("let x");
        let spanned = spanned(&tokens);
        let input = as_input(&spanned);

        let parser = keyword("let").ignore_then(ident_span());
        let (name, span) = parser.parse(input).into_result().unwrap();
        assert_eq!(name, "x");
        assert_eq!(span.start, 4);
        assert_eq!(span.end, 5);
    }

    #[test]
    fn parses_literals_and_punct() {
        let tokens = crate::token::tokenize("2s 50% null +");
        let spanned = spanned(&tokens);
        let input = as_input(&spanned);

        let parser = time().then(percent()).then(null()).then(plus());
        let (((t, p), _), _) = parser.parse(input).into_result().unwrap();
        assert_eq!(t, crate::ast::Time::Seconds(2.0));
        assert_eq!(p, 50.0);
    }

    #[test]
    fn parses_speech_cue_times() {
        let tokens = crate::token::tokenize("voice@intro");
        let spanned_toks = spanned(&tokens);
        let input = as_input(&spanned_toks);
        let t = time().parse(input).into_result().unwrap();
        assert_eq!(
            t,
            crate::ast::Time::SpeechMarker {
                track: "voice".to_string(),
                marker: "intro".to_string(),
                offset_ms: 0,
            }
        );

        let tokens = crate::token::tokenize("voice@step1 + 250ms");
        let spanned_toks = spanned(&tokens);
        let input = as_input(&spanned_toks);
        let t = time().parse(input).into_result().unwrap();
        assert_eq!(
            t,
            crate::ast::Time::SpeechMarker {
                track: "voice".to_string(),
                marker: "step1".to_string(),
                offset_ms: 250,
            }
        );

        let tokens = crate::token::tokenize("voice[\"attention\"]");
        let spanned_toks = spanned(&tokens);
        let input = as_input(&spanned_toks);
        let t = time().parse(input).into_result().unwrap();
        assert_eq!(
            t,
            crate::ast::Time::SpeechWord {
                track: "voice".to_string(),
                word: "attention".to_string(),
                occurrence: 1,
                offset_ms: 0,
            }
        );

        let tokens = crate::token::tokenize("voice[\"network\", 2] - 100ms");
        let spanned_toks = spanned(&tokens);
        let input = as_input(&spanned_toks);
        let t = time().parse(input).into_result().unwrap();
        assert_eq!(
            t,
            crate::ast::Time::SpeechWord {
                track: "voice".to_string(),
                word: "network".to_string(),
                occurrence: 2,
                offset_ms: -100,
            }
        );
    }
}
