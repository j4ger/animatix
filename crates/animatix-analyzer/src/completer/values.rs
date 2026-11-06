use animatix_syntax::ast::TypeAnnotation;

use super::*;

/// Value completions for a property, derived from the component definition's
/// declared parameter type, or from the property name when the type is
/// unannotated.
pub(super) fn value_for_property(
    property_name: &Option<String>,
    actor_type: Option<&str>,
    symbols: &SymbolTable,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Look up expected type from component definition
    let expected_type = expected_type_for(actor_type, property_name, symbols);

    // Provide type-specific completions
    if let Some(ref ty) = expected_type {
        items.extend(completions_for_type(ty));
    }

    // Property-name-specific completions (fallback when no type annotation)
    if expected_type.is_none()
        && let Some(name) = property_name
    {
        items.extend(completions_for_property_name(name));
    }

    // Always add common values
    items.extend(value_completions());
    items
}

/// Resolve the parameter type the component definition declares for `property_name`.
fn expected_type_for(
    actor_type: Option<&str>,
    property_name: &Option<String>,
    symbols: &SymbolTable,
) -> Option<TypeAnnotation> {
    let (ty, prop) = (actor_type?, property_name.as_ref()?);
    symbols.components.get(ty).and_then(|info| {
        info.params.iter().find(|p| p.name == *prop).and_then(|p| p.param_type.clone())
    })
}

/// Value completions for one declared parameter type.
fn completions_for_type(ty: &TypeAnnotation) -> Vec<CompletionItem> {
    match ty {
        TypeAnnotation::Union(types) => union_type_completions(types),
        other => simple_type_completions(other),
    }
}

/// Value completions for a single (non-union) type annotation.
fn simple_type_completions(ty: &TypeAnnotation) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    match ty {
        TypeAnnotation::Bool => {
            items.extend(["true", "false"].iter().map(|v| CompletionItem {
                label: v.to_string(),
                kind: CompletionKind::Value,
                detail: Some("Boolean".to_string()),
                documentation: None,
                insert_text: None,
            }));
        },
        TypeAnnotation::Vec2 => {
            items.push(CompletionItem {
                label: "(0, 0)".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("Vec2".to_string()),
                documentation: Some("2D vector (x, y)".to_string()),
                insert_text: Some("(${1:x}, ${2:y})".to_string()),
            });
        },
        TypeAnnotation::Vec3 => {
            items.push(CompletionItem {
                label: "(0, 0, 0)".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("Vec3".to_string()),
                documentation: Some("3D vector (x, y, z)".to_string()),
                insert_text: Some("(${1:x}, ${2:y}, ${3:z})".to_string()),
            });
        },
        TypeAnnotation::Tuple(_) => {
            items.push(CompletionItem {
                label: "(...)".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("Tuple".to_string()),
                documentation: Some("Fixed-size heterogeneous tuple".to_string()),
                insert_text: Some("(${1:value}, ${2:value})".to_string()),
            });
        },
        TypeAnnotation::Scene => {
            items.push(CompletionItem {
                label: "scene".to_string(),
                kind: CompletionKind::Value,
                detail: Some("Scene".to_string()),
                documentation: Some("Scene reference".to_string()),
                insert_text: None,
            });
        },
        TypeAnnotation::Vec4 => {
            items.push(CompletionItem {
                label: "(0, 0, 0, 0)".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("Vec4".to_string()),
                documentation: Some("4D vector (x, y, z, w)".to_string()),
                insert_text: Some("(${1:x}, ${2:y}, ${3:z}, ${4:w})".to_string()),
            });
        },
        TypeAnnotation::Color => {
            items.extend(["red", "blue", "green", "yellow", "white", "black"].iter().map(|v| {
                CompletionItem {
                    label: v.to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Color".to_string()),
                    documentation: None,
                    insert_text: None,
                }
            }));
            items.push(CompletionItem {
                label: "rgb(...)".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("Color".to_string()),
                documentation: Some("RGB color: rgb(r, g, b)".to_string()),
                insert_text: Some("rgb(${1:255}, ${2:255}, ${3:255})".to_string()),
            });
        },
        TypeAnnotation::Str => {
            items.push(CompletionItem {
                label: "\"\"".to_string(),
                kind: CompletionKind::Snippet,
                detail: Some("String".to_string()),
                documentation: Some("String literal".to_string()),
                insert_text: Some("\"${1:text}\"".to_string()),
            });
        },
        TypeAnnotation::Num => {
            items.extend(["0", "1", "0.5"].iter().map(|v| CompletionItem {
                label: v.to_string(),
                kind: CompletionKind::Value,
                detail: Some("Number".to_string()),
                documentation: None,
                insert_text: None,
            }));
        },
        _ => {},
    }
    items
}

/// Value completions for each member of a `Union` annotation, in declaration
/// order. A member without a literal form contributes nothing.
fn union_type_completions(types: &[TypeAnnotation]) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for ty in types {
        match ty {
            TypeAnnotation::Bool
            | TypeAnnotation::Str
            | TypeAnnotation::Num
            | TypeAnnotation::Vec3
            | TypeAnnotation::Tuple(_) => items.extend(simple_type_completions(ty)),
            _ => {},
        }
    }
    items
}

/// Value completions for a name-keyed property, used when the component
/// definition carries no parameter type for it.
fn completions_for_property_name(name: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    match name {
        "overflow" => {
            items.extend(["visible", "clip", "ellipsis"].iter().map(|v| CompletionItem {
                label: v.to_string(),
                kind: CompletionKind::Value,
                detail: Some("Overflow".to_string()),
                documentation: None,
                insert_text: Some(format!("\"{}\"", v)),
            }));
        },
        "text_align" => {
            items.extend(["left", "center", "right", "justify"].iter().map(|v| CompletionItem {
                label: v.to_string(),
                kind: CompletionKind::Value,
                detail: Some("Alignment".to_string()),
                documentation: None,
                insert_text: Some(format!("\"{}\"", v)),
            }));
        },
        "text_anchor" => {
            items.extend(["left", "center", "right"].iter().map(|v| CompletionItem {
                label: v.to_string(),
                kind: CompletionKind::Value,
                detail: Some("Anchor".to_string()),
                documentation: None,
                insert_text: Some(format!("\"{}\"", v)),
            }));
        },
        "anchor" => {
            items.extend(
                [
                    "center",
                    "top",
                    "bottom",
                    "left",
                    "right",
                    "top-left",
                    "top-right",
                    "bottom-left",
                    "bottom-right",
                ]
                .iter()
                .map(|v| CompletionItem {
                    label: v.to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Anchor point".to_string()),
                    documentation: None,
                    insert_text: Some(format!("\"{}\"", v)),
                }),
            );
        },
        "opacity" | "scale" => {
            items.extend([0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|v| CompletionItem {
                label: format!("{}", v),
                kind: CompletionKind::Value,
                detail: Some("Number".to_string()),
                documentation: None,
                insert_text: None,
            }));
        },
        _ => {},
    }
    items
}
