//! Lossless tokenizer for `.amx` source.
//!
//! This is the single lexical definition for the language. The tokenizer
//! always succeeds (even on malformed input), so it can drive syntax
//! highlighting and cursor-position queries without a full semantic parse.
//! The parser consumes this token stream to build the semantic AST.
//!
//! Keyword and operator spelling lives here and nowhere else, so there is no
//! second grammar to keep in sync.

use crate::ast::ByteSpan;

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A structural keyword, lowercased.
    Keyword(String),
    /// An identifier (labels, variables, type names, etc.).
    Ident(String),
    /// A numeric literal.
    Number(f64),
    /// A time literal such as `2s` or `500ms`.
    Time {
        /// Numeric magnitude.
        value: f64,
        /// `true` for milliseconds, `false` for seconds.
        ms: bool,
    },
    /// A beat literal such as `2b` — a musical-time stamp whose millisecond
    /// length comes from the scene's `config { bpm: … }`.
    BeatTime {
        /// Number of beats.
        value: f64,
    },
    /// A percentage literal such as `50%`.
    Percent(f64),
    /// A string literal (without quotes).
    Str(String),
    /// A `//` line comment (without the leading `//`).
    Comment(String),
    /// A `$$ ... $$` Typst shorthand body (without the delimiters).
    Typst(String),
    /// A boolean literal.
    Bool(bool),
    /// The `null` literal.
    Null,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%` used as a modulo operator.
    PercentOp,
    /// `^`
    Caret,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `!`
    Not,
    /// `=`
    Assign,
    /// `:=`
    ReactiveAssign,
    /// `=>`
    Arrow,
    /// `->`
    ThinArrow,
    /// `..=`
    RangeInclusive,
    /// `|`
    Pipe,
    /// `&` (single) — property reference operator, e.g. `&actor.prop`
    Amp,
    /// `::`
    ColonColon,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `#`
    Hash,
    /// `@`
    At,
    /// `@slot`
    AtSlot,
    /// `_`
    Underscore,
}

/// A token with its byte range in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The lexical kind of this token.
    pub kind: TokenKind,
    /// Byte range of this token in the source.
    pub span: ByteSpan,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Keyword(s) | TokenKind::Ident(s) => write!(f, "{s}"),
            TokenKind::Number(n) => write!(f, "{n}"),
            TokenKind::Time { value, ms } => write!(f, "{value}{}", if *ms { "ms" } else { "s" }),
            TokenKind::BeatTime { value } => write!(f, "{value}b"),
            TokenKind::Percent(n) => write!(f, "{n}%"),
            TokenKind::Str(s) => write!(f, "\"{s}\""),
            TokenKind::Comment(_) => write!(f, "comment"),
            TokenKind::Typst(s) => write!(f, "$$ {s} $$"),
            TokenKind::Bool(b) => write!(f, "{b}"),
            TokenKind::Null => write!(f, "null"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::PercentOp => write!(f, "%"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Eq => write!(f, "=="),
            TokenKind::Neq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Le => write!(f, "<="),
            TokenKind::Ge => write!(f, ">="),
            TokenKind::And => write!(f, "&&"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::Not => write!(f, "!"),
            TokenKind::Assign => write!(f, "="),
            TokenKind::ReactiveAssign => write!(f, ":="),
            TokenKind::Arrow => write!(f, "=>"),
            TokenKind::ThinArrow => write!(f, "->"),
            TokenKind::RangeInclusive => write!(f, "..="),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Amp => write!(f, "&"),
            TokenKind::ColonColon => write!(f, "::"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Hash => write!(f, "#"),
            TokenKind::At => write!(f, "@"),
            TokenKind::AtSlot => write!(f, "@slot"),
            TokenKind::Underscore => write!(f, "_"),
        }
    }
}

impl chumsky::span::Span for ByteSpan {
    type Context = ();
    type Offset = usize;

    fn new(_context: Self::Context, range: std::ops::Range<Self::Offset>) -> Self {
        ByteSpan {
            start: range.start,
            end: range.end,
        }
    }

    fn context(&self) -> Self::Context {}

    fn start(&self) -> Self::Offset {
        self.start
    }

    fn end(&self) -> Self::Offset {
        self.end
    }
}

/// Tokenize `source` into a lossless token stream.
///
/// Whitespace is skipped. Tokens are emitted in source order; the slice's end
/// serves as end-of-input, so no explicit EOF token is produced. Malformed
/// input is recovered by emitting [`TokenKind::Ident`] for unrecognized runs
/// instead of failing.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).run().0
}

use lexer::Lexer;

mod issues;
mod lexer;
mod positions;
#[cfg(test)]
mod tests;

pub use issues::{TokenIssue, TokenIssueKind, tokenize_with_issues};
pub use positions::{LineIndex, byte_to_line_col, line_col_to_byte, token_at_byte};
