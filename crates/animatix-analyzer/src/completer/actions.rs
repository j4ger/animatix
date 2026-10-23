use super::*;

pub(super) fn action_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols
        .actions
        .iter()
        .map(|action| {
            let doc = animatix_syntax::builtins::action_documentation(action);
            CompletionItem {
                label: action.clone(),
                kind: CompletionKind::Action,
                detail: Some("Action".to_string()),
                documentation: Some(doc.to_string()),
                insert_text: None,
            }
        })
        .collect()
}

/// Modifier completions.
pub(super) fn modifier_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "delay".to_string(),
            kind: CompletionKind::Property,
            detail: Some("Modifier".to_string()),
            documentation: Some("Delay before action starts: [delay: 500ms]".to_string()),
            insert_text: Some("delay: ".to_string()),
        },
        CompletionItem {
            label: "ease".to_string(),
            kind: CompletionKind::Property,
            detail: Some("Modifier".to_string()),
            documentation: Some(
                "Easing curve: [ease: expo-out], or a parameterized [ease: cubic-bezier(0.16, 1, 0.3, 1)] / [ease: spring(6, 9)]".to_string(),
            ),
            insert_text: Some("ease: ".to_string()),
        },
        CompletionItem {
            label: "duration".to_string(),
            kind: CompletionKind::Property,
            detail: Some("Modifier".to_string()),
            documentation: Some("Action duration: [2s]".to_string()),
            insert_text: Some("duration: ".to_string()),
        },
    ]
}

/// Value completions for common property types.
pub(super) fn value_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "true".to_string(),
            kind: CompletionKind::Value,
            detail: Some("Boolean".to_string()),
            documentation: None,
            insert_text: None,
        },
        CompletionItem {
            label: "false".to_string(),
            kind: CompletionKind::Value,
            detail: Some("Boolean".to_string()),
            documentation: None,
            insert_text: None,
        },
        CompletionItem {
            label: "null".to_string(),
            kind: CompletionKind::Value,
            detail: Some("Null value".to_string()),
            documentation: None,
            insert_text: None,
        },
    ]
}
