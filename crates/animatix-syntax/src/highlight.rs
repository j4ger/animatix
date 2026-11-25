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

/// Structural source headers recorded by the grammar, including inline comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Inclusive start and exclusive end in UTF-8 bytes.
    pub span: crate::ast::ByteSpan,
    /// Grammar context of the header.
    pub kind: HeaderKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How the source header participates in the editor.
pub enum HeaderKind {
    /// A document or scene time header.
    Keyframe,
    /// A time header scoped to a reusable timeline expression.
    Local,
    /// A named cue attached to a time header.
    Cue,
}

/// Merge the ordered token, occurrence and header streams without re-parsing.
/// Roles use byte offsets until a frontend converts to its coordinate system.
pub fn ranges(
    tokens: &[Token],
    occurrences: &[crate::occurrence::Occurrence],
    headers: &[Header],
) -> Vec<(usize, usize, &'static str)> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut occurrences = occurrences.iter().peekable();
    let mut headers = headers.iter().peekable();
    for token in tokens {
        let start = token.span.start;
        while occurrences.peek().is_some_and(|o| o.span.start < start) {
            occurrences.next();
        }
        while headers.peek().is_some_and(|h| h.span.end <= start) {
            headers.next();
        }
        let header =
            headers.peek().filter(|h| start >= h.span.start && token.span.end <= h.span.end);
        if let Some(header) = header
            && !matches!(token.kind, TokenKind::Comment(_))
        {
            let role = match header.kind {
                HeaderKind::Cue => "cue",
                HeaderKind::Local => "keyframe.local",
                HeaderKind::Keyframe => "keyframe",
            };
            if let Some((previous, end, previous_role)) = result.last_mut()
                && *previous >= header.span.start
                && (*previous_role == role
                    || (role == "keyframe" && *previous_role == "keyframe.part"))
            {
                *end = token.span.end;
                continue;
            }
            // Only the first segment opens a cell, even across a line comment.
            let role = if role == "keyframe" && start != header.span.start {
                "keyframe.part"
            } else {
                role
            };
            result.push((start, token.span.end, role));
        } else {
            let role = occurrences
                .peek()
                .filter(|o| {
                    o.span == token.span
                        && matches!(token.kind, TokenKind::Ident(_) | TokenKind::Keyword(_))
                })
                .map_or_else(|| lexical_role(token), |o| o.kind.token_role());
            result.push((start, token.span.end, role));
        }
    }
    result
}

#[cfg(test)]
mod tests;
