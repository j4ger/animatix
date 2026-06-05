//! Hover information provider.

use crate::symbol_table::{LabelKind, SymbolTable};
use crate::types::HoverInfo;
use tree_sitter::Tree;

/// Get hover information at a cursor position.
pub fn hover_at(
    symbols: &SymbolTable,
    tree: Option<&Tree>,
    source: &str,
    line: usize,
    col: usize,
) -> Option<HoverInfo> {
    let tree = tree?;
    let point = tree_sitter::Point::new(line, col);
    let node = tree.root_node().descendant_for_point_range(point, point)?;

    let text = &source[node.byte_range()];

    // Check what kind of node we're hovering over
    match node.kind() {
        "identifier" => {
            // Check if it's a label
            if let Some(info) = symbols.labels.get(text) {
                let kind = match info.kind {
                    LabelKind::Actor => "Actor",
                    LabelKind::Let => "Variable",
                    LabelKind::For => "Loop variable",
                    LabelKind::Always => "Always block",
                    LabelKind::Component => "Component",
                };
                let ty = info.ty.as_deref().unwrap_or("unknown");
                Some(HoverInfo {
                    contents: format!("**{}** `{}`\n\nType: {}", kind, text, ty),
                    range: Some((
                        node.start_position().row,
                        node.start_position().column,
                        node.end_position().row,
                        node.end_position().column,
                    )),
                })
            }
            // Check if it's a type
            else if symbols.types.contains(text) {
                let doc = type_documentation(text);
                Some(HoverInfo {
                    contents: format!("**Type** `{}`\n\n{}", text, doc),
                    range: Some((
                        node.start_position().row,
                        node.start_position().column,
                        node.end_position().row,
                        node.end_position().column,
                    )),
                })
            }
            // Check if it's an action
            else if symbols.actions.contains(text) {
                let doc = action_documentation(text);
                Some(HoverInfo {
                    contents: format!("**Action** `{}`\n\n{}", text, doc),
                    range: Some((
                        node.start_position().row,
                        node.start_position().column,
                        node.end_position().row,
                        node.end_position().column,
                    )),
                })
            }
            // Check if it's a keyword
            else if symbols.keywords.contains(text) {
                let doc = keyword_documentation(text);
                Some(HoverInfo {
                    contents: format!("**Keyword** `{}`\n\n{}", text, doc),
                    range: Some((
                        node.start_position().row,
                        node.start_position().column,
                        node.end_position().row,
                        node.end_position().column,
                    )),
                })
            }
            // Check if it's a property name (node is the "name" child of a "property" node)
            else if let Some(parent) = node.parent() {
                if parent.kind() == "property" {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        if name_node.id() == node.id() {
                            if let Some(doc) = crate::completer::property_documentation(text) {
                                return Some(HoverInfo {
                                    contents: format!("**Property** `{}`\n\n{}", text, doc),
                                    range: Some((
                                        node.start_position().row,
                                        node.start_position().column,
                                        node.end_position().row,
                                        node.end_position().column,
                                    )),
                                });
                            }
                        }
                    }
                }
                // Check if it's a component
                if symbols.components.contains_key(text) {
                    let info = &symbols.components[text];
                    let params_str = info.params.iter()
                        .map(|p| match &p.param_type {
                            Some(ty) => format!("{}: {:?}", p.name, ty),
                            None => p.name.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Some(HoverInfo {
                        contents: format!("**Component** `{}`\n\nParameters: ({})", text, params_str),
                        range: Some((
                            node.start_position().row,
                            node.start_position().column,
                            node.end_position().row,
                            node.end_position().column,
                        )),
                    });
                } else {
                    None
                }
            } else {
                None
            }
        }
        // type_identifier doesn't exist in tree-sitter-animatix; types are just identifiers
        // handled by the identifier branch above via symbols.types.contains(text)
        "string" => Some(HoverInfo {
            contents: format!("**String** `{}`", text),
            range: Some((
                node.start_position().row,
                node.start_position().column,
                node.end_position().row,
                node.end_position().column,
            )),
        }),
        "number" | "time_literal" | "percentage" => Some(HoverInfo {
            contents: format!("**Number** `{}`", text),
            range: Some((
                node.start_position().row,
                node.start_position().column,
                node.end_position().row,
                node.end_position().column,
            )),
        }),
        "comment" => Some(HoverInfo {
            contents: format!("*Comment*\n\n{}", text),
            range: Some((
                node.start_position().row,
                node.start_position().column,
                node.end_position().row,
                node.end_position().column,
            )),
        }),
        _ => None,
    }
}

/// Documentation for a type.
pub fn type_documentation(name: &str) -> &'static str {
    match name {
        "Text" => "Text element with content and styling properties.",
        "Math" => "Mathematical expression renderer.",
        "Code" => "Code block with syntax highlighting.",
        "Svg" => "SVG image element.",
        "Image" => "Raster image element.",
        "Rect" => "Rectangle shape with fill and stroke.",
        "Ellipse" => "Ellipse, circle, arc, or dot shape.",
        "Line" => "Line segment or arrow with optional head.",
        "Polygon" => "Polygon or regular polygon shape.",
        "Path" => "SVG path element.",
        "Graph" => "Function graph.",
        "PlotCurve" => "Plot curve with configurable sampling kind.",
        "Button" => "Interactive button element.",
        _ => "Unknown type.",
    }
}

/// Documentation for an action.
pub fn action_documentation(name: &str) -> &'static str {
    match name {
        "fade-in" => "Fade in from transparent.",
        "draw-in" => "Draw in (like handwriting).",
        "wipe-in" => "Wipe in from edge.",
        "fade-out" => "Fade out to transparent.",
        "wipe-out" => "Wipe out to edge.",
        "reveal-out" => "Reveal out (reverse draw).",
        "draw-out" => "Draw out (reverse handwriting).",
        "move" => "Move to position: `move target to (x, y)`",
        "shift" => "Shift by offset: `shift target by (dx, dy)`",
        "rotate" => "Rotate: `rotate target by 90`",
        "scale" => "Scale: `scale target to 2`",
        _ => "Unknown action.",
    }
}

/// Documentation for a keyword.
pub fn keyword_documentation(name: &str) -> &'static str {
    match name {
        "let" => "Declare a variable: `let name = value`",
        "import" => "Import another file: `import \"path\"`",
        "always" => "Reactive block that runs continuously.",
        "if" => "Conditional: `if condition { ... }`",
        "else" => "Else branch: `if ... { } else { }`",
        "for" => "Loop: `for item in collection { ... }`",
        "in" => "Used in for loops.",
        "pub" => "Make visible to other files.",
        "component" => "Define a reusable component.",
        "sequence" => "Run actions in sequence.",
        "stagger" => "Stagger actions with delay.",
        _ => "Keyword.",
    }
}
