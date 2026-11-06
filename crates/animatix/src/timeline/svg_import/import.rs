//! SVG import driver: document walk, viewBox configuration, and the attribute
//! helpers every converter shares.

use std::collections::HashMap;
use std::path::Path;

use roxmltree::{Document, Node};

use super::SvgImportError;
use super::defs::{
    GradientDef, MaskDef, collect_gradients, collect_masks, wrap_new_statements_with_mask,
};
use super::shapes::{
    convert_circle, convert_ellipse, convert_group, convert_line, convert_path, convert_poly,
    convert_rect, convert_text, convert_use,
};
use super::transform::{Transform, parse_numbers, parse_transform};
use crate::ast::{Expr, Property, Stmt};

impl std::fmt::Display for SvgImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Xml(e) => write!(f, "XML error: {e}"),
            Self::UnsupportedElement(tag) => write!(f, "unsupported SVG element: <{tag}>"),
        }
    }
}

impl std::error::Error for SvgImportError {}

// Public entry point

/// Read an SVG file and convert it into a `Vec<Stmt>` (Animatix AST statements).
///
/// The returned statements are `Stmt::ActorDecl` nodes for each SVG element:
///
/// | SVG element | Animatix actor | Key properties |
/// |------------|----------------|----------------|
/// | `<g>` | `Group` | `at`, `rotation`, `scale` (from transform) |
/// | `<rect>` | `Rect` | `size`, `at`, `color` |
/// | `<circle>` | `Ellipse` | `size`, `at`, `color` |
/// | `<ellipse>` | `Ellipse` | `size`, `at`, `color` |
/// | `<path>` | `Path` | `commands`, `color` |
/// | `<text>` | `Text` | `text`, `at`, `font_size`, `color` |
///
/// Each actor gets an auto-generated label like `rect_0`, `circle_1`, `path_2`.
pub fn import_svg(path: &Path) -> Result<Vec<Stmt>, SvgImportError> {
    let content = std::fs::read_to_string(path).map_err(SvgImportError::Io)?;
    let doc = Document::parse(&content).map_err(|e| SvgImportError::Xml(e.to_string()))?;

    let root = doc.root_element();
    let mut stmts = Vec::new();
    let mut counter = 0u64;

    // Collect gradient and mask definitions from <defs> elements.
    // <clipPath> is unsupported and intentionally not parsed (see module docs).
    let gradients = collect_gradients(&root);
    let masks = collect_masks(&root);

    // Parse viewBox and width/height for scene configuration
    if let Some(config_stmt) = parse_viewbox_config(&root) {
        stmts.push(config_stmt);
    }

    convert_children(&root, &mut stmts, &mut counter, &Transform::identity(), &gradients, &masks)?;

    Ok(stmts)
}

// viewBox parsing

/// Parse the `viewBox` attribute on a root `<svg>` element and return an
/// optional `@config { size: (width, height) }` statement.
///
/// Falls back to `width` / `height` attributes if `viewBox` is absent.
pub(crate) fn parse_viewbox_config(svg_node: &Node) -> Option<Stmt> {
    // Prefer viewBox; fall back to width/height
    if let Some(vb) = svg_node.attribute("viewBox") {
        let nums: Vec<f64> = parse_numbers(vb);
        if nums.len() >= 4 {
            let w = nums[2];
            let h = nums[3];
            if w > 0.0 && h > 0.0 {
                return Some(Stmt::Config {
                    settings: vec![Property::new(
                        "size",
                        Expr::Tuple(vec![Expr::Num(w), Expr::Num(h)]),
                    )],
                    span: None,
                });
            }
        }
    }

    // Fallback: width and height attributes
    let w = svg_node.attribute("width").and_then(|v| v.parse::<f64>().ok());
    let h = svg_node.attribute("height").and_then(|v| v.parse::<f64>().ok());
    if let (Some(w), Some(h)) = (w, h)
        && w > 0.0
        && h > 0.0
    {
        return Some(Stmt::Config {
            settings: vec![Property::new(
                "size",
                Expr::Tuple(vec![Expr::Num(w), Expr::Num(h)]),
            )],
            span: None,
        });
    }

    None
}

/// Recursively convert SVG child elements into Animatix statements.
pub(crate) fn convert_children(
    parent: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    parent_transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
    masks: &HashMap<String, MaskDef>,
) -> Result<(), SvgImportError> {
    for child in parent.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        let local_transform = parse_transform(child.attribute("transform").unwrap_or(""));
        let combined = parent_transform.compose(&local_transform);

        let first_new_index = stmts.len();

        let is_renderable = match tag {
            "g" => {
                convert_group(&child, stmts, counter, &combined, gradients, masks)?;
                true
            },
            "rect" => {
                convert_rect(&child, stmts, counter, &combined, gradients)?;
                true
            },
            "circle" => {
                convert_circle(&child, stmts, counter, &combined, gradients)?;
                true
            },
            "ellipse" => {
                convert_ellipse(&child, stmts, counter, &combined, gradients)?;
                true
            },
            "path" => {
                convert_path(&child, stmts, counter, &combined, gradients)?;
                true
            },
            "text" => {
                convert_text(&child, stmts, counter, &combined, parent, gradients)?;
                true
            },
            "svg" => {
                convert_children(&child, stmts, counter, &combined, gradients, masks)?;
                true
            },
            "line" => {
                convert_line(&child, stmts, counter, &combined, gradients)?;
                true
            },
            "polyline" | "polygon" => {
                convert_poly(&child, stmts, counter, &combined, tag, gradients)?;
                true
            },
            "use" => {
                convert_use(&child, parent, stmts, counter, &combined, gradients, masks)?;
                true
            },
            "defs" | "mask" => false,
            "clipPath" | "pattern" | "linearGradient" | "radialGradient" | "filter" | "style"
            | "title" | "desc" => false,
            _ => {
                // Unknown element: still recurse in case it has renderable children
                convert_children(&child, stmts, counter, &combined, gradients, masks)?;
                true
            },
        };

        if is_renderable {
            wrap_new_statements_with_mask(&child, stmts, first_new_index, counter, masks);
        }
    }
    Ok(())
}

pub(crate) fn make_label(base: &str, counter: &mut u64) -> String {
    let label = format!("{}_{}", base, *counter);
    *counter += 1;
    label
}

/// Helper to convert nested `Vec<Stmt>` (from imported children) into `Vec<InlineItem>`.
/// Children imported from SVG `<g>` groups are wrapped as anonymous inline items
/// within the Group's children block.
pub(crate) fn children_stmts_to_inline(stmts: &[Stmt]) -> Vec<crate::ast::InlineItem> {
    stmts
        .iter()
        .filter_map(|stmt| {
            if let Stmt::ActorDecl {
                label,
                ty,
                props,
                modifiers,
                children,
                ..
            } = stmt
            {
                // For Group children, use the label as an inline labeled item
                Some(crate::ast::InlineItem::Labeled {
                    label: label.clone(),
                    array_index: None,
                    ty: ty.clone(),
                    props: props.clone(),
                    modifiers: modifiers.clone(),
                    children: children.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Parse an SVG attribute as f64, returning `default` on missing or invalid.
pub(crate) fn attr_float(node: &Node, name: &str, default: f64) -> f64 {
    node.attribute(name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(default)
}
