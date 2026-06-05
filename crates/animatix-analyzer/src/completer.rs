//! Context-aware completion provider.

use crate::symbol_table::{SymbolTable, LabelKind};
use tree_sitter::{Tree, Point};

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
    tree: Option<&Tree>,
    source: &str,
    line: usize,
    col: usize,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Determine context from tree-sitter
    if let Some(tree) = tree {
        let point = Point::new(line, col);
        let node = tree.root_node().descendant_for_point_range(point, point);
        if let Some(node) = node {
            let context = CompletionContext::from_node(node, source);

            match context {
                CompletionContext::TopLevel => {
                    items.extend(snippet_completions());
                    items.extend(keyword_completions(symbols));
                    items.extend(label_completions(symbols));
                    items.extend(type_completions(symbols));
                    items.extend(action_completions(symbols));
                }
                CompletionContext::TypePosition => {
                    items.extend(type_completions(symbols));
                }
                CompletionContext::PropertyBlock { actor_type } => {
                    items.extend(property_completions(symbols, actor_type.as_deref()));
                    items.extend(value_completions());
                }
                CompletionContext::ActionTarget => {
                    items.extend(label_completions(symbols));
                }
                CompletionContext::ModifierList => {
                    items.extend(modifier_completions());
                }
                CompletionContext::PropertyValue { property_name, actor_type } => {
                    items.extend(value_for_property(&property_name, actor_type.as_deref(), symbols));
                }
                CompletionContext::Unknown => {
                    items.extend(keyword_completions(symbols));
                    items.extend(label_completions(symbols));
                    items.extend(type_completions(symbols));
                }
            }
        }
    } else {
        items.extend(snippet_completions());
        items.extend(keyword_completions(symbols));
        items.extend(label_completions(symbols));
        items.extend(type_completions(symbols));
        items.extend(action_completions(symbols));
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
    fn from_node(node: tree_sitter::Node, source: &str) -> Self {
        let kind = node.kind();

        // Check parent context
        if let Some(parent) = node.parent() {
            let parent_kind = parent.kind();

            match parent_kind {
                // Inside a property list
                "property_list" => {
                    let actor_type = find_actor_type(parent, source);
                    return CompletionContext::PropertyBlock { actor_type };
                }

                // After ":" in actor declaration
                "actor_declaration" => {
                    if is_after_colon(node, source) {
                        return CompletionContext::TypePosition;
                    }
                }

                // After action verb
                "action_invocation" => {
                    return CompletionContext::ActionTarget;
                }

                // Inside modifier list
                "modifier_list" | "modifier" => {
                    return CompletionContext::ModifierList;
                }

                // Property value context
                "property" => {
                    if kind == "identifier" || kind == "string" || kind == "number" {
                        let prop_name = find_property_name(parent, source);
                        let actor_type = find_actor_type(parent, source);
                        return CompletionContext::PropertyValue {
                            property_name: prop_name,
                            actor_type,
                        };
                    }
                }

                _ => {}
            }
        }

        // Check if we're at the top level
        if node.parent().is_none_or(|p| p.kind() == "source_file") {
            return CompletionContext::TopLevel;
        }

        CompletionContext::Unknown
    }
}

/// Find the actor type for a property block by walking up the tree.
fn find_actor_type(node: tree_sitter::Node, source: &str) -> Option<String> {
    let mut current = node.parent()?;
    loop {
        match current.kind() {
            "actor_declaration" => {
                if let Some(type_node) = current.child_by_field_name("type") {
                    return Some(source[type_node.byte_range()].to_string());
                }
            }
            _ => {}
        }
        current = current.parent()?;
    }
}

/// Find the property name for a property value node.
fn find_property_name(node: tree_sitter::Node, source: &str) -> Option<String> {
    if node.kind() == "property" {
        if let Some(name_node) = node.child_by_field_name("name") {
            return Some(source[name_node.byte_range()].to_string());
        }
    }
    None
}

/// Check if a node is after a colon in a declaration.
fn is_after_colon(node: tree_sitter::Node, source: &str) -> bool {
    let start = node.start_byte();
    if start == 0 {
        return false;
    }
    let before = &source[..start];
    before.ends_with(':')
}

/// Snippet completions for common patterns.
fn snippet_completions() -> Vec<CompletionItem> {
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
fn keyword_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols.keywords.iter().map(|kw| {
        let doc = match kw.as_str() {
            "let" => Some("Declare a variable: let name = value"),
            "import" => Some("Import another file: import \"path\""),
            "always" => Some("Reactive block that runs continuously"),
            "if" => Some("Conditional: if condition { ... }"),
            "else" => Some("Else branch: if ... { } else { }"),
            "for" => Some("Loop: for item in collection { ... }"),
            "in" => Some("Used in for loops"),
            "pub" => Some("Make visible to other files"),
            "component" => Some("Define a reusable component"),
            "sequence" => Some("Run actions in sequence"),
            "stagger" => Some("Stagger actions with delay"),
            _ => None,
        };
        CompletionItem {
            label: kw.clone(),
            kind: CompletionKind::Keyword,
            detail: Some("Keyword".to_string()),
            documentation: doc.map(|s| s.to_string()),
            insert_text: None,
        }
    }).collect()
}

/// Type completions with documentation.
fn type_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols.types.iter().map(|ty| {
        let doc = match ty.as_str() {
            "Text" => Some("Text element with content and styling"),
            "Math" => Some("Mathematical expression renderer"),
            "Code" => Some("Code block with syntax highlighting"),
            "Svg" => Some("SVG image element"),
            "Image" => Some("Raster image element"),
            "Rect" => Some("Rectangle shape"),
            "Ellipse" => Some("Ellipse, circle, arc, or dot"),
            "Line" => Some("Line segment or arrow"),
            "Polygon" => Some("Polygon or regular polygon"),
            "Path" => Some("SVG path element"),
            "Graph" => Some("Function graph"),
            "PlotCurve" => Some("Plot curve with configurable sampling kind"),
            "Button" => Some("Interactive button element"),
            _ => None,
        };
        CompletionItem {
            label: ty.clone(),
            kind: CompletionKind::Type,
            detail: Some("Type".to_string()),
            documentation: doc.map(|s| s.to_string()),
            insert_text: None,
        }
    }).collect()
}

/// Label completions (actor names, let bindings).
fn label_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols.labels.iter().map(|(name, info)| {
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
    }).collect()
}

/// Property completions for a given actor type.
fn property_completions(symbols: &SymbolTable, actor_type: Option<&str>) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some(ty) = actor_type {
        if let Some(props) = symbols.properties.get(ty) {
            for prop in props {
                let doc = property_documentation(prop);
                items.push(CompletionItem {
                    label: prop.clone(),
                    kind: CompletionKind::Property,
                    detail: Some(format!("Property of {}", ty)),
                    documentation: doc.map(|s| s.to_string()),
                    insert_text: Some(format!("{}: ", prop)),
                });
            }
        }
    }

    // Also suggest common properties
    for prop in &["position", "anchor", "offset", "scale", "rotation", "opacity", "color"] {
        if !items.iter().any(|i| &i.label == prop) {
            let doc = property_documentation(prop);
            items.push(CompletionItem {
                label: prop.to_string(),
                kind: CompletionKind::Property,
                detail: Some("Common property".to_string()),
                documentation: doc.map(|s| s.to_string()),
                insert_text: Some(format!("{}: ", prop)),
            });
        }
    }

    items
}

/// Documentation for common properties.
pub fn property_documentation(name: &str) -> Option<&str> {
    match name {
        "position" | "at" => Some("Position as (x, y) coordinates"),
        "anchor" => Some("Anchor point for positioning"),
        "offset" => Some("Offset from position"),
        "scale" => Some("Scale factor (1.0 = normal)"),
        "rotation" => Some("Rotation in degrees"),
        "opacity" => Some("Opacity (0.0 = transparent, 1.0 = opaque)"),
        "color" => Some("Color value (name, hex, or rgb)"),
        "content" => Some("Text content"),
        "font_size" => Some("Font size in pixels"),
        "font_family" => Some("Font family name"),
        "text_align" => Some("Text alignment (left, center, right)"),
        "fill" => Some("Fill color"),
        "stroke" => Some("Stroke color"),
        "stroke_width" => Some("Stroke width in pixels"),
        "size" => Some("Size as (width, height)"),
        "radius" => Some("Radius for circles/rounded shapes"),
        "start" => Some("Start point as (x, y)"),
        "end" => Some("End point as (x, y)"),
        "url" => Some("URL for image/svg"),
        "text" => Some("Button text"),
        "x_range" => Some("X axis range"),
        "y_range" => Some("Y axis range"),
        "function" => Some("Function to plot"),
        _ => None,
    }
}

/// Action completions with documentation.
fn action_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols.actions.iter().map(|action| {
        let doc = match action.as_str() {
            "fade-in" => Some("Fade in from transparent"),
            "draw-in" => Some("Draw in (like handwriting)"),
            "wipe-in" => Some("Wipe in from edge"),
            "fade-out" => Some("Fade out to transparent"),
            "wipe-out" => Some("Wipe out to edge"),
            "reveal-out" => Some("Reveal out (reverse draw)"),
            "draw-out" => Some("Draw out (reverse handwriting)"),
            "move" => Some("Move to position: move target to (x, y)"),
            "shift" => Some("Shift by offset: shift target by (dx, dy)"),
            "rotate" => Some("Rotate: rotate target by 90"),
            "scale" => Some("Scale: scale target to 2"),
            _ => None,
        };
        CompletionItem {
            label: action.clone(),
            kind: CompletionKind::Action,
            detail: Some("Action".to_string()),
            documentation: doc.map(|s| s.to_string()),
            insert_text: None,
        }
    }).collect()
}

/// Modifier completions.
fn modifier_completions() -> Vec<CompletionItem> {
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
            documentation: Some("Easing function: [ease: bounce]".to_string()),
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
fn value_completions() -> Vec<CompletionItem> {
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

/// Value completions based on property name and expected type.
fn value_for_property(
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
            }
            animatix_syntax::ast::TypeAnnotation::Vec2 => {
                items.push(CompletionItem {
                    label: "(0, 0)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Vec2".to_string()),
                    documentation: Some("2D vector (x, y)".to_string()),
                    insert_text: Some("(${1:x}, ${2:y})".to_string()),
                });
            }
            animatix_syntax::ast::TypeAnnotation::Vec4 => {
                items.push(CompletionItem {
                    label: "(0, 0, 0, 0)".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("Vec4".to_string()),
                    documentation: Some("4D vector (x, y, z, w)".to_string()),
                    insert_text: Some("(${1:x}, ${2:y}, ${3:z}, ${4:w})".to_string()),
                });
            }
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
            }
            animatix_syntax::ast::TypeAnnotation::Str => {
                items.push(CompletionItem {
                    label: "\"\"".to_string(),
                    kind: CompletionKind::Snippet,
                    detail: Some("String".to_string()),
                    documentation: Some("String literal".to_string()),
                    insert_text: Some("\"${1:text}\"".to_string()),
                });
            }
            animatix_syntax::ast::TypeAnnotation::Num => {
                items.extend(["0", "1", "0.5"].iter().map(|v| CompletionItem {
                    label: v.to_string(),
                    kind: CompletionKind::Value,
                    detail: Some("Number".to_string()),
                    documentation: None,
                    insert_text: None,
                }));
            }
            _ => {}
        }
    }

    // Property-name-specific completions (fallback when no type annotation)
    if expected_type.is_none() {
        if let Some(name) = property_name {
            match name.as_str() {
                "text_align" => {
                    items.extend(["left", "center", "right"].iter().map(|v| CompletionItem {
                        label: v.to_string(),
                        kind: CompletionKind::Value,
                        detail: Some("Alignment".to_string()),
                        documentation: None,
                        insert_text: Some(format!("\"{}\"", v)),
                    }));
                }
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
                }
                "opacity" | "scale" => {
                    items.extend([0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|v| CompletionItem {
                        label: format!("{}", v),
                        kind: CompletionKind::Value,
                        detail: Some("Number".to_string()),
                        documentation: None,
                        insert_text: None,
                    }));
                }
                _ => {}
            }
        }
    }

    // Always add common values
    items.extend(value_completions());
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    #[test]
    fn top_level_completions_include_keywords() {
        let source = "";
        let symbols = SymbolTable::build_from_ast(&[]);
        let items = completions_at(&symbols, None, source, 0, 0);

        let keywords: Vec<_> = items.iter()
            .filter(|i| i.kind == CompletionKind::Keyword)
            .map(|i| i.label.as_str())
            .collect();

        assert!(keywords.contains(&"let"));
        assert!(keywords.contains(&"import"));
        assert!(keywords.contains(&"if"));
    }

    #[test]
    fn type_completions_include_builtins() {
        let source = "";
        let symbols = SymbolTable::build_from_ast(&[]);
        let items = completions_at(&symbols, None, source, 0, 0);

        let types: Vec<_> = items.iter()
            .filter(|i| i.kind == CompletionKind::Type)
            .map(|i| i.label.as_str())
            .collect();

        assert!(types.contains(&"Text"));
        assert!(types.contains(&"Ellipse"));
        assert!(types.contains(&"Button"));
    }

    #[test]
    fn snippet_completions_at_top_level() {
        let source = "";
        let symbols = SymbolTable::build_from_ast(&[]);
        let items = completions_at(&symbols, None, source, 0, 0);

        let snippets: Vec<_> = items.iter()
            .filter(|i| i.kind == CompletionKind::Snippet)
            .map(|i| i.label.as_str())
            .collect();

        assert!(snippets.contains(&"actor"));
        assert!(snippets.contains(&"keyframe"));
        assert!(snippets.contains(&"component"));
    }

    #[test]
    fn action_completions_include_builtins() {
        let source = "";
        let symbols = SymbolTable::build_from_ast(&[]);
        let items = completions_at(&symbols, None, source, 0, 0);

        let actions: Vec<_> = items.iter()
            .filter(|i| i.kind == CompletionKind::Action)
            .map(|i| i.label.as_str())
            .collect();

        assert!(actions.contains(&"fade-in"));
        assert!(actions.contains(&"move"));
        assert!(actions.contains(&"scale"));
    }
}
