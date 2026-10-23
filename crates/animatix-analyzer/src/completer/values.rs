use super::*;

pub(super) fn value_for_property(
    property_name: &Option<String>,
    actor_type: Option<&str>,
    symbols: &SymbolTable,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Look up expected type from component definition
    let expected_type = if let (Some(ty), Some(prop)) = (actor_type, property_name) {
        symbols.components.get(ty).and_then(|info| {
            info.params.iter().find(|p| p.name == *prop).and_then(|p| p.param_type.clone())
        })
    } else {
        None
    };

    // Provide type-specific completions
    if let Some(ref ty) = expected_type {
        match ty {
            animatix_syntax::ast::TypeAnnotation::Bool => {
                items.extend(["true", "false"].iter().map(|v| CompletionItem {
                    label: v.to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Boolean".to_string()),
                    documentation: None,
                    insert_text: None,
                }));
            },
            animatix_syntax::ast::TypeAnnotation::Vec2 => {
                items.push(CompletionItem {
                    label: "(0, 0)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Vec2".to_string()),
                    documentation: Some("2D vector (x, y)".to_string()),
                    insert_text: Some("(${1:x}, ${2:y})".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Vec3 => {
                items.push(CompletionItem {
                    label: "(0, 0, 0)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Vec3".to_string()),
                    documentation: Some("3D vector (x, y, z)".to_string()),
                    insert_text: Some("(${1:x}, ${2:y}, ${3:z})".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Tuple(_) => {
                items.push(CompletionItem {
                    label: "(...)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Tuple".to_string()),
                    documentation: Some("Fixed-size heterogeneous tuple".to_string()),
                    insert_text: Some("(${1:value}, ${2:value})".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Scene => {
                items.push(CompletionItem {
                    label: "scene".to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Scene".to_string()),
                    documentation: Some("Scene reference".to_string()),
                    insert_text: None,
                });
            },
            animatix_syntax::ast::TypeAnnotation::Vec4 => {
                items.push(CompletionItem {
                    label: "(0, 0, 0, 0)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Vec4".to_string()),
                    documentation: Some("4D vector (x, y, z, w)".to_string()),
                    insert_text: Some("(${1:x}, ${2:y}, ${3:z}, ${4:w})".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Color => {
                items.extend(["red", "blue", "green", "yellow", "white", "black"].iter().map(
                    |v| CompletionItem {
                        label: v.to_string(),
                        kind: CompletionKind::Value,
                        detail: Some("Color".to_string()),
                        documentation: None,
                        insert_text: None,
                    },
                ));
                items.push(CompletionItem {
                    label: "rgb(...)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Color".to_string()),
                    documentation: Some("RGB color: rgb(r, g, b)".to_string()),
                    insert_text: Some("rgb(${1:255}, ${2:255}, ${3:255})".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Str => {
                items.push(CompletionItem {
                    label: "\"\"".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("String".to_string()),
                    documentation: Some("String literal".to_string()),
                    insert_text: Some("\"${1:text}\"".to_string()),
                });
            },
            animatix_syntax::ast::TypeAnnotation::Num => {
                items.extend(["0", "1", "0.5"].iter().map(|v| CompletionItem {
                    label: v.to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Number".to_string()),
                    documentation: None,
                    insert_text: None,
                }));
            },
            animatix_syntax::ast::TypeAnnotation::Union(types) => {
                for ty in types {
                    match ty {
                        animatix_syntax::ast::TypeAnnotation::Bool => {
                            items.extend(["true", "false"].iter().map(|v| CompletionItem {
                                label: v.to_string(),
                                kind: CompletionKind::Value,
                                detail: Some("Boolean".to_string()),
                                documentation: None,
                                insert_text: None,
                            }));
                        },
                        animatix_syntax::ast::TypeAnnotation::Str => {
                            items.push(CompletionItem {
                                label: "\"\"".to_string(),
                                kind: CompletionKind::Snippet,
                                detail: Some("String".to_string()),
                                documentation: Some("String literal".to_string()),
                                insert_text: Some("\"${1:text}\"".to_string()),
                            });
                        },
                        animatix_syntax::ast::TypeAnnotation::Num => {
                            items.extend(["0", "1", "0.5"].iter().map(|v| CompletionItem {
                                label: v.to_string(),
                                kind: CompletionKind::Value,
                                detail: Some("Number".to_string()),
                                documentation: None,
                                insert_text: None,
                            }));
                        },
                        animatix_syntax::ast::TypeAnnotation::Vec3 => {
                            items.push(CompletionItem {
                                label: "(0, 0, 0)".to_string(),
                                kind: CompletionKind::Snippet,
                                detail: Some("Vec3".to_string()),
                                documentation: Some("3D vector (x, y, z)".to_string()),
                                insert_text: Some("(${1:x}, ${2:y}, ${3:z})".to_string()),
                            });
                        },
                        animatix_syntax::ast::TypeAnnotation::Tuple(_) => {
                            items.push(CompletionItem {
                                label: "(...)".to_string(),
                                kind: CompletionKind::Snippet,
                                detail: Some("Tuple".to_string()),
                                documentation: Some("Fixed-size heterogeneous tuple".to_string()),
                                insert_text: Some("(${1:value}, ${2:value})".to_string()),
                            });
                        },
                        _ => {},
                    }
                }
            },
            _ => {},
        }
    }

    // Property-name-specific completions (fallback when no type annotation)
    if expected_type.is_none() {
        if let Some(name) = property_name {
            match name.as_str() {
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
                    items.extend(["left", "center", "right", "justify"].iter().map(|v| {
                        CompletionItem {
                            label: v.to_string(),
                            kind: CompletionKind::Value,
                            detail: Some("Alignment".to_string()),
                            documentation: None,
                            insert_text: Some(format!("\"{}\"", v)),
                        }
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
        }
    }

    // Always add common values
    items.extend(value_completions());
    items
}
