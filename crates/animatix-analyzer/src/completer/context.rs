use super::*;

impl CompletionContext {
    pub(super) fn from_tokens(
        tokens: &[Token],
        byte: usize,
        symbols: &SymbolTable,
        source: &str,
    ) -> Self {
        // Find the last non-trivia token that ends at or before the cursor.
        let prev = tokens
            .iter()
            .filter(|t| t.span.end <= byte)
            .rfind(|t| !matches!(t.kind, TokenKind::Comment(_)));

        let Some(prev) = prev else {
            return CompletionContext::TopLevel;
        };

        match &prev.kind {
            TokenKind::Colon => {
                let before_colon = tokens
                    .iter()
                    .take_while(|t| t.span != prev.span)
                    .filter(|t| !matches!(t.kind, TokenKind::Comment(_)))
                    .last();
                if before_colon.is_some_and(|t| {
                    matches!(&t.kind, TokenKind::Ident(name) if symbols.labels.contains_key(name))
                }) {
                    CompletionContext::TypePosition
                } else {
                    let property_name = before_colon.and_then(|t| match &t.kind {
                        TokenKind::Ident(name) => Some(name.clone()),
                        _ => None,
                    });
                    CompletionContext::PropertyValue {
                        property_name,
                        actor_type: None,
                    }
                }
            },
            TokenKind::Assign | TokenKind::ReactiveAssign => CompletionContext::PropertyValue {
                property_name: None,
                actor_type: None,
            },
            TokenKind::LBracket => CompletionContext::ModifierList,
            TokenKind::Ident(name) if symbols.actions.contains(name) => {
                CompletionContext::ActionTarget
            },
            TokenKind::Ident(_) if in_verb_position(tokens, prev, source) => {
                CompletionContext::VerbPosition
            },
            TokenKind::LBrace => CompletionContext::PropertyBlock {
                actor_type: find_actor_type_before_brace(tokens, prev, symbols),
            },
            _ => CompletionContext::Unknown,
        }
    }
}

/// True when `token` sits where a statement's action verb would start: it is
/// the first token on its line, or the first token after `#0s`-style keyframe
/// time, `{`, or `;`. This is what lets a partially typed or typo'd verb
/// (`fa`, `fly`) still surface the action-verb list.
pub(super) fn in_verb_position(tokens: &[Token], token: &Token, source: &str) -> bool {
    use animatix_syntax::token::byte_to_line_col;

    let Some(index) = tokens.iter().position(|t| t.span == token.span) else {
        return false;
    };
    let before = tokens[..index].iter().rev().find(|t| !matches!(t.kind, TokenKind::Comment(_)));
    let Some(before) = before else {
        return true; // first statement in the file
    };
    let before_line = byte_to_line_col(source, before.span.start).0;
    let token_line = byte_to_line_col(source, token.span.start).0;
    before_line < token_line || matches!(before.kind, TokenKind::Time { .. } | TokenKind::LBrace)
}

pub(super) fn find_actor_type_before_brace(
    tokens: &[Token],
    brace: &Token,
    symbols: &SymbolTable,
) -> Option<String> {
    let idx = tokens.iter().position(|t| t.span == brace.span)?;
    for t in tokens[..idx].iter().rev() {
        if let TokenKind::Ident(name) = &t.kind
            && symbols.types.contains(name)
        {
            return Some(name.clone());
        }
    }
    None
}
