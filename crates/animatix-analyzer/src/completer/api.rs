use super::*;

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
        // LSP positions count UTF-16 code units, not Unicode scalar values.
        let byte = line_col_to_byte_utf16(source, line, col);
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
