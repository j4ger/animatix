use super::callables::method_completions;
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
            items.extend(callable_completions(symbols, manifest));
        },
        Some(CompletionContext::TopLevel) => {
            items.extend(snippet_completions());
            items.extend(keyword_completions(symbols));
            items.extend(label_completions(symbols));
            items.extend(type_completions(symbols, manifest));
            items.extend(callable_completions(symbols, manifest));
        },
        Some(CompletionContext::TypePosition) => {
            items.extend(type_completions(symbols, manifest));
        },
        Some(CompletionContext::PropertyBlock { actor_type }) => {
            items.extend(property_completions(symbols, actor_type.as_deref(), manifest));
            items.extend(value_completions());
        },
        Some(CompletionContext::CallablePosition) => {
            items.extend(callable_completions(symbols, manifest));
            items.extend(keyword_completions(symbols));
        },
        Some(CompletionContext::NamespacePosition { prefix }) => {
            let prefix = format!("{prefix}::");
            items.extend(
                callable_completions(symbols, manifest)
                    .into_iter()
                    .filter(|item| item.label.starts_with(&prefix)),
            );
        },
        Some(CompletionContext::MethodPosition { receiver }) => {
            items.extend(member_completions(symbols, &receiver));
            items.extend(method_completions(symbols, &receiver));
        },
        Some(CompletionContext::CallArgument {
            callee,
            argument_index,
            current_name,
            supplied_names,
        }) => {
            items.extend(argument_completions(
                symbols,
                manifest,
                callee.as_deref(),
                argument_index,
                current_name.as_deref(),
                &supplied_names,
            ));
        },
        Some(CompletionContext::ModifierList) => {
            items.extend(modifier_completions());
        },
        Some(CompletionContext::PropertyValue {
            property_name,
            actor_type,
        }) => {
            items.extend(value_for_property(&property_name, actor_type.as_deref(), symbols));
            items.extend(callable_completions(symbols, manifest));
        },
        Some(CompletionContext::Unknown) => {
            items.extend(keyword_completions(symbols));
            items.extend(label_completions(symbols));
            items.extend(type_completions(symbols, manifest));
            items.extend(callable_completions(symbols, manifest));
        },
    }

    items
}

fn member_completions(symbols: &SymbolTable, receiver: &str) -> Vec<CompletionItem> {
    use animatix_core::builtins::members::{ACTOR_GEOMETRY_MEMBERS, BOX_POINT_MEMBERS};
    use animatix_syntax::{ast::Expr, typing::Type};
    let ty = symbols.infer_expr_type(&Expr::Path(receiver.split('.').map(str::to_owned).collect()));
    let (names, additional, detail) = match ty {
        Type::Actor(_) | Type::Component(_) => {
            (BOX_POINT_MEMBERS, ACTOR_GEOMETRY_MEMBERS, "Vec2 · read-only geometry")
        },
        Type::List(_) | Type::Str => (&["length"][..], &[][..], "Num · read-only length"),
        _ if receiver == "scene" => (BOX_POINT_MEMBERS, &[][..], "Vec2 · scene anchor"),
        _ => return Vec::new(),
    };
    names
        .iter()
        .chain(additional)
        .map(|name| CompletionItem {
            label: (*name).into(),
            kind: CompletionKind::Property,
            detail: Some(detail.into()),
            documentation: None,
            insert_text: Some((*name).into()),
        })
        .collect()
}
