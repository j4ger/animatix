//! Shared lexical token-role classification for syntax highlighting.
//!
//! GUI highlighting and LSP semantic tokens both consume this module so the
//! keyword, literal, operator, and punctuation role vocabulary is defined once
//! instead of drifting between consumers. Identifier roles come from
//! parser-recorded [`crate::occurrence::Occurrence`] entries.

use crate::token::{Token, TokenKind};

/// Classify a token using the shared tokenizer role vocabulary.
pub fn lexical_role(token: &Token) -> &'static str {
    match &token.kind {
        TokenKind::Keyword(_) => "keyword",
        TokenKind::Number(_)
        | TokenKind::Time { .. }
        | TokenKind::BeatTime { .. }
        | TokenKind::Percent(_) => "number",
        TokenKind::Bool(_) => "boolean",
        TokenKind::Str(_) | TokenKind::Typst(_) => "string",
        TokenKind::Comment(_) => "comment",
        TokenKind::Null => "keyword",
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::PercentOp
        | TokenKind::Caret
        | TokenKind::Eq
        | TokenKind::Neq
        | TokenKind::Lt
        | TokenKind::Gt
        | TokenKind::Le
        | TokenKind::Ge
        | TokenKind::And
        | TokenKind::Or
        | TokenKind::Not
        | TokenKind::Assign
        | TokenKind::ReactiveAssign
        | TokenKind::Arrow
        | TokenKind::ThinArrow
        | TokenKind::RangeInclusive
        | TokenKind::Pipe
        | TokenKind::Amp
        | TokenKind::ColonColon => "operator",
        TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::Colon
        | TokenKind::Comma
        | TokenKind::Dot
        | TokenKind::Hash
        | TokenKind::At
        | TokenKind::AtSlot => "punctuation",
        TokenKind::Underscore => "wildcard",
        TokenKind::Ident(_) => "variable",
    }
}
