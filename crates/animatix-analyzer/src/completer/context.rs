use super::*;

impl CompletionContext {
    pub(super) fn from_tokens(
        tokens: &[Token],
        byte: usize,
        symbols: &SymbolTable,
        source: &str,
    ) -> Self {
        let previous = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| {
                token.span.end <= byte && !matches!(token.kind, TokenKind::Comment(_))
            })
            .map(|(index, _)| index)
            .next_back();
        let Some(previous) = previous else {
            return CompletionContext::TopLevel;
        };

        if let Some(call) = call_context(tokens, previous, byte, source) {
            return call;
        }

        let prev = &tokens[previous];
        match &prev.kind {
            TokenKind::ColonColon => namespace_prefix(tokens, previous)
                .map(|prefix| CompletionContext::NamespacePosition { prefix })
                .unwrap_or(CompletionContext::Unknown),
            TokenKind::Dot => tokens
                .get(previous.wrapping_sub(1))
                .and_then(|token| match &token.kind {
                    TokenKind::Ident(receiver) => Some(receiver.clone()),
                    _ => None,
                })
                .map(|receiver| CompletionContext::MethodPosition { receiver })
                .unwrap_or(CompletionContext::Unknown),
            TokenKind::Colon => {
                let before_colon = tokens[..previous]
                    .iter()
                    .rev()
                    .find(|token| !matches!(token.kind, TokenKind::Comment(_)));
                if before_colon.is_some_and(|token| {
                    matches!(&token.kind, TokenKind::Ident(name) if symbols.labels.contains_key(name))
                }) {
                    CompletionContext::TypePosition
                } else {
                    let property_name = before_colon.and_then(|token| match &token.kind {
                        TokenKind::Ident(name) => Some(name.clone()),
                        _ => None,
                    });
                    let actor_type = find_inline_actor_type(tokens, previous, symbols);
                    CompletionContext::PropertyValue { property_name, actor_type }
                }
            },
            TokenKind::Assign | TokenKind::ReactiveAssign => CompletionContext::PropertyValue {
                property_name: None,
                actor_type: None,
            },
            TokenKind::LBracket => CompletionContext::ModifierList,
            TokenKind::Ident(_) if in_verb_position(tokens, prev, source) => {
                CompletionContext::CallablePosition
            },
            TokenKind::LBrace => CompletionContext::PropertyBlock {
                actor_type: find_actor_type_before_brace(tokens, prev, symbols),
            },
            _ => CompletionContext::Unknown,
        }
    }
}

// At an inline property's colon, search back to the declaration's type.
fn find_inline_actor_type(tokens: &[Token], colon: usize, symbols: &SymbolTable) -> Option<String> {
    for (index, token) in tokens[..colon].iter().enumerate().rev() {
        match &token.kind {
            TokenKind::Ident(name)
                if index >= 2
                    && matches!(tokens[index - 1].kind, TokenKind::Colon)
                    && animatix_syntax::typing::known_properties().contains_key(name) =>
            {
                return Some(name.clone());
            },
            TokenKind::RBrace if index + 1 == colon => return None,
            TokenKind::LBrace => return find_actor_type_before_brace(tokens, token, symbols),
            _ => {},
        }
    }
    None
}

fn namespace_prefix(tokens: &[Token], separator: usize) -> Option<String> {
    let mut segments = Vec::new();
    let mut cursor = separator.checked_sub(1)?;
    let TokenKind::Ident(name) = &tokens[cursor].kind else {
        return None;
    };
    segments.push(name.clone());
    while cursor >= 2 && matches!(&tokens[cursor - 1].kind, TokenKind::ColonColon) {
        cursor -= 2;
        let TokenKind::Ident(name) = &tokens[cursor].kind else {
            break;
        };
        segments.push(name.clone());
    }
    segments.reverse();
    Some(segments.join("::"))
}

fn call_context(
    tokens: &[Token],
    previous: usize,
    byte: usize,
    source: &str,
) -> Option<CompletionContext> {
    let mut nested = 0usize;
    let mut open = None;
    for index in (0..=previous).rev() {
        match &tokens[index].kind {
            TokenKind::RParen => nested += 1,
            TokenKind::LParen if nested == 0 => {
                open = Some(index);
                break;
            },
            TokenKind::LParen => nested -= 1,
            _ => {},
        }
    }
    let open = open?;
    let name_index = open.checked_sub(1)?;
    let TokenKind::Ident(_) = &tokens[name_index].kind else {
        return None;
    };
    let mut callee_start = name_index;
    while callee_start >= 2
        && matches!(&tokens[callee_start - 1].kind, TokenKind::ColonColon)
        && matches!(&tokens[callee_start - 2].kind, TokenKind::Ident(_))
    {
        callee_start -= 2;
    }
    let qualified = tokens[callee_start..=name_index]
        .iter()
        .map(|token| match &token.kind {
            TokenKind::Ident(name) => name.as_str(),
            TokenKind::ColonColon => "::",
            _ => "",
        })
        .collect::<String>();
    let callee = if callee_start >= 2
        && matches!(&tokens[callee_start - 1].kind, TokenKind::Dot)
        && let TokenKind::Ident(receiver) = &tokens[callee_start - 2].kind
    {
        Some(format!("{receiver}.{qualified}"))
    } else {
        Some(qualified)
    };

    let args_start = tokens[open].span.end;
    let text = source.get(args_start..byte).unwrap_or_default();
    let segments = call_segments(text);
    let argument_index = segments.len().saturating_sub(1);
    let current = segments.last().copied().unwrap_or_default();
    let current_name = top_level_named_argument(current).map(str::to_owned);
    let supplied_names = segments
        .iter()
        .take(segments.len().saturating_sub(1))
        .filter_map(|segment| top_level_named_argument(segment).map(str::to_owned))
        .collect();
    Some(CompletionContext::CallArgument {
        callee,
        argument_index,
        current_name,
        supplied_names,
    })
}

fn call_segments(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut parens = 0i32;
    let mut brackets = 0i32;
    let mut braces = 0i32;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        match ch {
            '(' => parens += 1,
            ')' => parens -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            '{' => braces += 1,
            '}' => braces -= 1,
            ',' if parens == 0 && brackets == 0 && braces == 0 => {
                parts.push(text[start..index].trim());
                start = index + ch.len_utf8();
            },
            _ => {},
        }
    }
    parts.push(text[start..].trim());
    parts
}

fn top_level_named_argument(segment: &str) -> Option<&str> {
    let mut depth = 0i32;
    for (index, ch) in segment.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => {
                let name = segment[..index].trim();
                return (!name.is_empty()).then_some(name);
            },
            _ => {},
        }
    }
    None
}

/// True when this identifier starts an expression statement, where callable
/// completion is useful for replacing a misspelled function name.
pub(super) fn in_verb_position(tokens: &[Token], token: &Token, source: &str) -> bool {
    use animatix_syntax::token::byte_to_line_col;

    let Some(index) = tokens.iter().position(|candidate| candidate.span == token.span) else {
        return false;
    };
    let before = tokens[..index].iter().rev().find(|t| !matches!(t.kind, TokenKind::Comment(_)));
    let Some(before) = before else {
        return true;
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
