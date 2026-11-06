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

mod impls;
mod issues;
mod lexer;
mod positions;
#[cfg(test)]
mod tests;

pub use issues::{TokenIssue, TokenIssueKind, tokenize_with_issues};
pub use lexer::tokenize;
pub use positions::{
    LineIndex, byte_to_line_col, line_col_to_byte, line_col_to_byte_utf16, token_at_byte,
};
