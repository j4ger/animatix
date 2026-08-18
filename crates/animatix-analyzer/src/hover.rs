//! Hover information provider.

use animatix_syntax::token::{Token, TokenKind, byte_to_line_col, line_col_to_byte, token_at_byte};

use crate::symbol_table::{LabelKind, SymbolTable};
use crate::types::HoverInfo;

/// Get hover information at a cursor position.
pub fn hover_at(
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    line: usize,
    col: usize,
) -> Option<HoverInfo> {
    let byte = line_col_to_byte(source, line, col);
    let token = token_at_byte(tokens, byte)?;
    let text = token_text(source, token)?;
    let range = Some(token_range(source, token));

    match &token.kind {
        TokenKind::Str(_) | TokenKind::Typst(_) => {
            return Some(HoverInfo {
                contents: format!("**String** `{}`", text),
                range,
            });
        },
        TokenKind::Number(_) | TokenKind::Time { .. } | TokenKind::Percent(_) => {
            return Some(HoverInfo {
                contents: format!("**Number** `{}`", text),
                range,
            });
        },
        TokenKind::Comment(_) => {
            return Some(HoverInfo {
                contents: format!("*Comment*\n\n{}", text),
                range,
            });
        },
        TokenKind::Ident(_) | TokenKind::Keyword(_) => {},
        _ => return None,
    }

    // Labels, types, actions, and keywords take precedence over property names.
    if let Some(info) = symbols.labels.get(text) {
        let kind = match info.kind {
            LabelKind::Actor => "Actor",
            LabelKind::Let => "Variable",
            LabelKind::For => "Loop variable",
            LabelKind::Always => "Always block",
            LabelKind::Component => "Component",
        };
        let ty = info.ty.as_deref().unwrap_or("unknown");
        return Some(HoverInfo {
            contents: format!("**{}** `{}`\n\nType: {}", kind, text, ty),
            range,
        });
    }
    if symbols.types.contains(text) {
        let doc = animatix_syntax::builtins::type_documentation(text);
        let doc = if doc == "Unknown type." {
            animatix_syntax::schema::builtin_primitive_specs()
                .iter()
                .find(|spec| spec.type_name == text)
                .map(|spec| spec.display_name)
                .unwrap_or(doc)
        } else {
            doc
        };
        return Some(HoverInfo {
            contents: format!("**Type** `{}`\n\n{}", text, doc),
            range,
        });
    }
    if symbols.actions.contains(text) {
        return Some(HoverInfo {
            contents: format!(
                "**Action** `{}`\n\n{}",
                text,
                animatix_syntax::builtins::action_documentation(text)
            ),
            range,
        });
    }
    if symbols.keywords.contains(text) {
        return Some(HoverInfo {
            contents: format!(
                "**Keyword** `{}`\n\n{}",
                text,
                animatix_syntax::builtins::keyword_documentation(text)
            ),
            range,
        });
    }
    if symbols.components.contains_key(text) {
        let info = &symbols.components[text];
        let params_str = info
            .params
            .iter()
            .map(|p| match &p.param_type {
                Some(ty) => format!("{}: {:?}", p.name, ty),
                None => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Some(HoverInfo {
            contents: format!("**Component** `{}`\n\nParameters: ({})", text, params_str),
            range,
        });
    }

    // Property names are identifiers followed by `:` in a property list or
    // preceded by `.` in an access path.
    if is_property_position(tokens, token) {
        if let Some(doc) = crate::completer::property_documentation(text) {
            return Some(HoverInfo {
                contents: format!("**Property** `{}`\n\n{}", text, doc),
                range,
            });
        }
    }

    None
}

fn token_text<'a>(source: &'a str, token: &Token) -> Option<&'a str> {
    source.get(token.span.start..token.span.end)
}

fn token_range(source: &str, token: &Token) -> (usize, usize, usize, usize) {
    let (start_line, start_col) = byte_to_line_col(source, token.span.start);
    let (end_line, end_col) = byte_to_line_col(source, token.span.end);
    (start_line, start_col, end_line, end_col)
}

fn is_property_position(tokens: &[Token], token: &Token) -> bool {
    let idx = tokens.iter().position(|t| t.span == token.span);
    match idx {
        Some(i) => {
            let next_is_colon =
                tokens.get(i + 1).is_some_and(|t| matches!(t.kind, TokenKind::Colon));
            let prev_is_dot = i > 0 && matches!(tokens[i - 1].kind, TokenKind::Dot);
            next_is_colon || prev_is_dot
        },
        None => false,
    }
}
