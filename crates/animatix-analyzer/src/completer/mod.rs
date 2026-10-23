//! Context-aware completion provider.

use animatix_syntax::token::{Token, TokenKind, line_col_to_byte};

use crate::extension_manifest::ExtensionManifest;
use crate::symbol_table::{LabelKind, SymbolTable};

/// A completion item to suggest to the user.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The text to display in the completion list.
    pub label: String,
    /// The kind of completion (for icon/color).
    pub kind: CompletionKind,
    /// Additional detail (e.g., type info).
    pub detail: Option<String>,
    /// Documentation string.
    pub documentation: Option<String>,
    /// Text to insert (if different from label).
    pub insert_text: Option<String>,
}

/// The kind of completion item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    /// A language keyword (e.g., `let`, `import`).
    Keyword,
    /// A type name (e.g., `Text`, `Rect`).
    Type,
    /// An actor property name (e.g., `content`, `position`).
    Property,
    /// An actor or variable label.
    Label,
    /// An action verb (e.g., `fade-in`, `move`).
    Action,
    /// A literal value (e.g., `true`, `null`).
    Value,
    /// A code snippet/template.
    Snippet,
}

/// Get completions at a cursor position.
pub fn completions_at(
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    line: usize,
    col: usize,
    manifest: &ExtensionManifest,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    let context = if tokens.is_empty() {
        None
    } else {
        let byte = line_col_to_byte(source, line, col);
        Some(CompletionContext::from_tokens(tokens, byte, symbols, source))
    };

    match context {
        None => {
            items.extend(snippet_completions());
            items.extend(keyword_completions(symbols));
            items.extend(label_completions(symbols));
            items.extend(type_completions(symbols, manifest));
            items.extend(action_completions(symbols));
        },
        Some(CompletionContext::TopLevel) => {
            items.extend(snippet_completions());
            items.extend(keyword_completions(symbols));
            items.extend(label_completions(symbols));
            items.extend(type_completions(symbols, manifest));
            items.extend(action_completions(symbols));
        },
        Some(CompletionContext::TypePosition) => {
            items.extend(type_completions(symbols, manifest));
        },
        Some(CompletionContext::PropertyBlock { actor_type }) => {
            items.extend(property_completions(symbols, actor_type.as_deref(), manifest));
            items.extend(value_completions());
        },
        Some(CompletionContext::ActionTarget) => {
            items.extend(label_completions(symbols));
        },
        Some(CompletionContext::VerbPosition) => {
            items.extend(action_completions(symbols));
            items.extend(keyword_completions(symbols));
        },
        Some(CompletionContext::ModifierList) => {
            items.extend(modifier_completions());
        },
        Some(CompletionContext::PropertyValue {
            property_name,
            actor_type,
        }) => {
            items.extend(value_for_property(&property_name, actor_type.as_deref(), symbols));
        },
        Some(CompletionContext::Unknown) => {
            items.extend(keyword_completions(symbols));
            items.extend(label_completions(symbols));
            items.extend(type_completions(symbols, manifest));
        },
    }

    items
}

/// The context in which completion is being requested.
enum CompletionContext {
    /// Top-level of the file (keywords, labels, types)
    TopLevel,
    /// After ":" in "label: " (expecting a type name)
    TypePosition,
    /// Inside a property block { ... } (expecting property names)
    PropertyBlock { actor_type: Option<String> },
    /// After an action verb (expecting actor labels)
    ActionTarget,
    /// At a statement's verb slot: start of a line/block, or a word that is
    /// not a known action in verb position (offers action verbs so a typo'd
    /// verb like `fly` can be corrected from the completion list)
    VerbPosition,
    /// Inside a modifier list [ ... ] (expecting modifier names)
    ModifierList,
    /// Inside a property value (after "=" or ":")
    PropertyValue {
        property_name: Option<String>,
        actor_type: Option<String>,
    },
    /// Unknown context
    Unknown,
}

impl CompletionContext {
    fn from_tokens(tokens: &[Token], byte: usize, symbols: &SymbolTable, source: &str) -> Self {
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
        if let TokenKind::Ident(name) = &t.kind {
            if symbols.types.contains(name) {
                return Some(name.clone());
            }
        }
    }
    None
}

/// Snippet completions for common patterns.
use actions::*;
use properties::*;
use snippets::*;
use values::*;

mod actions;
mod properties;
mod snippets;
#[cfg(test)]
mod tests;
mod values;

pub use properties::property_documentation;
pub use snippets::all_snippets;
