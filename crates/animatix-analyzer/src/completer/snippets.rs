use super::*;

pub(super) fn snippet_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "actor".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Actor declaration".to_string()),
            documentation: Some("Declare a new actor with type and properties".to_string()),
            insert_text: Some("${1:label}: ${2:Text}, ${3:content}: \"${4:}\"".to_string()),
        },
        CompletionItem {
            label: "keyframe".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Keyframe block".to_string()),
            documentation: Some("Create a keyframe at a specific time".to_string()),
            insert_text: Some("# ${1:0s}\n${2:}".to_string()),
        },
        CompletionItem {
            label: "relkeyframe".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Relative keyframe".to_string()),
            documentation: Some("Create a relative keyframe offset".to_string()),
            insert_text: Some("#+ ${1:1s}\n${2:}".to_string()),
        },
        CompletionItem {
            label: "component".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Component definition".to_string()),
            documentation: Some("Define a reusable component".to_string()),
            insert_text: Some("component ${1:Name}(${2:params}) {\n    ${3:}\n}".to_string()),
        },
        CompletionItem {
            label: "if".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Conditional block".to_string()),
            documentation: Some("Conditional statement".to_string()),
            insert_text: Some("if ${1:condition} {\n    ${2:}\n}".to_string()),
        },
        CompletionItem {
            label: "for".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Loop block".to_string()),
            documentation: Some("Iterate over items".to_string()),
            insert_text: Some("for ${1:item} in ${2:items} {\n    ${3:}\n}".to_string()),
        },
        CompletionItem {
            label: "always".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Always block".to_string()),
            documentation: Some("Reactive block that runs continuously".to_string()),
            insert_text: Some("always {\n    ${1:}\n}".to_string()),
        },
        CompletionItem {
            label: "sequence".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Sequence block".to_string()),
            documentation: Some("Run actions in sequence".to_string()),
            insert_text: Some("sequence {\n    ${1:}\n}".to_string()),
        },
        CompletionItem {
            label: "stagger".to_string(),
            kind: CompletionKind::Snippet,
            detail: Some("Stagger block".to_string()),
            documentation: Some("Stagger actions with delay".to_string()),
            insert_text: Some("stagger [${1:150ms}] {\n    ${2:}\n}".to_string()),
        },
    ]
}

/// Expose all snippet completions for external consumers (e.g. insertion palette).
pub fn all_snippets() -> Vec<CompletionItem> {
    snippet_completions()
}

/// Keyword completions with documentation.
pub(super) fn keyword_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols
        .keywords
        .iter()
        .map(|kw| {
            let doc = animatix_syntax::builtins::keyword_documentation(kw);
            CompletionItem {
                label: kw.clone(),
                kind: CompletionKind::Keyword,
                detail: Some("Keyword".to_string()),
                documentation: Some(doc.to_string()),
                insert_text: None,
            }
        })
        .collect()
}

/// Type completions with documentation.
pub(super) fn type_completions(
    symbols: &SymbolTable,
    manifest: &ExtensionManifest,
) -> Vec<CompletionItem> {
    symbols
        .types
        .iter()
        .map(|ty| {
            let (detail, doc) = manifest
                .primitives
                .iter()
                .find(|primitive| primitive.type_name == *ty)
                .map(|primitive| {
                    (
                        primitive.category.label().to_string(),
                        format!("{} ({})", primitive.display_name, primitive.category.label()),
                    )
                })
                .unwrap_or_else(|| {
                    ("Type".to_string(), symbols.doc_of(ty).unwrap_or("Unknown type.").to_string())
                });
            CompletionItem {
                label: ty.clone(),
                kind: CompletionKind::Type,
                detail: Some(detail),
                documentation: Some(doc),
                insert_text: None,
            }
        })
        .collect()
}

/// Label completions (actor names, let bindings).
pub(super) fn label_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols
        .labels
        .iter()
        .map(|(name, info)| {
            let detail = match info.kind {
                LabelKind::Actor => info.ty.as_ref().map(|t| format!("Actor: {}", t)),
                LabelKind::Let => Some("Variable".to_string()),
                LabelKind::For => Some("Loop variable".to_string()),
                LabelKind::Always => Some("Always block".to_string()),
                LabelKind::Component => Some("Component".to_string()),
            };
            CompletionItem {
                label: name.clone(),
                kind: CompletionKind::Label,
                detail,
                documentation: None,
                insert_text: None,
            }
        })
        .collect()
}
