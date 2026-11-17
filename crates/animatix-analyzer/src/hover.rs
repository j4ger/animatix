//! Hover information provider.

use animatix_syntax::token::{LineIndex, Token, TokenKind, token_at_byte};

use crate::extension_manifest::ExtensionManifest;
use crate::symbol_table::{LabelKind, SymbolTable};
use crate::types::HoverInfo;

/// Get hover information at a cursor position.
pub fn hover_at(
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    line: usize,
    col: usize,
    manifest: &ExtensionManifest,
) -> Option<HoverInfo> {
    // LSP positions count UTF-16 code units, not Unicode scalar values.
    let index = LineIndex::new(source);
    let byte = index.line_col_to_byte_utf16(line, col);
    let token = token_at_byte(tokens, byte)?;
    let text = token_text(source, token)?;
    let range = Some(token_range(&index, token));

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

    // Labels, types, callables, and keywords take precedence over property names.
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
        if let Some(primitive) =
            manifest.primitives.iter().find(|primitive| primitive.type_name == text)
        {
            return Some(HoverInfo {
                contents: format!(
                    "**Type** `{}`\n\n{} ({})",
                    text,
                    primitive.display_name,
                    primitive.category.label()
                ),
                range,
            });
        }
        let doc = symbols.doc_of(text).unwrap_or("Unknown type.");
        return Some(HoverInfo {
            contents: format!("**Type** `{}`\n\n{}", text, doc),
            range,
        });
    }
    let qualified_name = qualified_callable_at(tokens, token, text);
    let callable = qualified_name
        .as_deref()
        .and_then(|name| crate::completer::callable_for(name, symbols, manifest))
        .or_else(|| crate::completer::callable_for(text, symbols, manifest));
    if let Some(callable) = callable {
        return Some(HoverInfo {
            contents: format!(
                "**Callable** `{}`\n\n{}",
                crate::completer::callable_signature(&callable),
                callable.help
            ),
            range,
        });
    }
    if symbols.functions.contains(text)
        || qualified_name.as_ref().is_some_and(|name| symbols.functions.contains(name))
    {
        return Some(HoverInfo {
            contents: format!("**Callable** `{}`", qualified_name.as_deref().unwrap_or(text)),
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
        if let Some(property) = manifest.properties.iter().find(|property| property.name == text) {
            let actor_types = property.actor_types.join(", ");
            return Some(HoverInfo {
                contents: format!(
                    "**Property** `{}`\n\nType: {:?}\nExtensions: {}",
                    text, property.ty, actor_types
                ),
                range,
            });
        }
        if let Some(doc) = crate::completer::property_documentation(text) {
            return Some(HoverInfo {
                contents: format!("**Property** `{}`\n\n{}", text, doc),
                range,
            });
        }
    }

    None
}

fn qualified_callable_at(tokens: &[Token], token: &Token, text: &str) -> Option<String> {
    let index = tokens.iter().position(|candidate| candidate.span == token.span)?;
    if index >= 2
        && matches!(tokens[index - 1].kind, TokenKind::ColonColon)
        && let TokenKind::Ident(namespace) = &tokens[index - 2].kind
    {
        return Some(format!("{namespace}::{text}"));
    }
    if index >= 2
        && matches!(tokens[index - 1].kind, TokenKind::Dot)
        && let TokenKind::Ident(receiver) = &tokens[index - 2].kind
    {
        return Some(format!("{receiver}.{text}"));
    }
    // A standard callable is also available unqualified, but explicit `std::`
    // qualification should be reflected in the displayed signature.
    None
}

fn token_text<'a>(source: &'a str, token: &Token) -> Option<&'a str> {
    source.get(token.span.start..token.span.end)
}

fn token_range(index: &LineIndex<'_>, token: &Token) -> (usize, usize, usize, usize) {
    let (start_line, start_col) = index.byte_to_line_col_utf16(token.span.start);
    let (end_line, end_col) = index.byte_to_line_col_utf16(token.span.end);
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
