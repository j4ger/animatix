//! SVG definitions: gradients, clip paths, and masks.

use std::collections::HashMap;

use roxmltree::Node;

use super::attr_float;
use super::shapes::parse_svg_color;
use crate::ast::{Expr, Property, Stmt};
use crate::timeline::path_data::parse_svg_path_data;

// ---------------------------------------------------------------------------
// Gradient definition types (used for url(#id) fill/stroke fallback)
// ---------------------------------------------------------------------------

/// A single color stop in a gradient.
#[derive(Clone, Debug)]
pub(crate) struct GradientStop {
    pub(crate) offset: f64,
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) opacity: f64,
}

/// A parsed SVG clip path definition.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Reserved for future clip-path rendering
pub(crate) struct ClipPathDef {
    /// The path data for clipping.
    pub(crate) path_data: String,
}

/// A parsed SVG mask definition.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Reserved for future mask rendering
pub(crate) struct MaskDef {
    /// The path data for masking.
    pub(crate) path_data: String,
}

/// A parsed SVG gradient definition (linear or radial).
#[derive(Clone, Debug)]
pub(crate) enum GradientDef {
    Linear {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stops: Vec<GradientStop>,
    },
    Radial {
        cx: f64,
        cy: f64,
        r: f64,
        stops: Vec<GradientStop>,
    },
}

impl GradientDef {
    /// The ramp's stops, in document order.
    pub(crate) fn stops(&self) -> &[GradientStop] {
        match self {
            GradientDef::Linear { stops, .. } | GradientDef::Radial { stops, .. } => stops,
        }
    }

    /// Express the definition as a `linear(…)` / `radial(…)` gradient call so
    /// an imported SVG keeps its ramp instead of collapsing to one color.
    ///
    /// SVG's two-point linear form becomes an angle (the ramp is stretched
    /// across the shape's bounding box either way); a single-stop ramp has
    /// nothing to interpolate and returns `None` for the solid fallback.
    pub(crate) fn to_paint_expr(&self) -> Option<Expr> {
        let stops = self.stops();
        if stops.len() < 2 {
            return None;
        }
        let list = Expr::List(
            stops
                .iter()
                .map(|s| {
                    Expr::Tuple(vec![
                        Expr::Num(s.offset),
                        Expr::Tuple(vec![
                            Expr::Num(f64::from(s.r) / 255.0),
                            Expr::Num(f64::from(s.g) / 255.0),
                            Expr::Num(f64::from(s.b) / 255.0),
                            Expr::Num(s.opacity),
                        ]),
                    ])
                })
                .collect(),
        );
        match self {
            GradientDef::Linear { x1, y1, x2, y2, .. } => {
                // CSS convention: 0deg points up, 90deg to the right.
                let angle = (x2 - x1).atan2(-(y2 - y1)).to_degrees();
                Some(Expr::Call("linear".into(), vec![Expr::Num(angle), list]))
            },
            GradientDef::Radial { cx, cy, r, .. } => Some(Expr::Call(
                "radial".into(),
                vec![
                    Expr::Tuple(vec![Expr::Num(*cx), Expr::Num(*cy)]),
                    Expr::Num(*r),
                    list,
                ],
            )),
        }
    }

    /// Approximate this gradient as a single solid RGB color by averaging
    /// stops weighted by their offset spans.
    pub(crate) fn approximate_solid_color(&self) -> (u8, u8, u8) {
        let stops = self.stops();
        if stops.is_empty() {
            return (0, 0, 0);
        }
        if stops.len() == 1 {
            return (stops[0].r, stops[0].g, stops[0].b);
        }

        let mut total_weight = 0.0f64;
        let mut r_acc = 0.0f64;
        let mut g_acc = 0.0f64;
        let mut b_acc = 0.0f64;

        for pair in stops.windows(2) {
            let span = pair[1].offset - pair[0].offset;
            if span <= 0.0 {
                continue;
            }
            let mid_r = (pair[0].r as f64 + pair[1].r as f64) / 2.0;
            let mid_g = (pair[0].g as f64 + pair[1].g as f64) / 2.0;
            let mid_b = (pair[0].b as f64 + pair[1].b as f64) / 2.0;
            r_acc += mid_r * span;
            g_acc += mid_g * span;
            b_acc += mid_b * span;
            total_weight += span;
        }

        if total_weight <= 0.0 {
            // All stops at same offset — use the last stop, if any.
            return stops.last().map(|stop| (stop.r, stop.g, stop.b)).unwrap_or((0, 0, 0));
        }

        (
            (r_acc / total_weight).round().clamp(0.0, 255.0) as u8,
            (g_acc / total_weight).round().clamp(0.0, 255.0) as u8,
            (b_acc / total_weight).round().clamp(0.0, 255.0) as u8,
        )
    }
}

// ---------------------------------------------------------------------------
// Gradient collection from <defs>
// ---------------------------------------------------------------------------

/// Scan the entire SVG tree for `<defs>` elements and collect gradient
/// definitions into a map keyed by their `id` attribute.
pub(crate) fn collect_gradients(node: &Node) -> HashMap<String, GradientDef> {
    let mut gradients = HashMap::new();
    collect_gradients_recursive(node, &mut gradients);
    gradients
}

pub(crate) fn collect_gradients_recursive(
    node: &Node,
    gradients: &mut HashMap<String, GradientDef>,
) {
    for child in node.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        if tag == "defs" {
            collect_gradients_from_defs(&child, gradients);
        }
        // Recurse into all element children to find nested defs
        collect_gradients_recursive(&child, gradients);
    }
}

pub(crate) fn collect_gradients_from_defs(
    defs_node: &Node,
    gradients: &mut HashMap<String, GradientDef>,
) {
    for child in defs_node.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        let id = match child.attribute("id") {
            Some(id) => id.to_string(),
            None => continue,
        };

        match tag {
            "linearGradient" => {
                let stops = parse_gradient_stops(&child);
                if stops.is_empty() {
                    continue;
                }
                let x1 = attr_float(&child, "x1", 0.0);
                let y1 = attr_float(&child, "y1", 0.0);
                let x2 = attr_float(&child, "x2", 1.0);
                let y2 = attr_float(&child, "y2", 0.0);
                gradients.insert(
                    id,
                    GradientDef::Linear {
                        x1,
                        y1,
                        x2,
                        y2,
                        stops,
                    },
                );
            },
            "radialGradient" => {
                let stops = parse_gradient_stops(&child);
                if stops.is_empty() {
                    continue;
                }
                let cx = attr_float(&child, "cx", 0.5);
                let cy = attr_float(&child, "cy", 0.5);
                let r = attr_float(&child, "r", 0.5);
                gradients.insert(id, GradientDef::Radial { cx, cy, r, stops });
            },
            _ => {
                // Recurse into child elements of defs (e.g., nested groups)
                collect_gradients_from_defs(&child, gradients);
            },
        }
    }
}

/// Parse `<stop>` elements inside a gradient definition.
pub(crate) fn parse_gradient_stops(grad_node: &Node) -> Vec<GradientStop> {
    let mut stops = Vec::new();
    for child in grad_node.children() {
        if !child.is_element() {
            continue;
        }
        if child.tag_name().name() != "stop" {
            continue;
        }

        let offset_str = child.attribute("offset").unwrap_or("0");
        let offset = if offset_str.ends_with('%') {
            offset_str.trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)
        } else {
            offset_str.parse::<f64>().ok()
        }
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);

        let color_str = child.attribute("stop-color").unwrap_or("black");
        let opacity = child
            .attribute("stop-opacity")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        let (r, g, b) = parse_stop_color(color_str);
        stops.push(GradientStop {
            offset,
            r,
            g,
            b,
            opacity,
        });
    }
    stops
}

/// Parse a color value for a gradient stop into (r, g, b).
/// Handles hex and named colors; falls back to black.
pub(crate) fn parse_stop_color(value: &str) -> (u8, u8, u8) {
    let value = value.trim();
    // Hex colors
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 3 {
            if let (Some(rc), Some(gc), Some(bc)) =
                (hex.chars().next(), hex.chars().nth(1), hex.chars().nth(2))
            {
                let r = u8::from_str_radix(&format!("{rc}{rc}"), 16).unwrap_or(0);
                let g = u8::from_str_radix(&format!("{gc}{gc}"), 16).unwrap_or(0);
                let b = u8::from_str_radix(&format!("{bc}{bc}"), 16).unwrap_or(0);
                return (r, g, b);
            }
        } else if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return (r, g, b);
            }
        }
        return (0, 0, 0);
    }

    // Named colors (simple subset)
    let named: &[(&str, (u8, u8, u8))] = &[
        ("black", (0, 0, 0)),
        ("white", (255, 255, 255)),
        ("red", (255, 0, 0)),
        ("green", (0, 128, 0)),
        ("blue", (0, 0, 255)),
        ("yellow", (255, 255, 0)),
        ("cyan", (0, 255, 255)),
        ("magenta", (255, 0, 255)),
        ("gray", (128, 128, 128)),
        ("grey", (128, 128, 128)),
        ("orange", (255, 165, 0)),
        ("purple", (128, 0, 128)),
        ("pink", (255, 192, 203)),
        ("brown", (165, 42, 42)),
        ("transparent", (0, 0, 0)),
        ("none", (0, 0, 0)),
    ];
    for (name, rgb) in named {
        if value.eq_ignore_ascii_case(name) {
            return *rgb;
        }
    }

    // Fallback
    (0, 0, 0)
}

/// Collect clipPath definitions from the SVG tree.
pub(crate) fn collect_clip_paths(node: &Node) -> HashMap<String, ClipPathDef> {
    let mut clip_paths = HashMap::new();
    collect_clip_paths_recursive(node, &mut clip_paths);
    clip_paths
}

pub(crate) fn collect_clip_paths_recursive(
    node: &Node,
    clip_paths: &mut HashMap<String, ClipPathDef>,
) {
    for child in node.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        if tag == "clipPath" {
            if let Some(id) = child.attribute("id") {
                // Extract path data from children (path, rect, circle, etc.)
                let path_data = extract_clip_path_data(&child);
                if !path_data.is_empty() {
                    clip_paths.insert(id.to_string(), ClipPathDef { path_data });
                }
            }
        }
        // Recurse into all element children
        collect_clip_paths_recursive(&child, clip_paths);
    }
}

/// Extract path data from clipPath children.
pub(crate) fn extract_clip_path_data(node: &Node) -> String {
    let mut paths = Vec::new();
    for child in node.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        match tag {
            "path" => {
                if let Some(d) = child.attribute("d") {
                    paths.push(d.to_string());
                }
            },
            "rect" => {
                let x = attr_float(&child, "x", 0.0);
                let y = attr_float(&child, "y", 0.0);
                let w = attr_float(&child, "width", 0.0);
                let h = attr_float(&child, "height", 0.0);
                paths.push(format!(
                    "M {} {} L {} {} L {} {} L {} {} Z",
                    x,
                    y,
                    x + w,
                    y,
                    x + w,
                    y + h,
                    x,
                    y + h
                ));
            },
            "circle" => {
                let cx = attr_float(&child, "cx", 0.0);
                let cy = attr_float(&child, "cy", 0.0);
                let r = attr_float(&child, "r", 0.0);
                // Approximate circle with 4 bezier curves
                let k = 0.5522847498; // 4/3 * (sqrt(2) - 1)
                paths.push(format!(
                    "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} Z",
                    cx, cy-r,
                    cx+r*k, cy-r, cx+r, cy-r*k, cx+r, cy,
                    cx+r, cy+r*k, cx+r*k, cy+r, cx, cy+r,
                    cx-r*k, cy+r, cx-r, cy+r*k, cx-r, cy,
                    cx-r, cy-r*k, cx-r*k, cy-r, cx, cy-r
                ));
            },
            _ => {},
        }
    }
    paths.join(" ")
}

/// Apply clip paths to elements that reference them via clip-path attribute.
pub(crate) fn apply_clip_paths(stmts: &mut Vec<Stmt>, _clip_paths: &HashMap<String, ClipPathDef>) {
    // For now, clip paths are applied at the SVG level.
    // The actual clipping is done during rendering via the mask/group mechanism.
    // This is a placeholder for future enhancement.
    let _ = stmts;
}

/// Collect `<mask>` definitions from the SVG tree.
pub(crate) fn collect_masks(node: &Node) -> HashMap<String, MaskDef> {
    let mut masks = HashMap::new();
    collect_masks_recursive(node, &mut masks);
    masks
}

pub(crate) fn collect_masks_recursive(node: &Node, masks: &mut HashMap<String, MaskDef>) {
    for child in node.children() {
        if !child.is_element() {
            continue;
        }
        if child.tag_name().name() == "mask" {
            if let Some(id) = child.attribute("id") {
                let path_data = extract_clip_path_data(&child);
                if !path_data.is_empty() {
                    masks.insert(id.to_string(), MaskDef { path_data });
                }
            }
        }
        collect_masks_recursive(&child, masks);
    }
}

/// Try to resolve a `url(#id)` reference in a fill/stroke attribute.
/// If the id refers to a known gradient, return an approximate solid color.
/// Returns `None` if the value is not a url reference or the gradient is unknown.
pub(crate) fn resolve_gradient_url(
    value: &str,
    gradients: &HashMap<String, GradientDef>,
) -> Option<(u8, u8, u8)> {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix("url(#") {
        let id = if let Some(end) = inner.find(')') {
            &inner[..end]
        } else {
            return None;
        };
        if let Some(grad) = gradients.get(id) {
            Some(grad.approximate_solid_color())
        } else {
            // Gradient referenced but not found in defs — log warning
            tracing::warn!("SVG import: gradient '{}' referenced but not found in <defs>", id);
            None
        }
    } else {
        None
    }
}

/// The `<defs>` id behind a `url(#id)` paint reference, if that is what `value` is.
pub(crate) fn gradient_ref_id(value: &str) -> Option<&str> {
    let inner = value.trim().strip_prefix("url(#")?;
    Some(&inner[..inner.find(')')?])
}

/// Resolve a `url(#id)` paint to a real gradient call expression.
///
/// Returns `None` for non-url values and for ramps too short to interpolate,
/// which then fall through to the averaged-solid path.
pub(crate) fn svg_gradient_paint(
    value: &str,
    gradients: &HashMap<String, GradientDef>,
) -> Option<Expr> {
    gradients.get(gradient_ref_id(value)?)?.to_paint_expr()
}

/// Parse an SVG color value, with support for `url(#...)` gradient references.
///
/// If the value is a `url(#id)` and the gradient is found in `gradients`,
/// the gradient is approximated as a solid color (average of stops). A warning
/// is emitted via tracing.
///
/// Returns `None` for `none`/`transparent`, or for unknown/unresolvable urls.
pub(crate) fn parse_svg_color_with_gradients(
    value: &str,
    gradients: &HashMap<String, GradientDef>,
    current_color: Option<&Expr>,
) -> Option<Expr> {
    let value = value.trim();

    // Check for url(#id) gradient reference
    if value.starts_with("url(#") {
        if let Some((r, g, b)) = resolve_gradient_url(value, gradients) {
            tracing::warn!(
                "SVG import: gradient '{}' approximated as solid rgb({}, {}, {})",
                &value[5..value.find(')').unwrap_or(value.len() - 1)],
                r,
                g,
                b
            );
            return Some(Expr::Call(
                "rgb".into(),
                vec![
                    Expr::Num(r as f64),
                    Expr::Num(g as f64),
                    Expr::Num(b as f64),
                ],
            ));
        }
        // Unknown or unresolvable gradient reference — return None (transparent)
        return None;
    }

    // Fall through to standard color parsing
    parse_svg_color(value, current_color)
}

/// Resolve a `url(#id)` reference to just the id string. Returns None for
/// non-url values or invalid references.
pub(crate) fn resolve_url_id(value: &str) -> Option<&str> {
    let value = value.trim();
    let inner = value.strip_prefix("url(")?.strip_suffix(')')?.trim();
    let inner = inner.trim_matches(|ch| ch == '"' || ch == '\'');
    inner.strip_prefix('#').filter(|id| !id.is_empty())
}

/// Wrap statements generated for a node in a `Mask` actor if the node has
/// a `mask="url(#id)"` attribute.
pub(crate) fn wrap_new_statements_with_mask(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    first_new_index: usize,
    counter: &mut u64,
    masks: &HashMap<String, MaskDef>,
) {
    let Some(mask_attr) = node.attribute("mask") else {
        return;
    };
    let Some(mask_id) = resolve_url_id(mask_attr) else {
        tracing::warn!("SVG import: unsupported mask reference '{}'", mask_attr);
        return;
    };
    let Some(mask) = masks.get(mask_id) else {
        tracing::warn!("SVG import: mask '{}' referenced but not found in <defs>", mask_id);
        return;
    };

    if first_new_index >= stmts.len() {
        return;
    }

    // Split off the generated statements for this element
    let content_stmts = stmts.split_off(first_new_index);
    let content_items = super::children_stmts_to_inline(&content_stmts);
    if content_items.is_empty() {
        stmts.extend(content_stmts);
        return;
    }

    // Create the mask shape actor (invisible, defines clip geometry)
    let mask_label = super::make_label("mask_shape", counter);
    let mask_shape = crate::ast::InlineItem::Labeled {
        label: mask_label,
        array_index: None,
        ty: "Path".into(),
        props: vec![
            Property::new("commands", parse_svg_path_data(&mask.path_data)),
            Property::new("at", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            Property::new("color", Expr::Ident("white".into())),
            Property::new("opacity", Expr::Num(0.0)),
        ],
        modifiers: Vec::new(),
        children: Vec::new(),
    };

    // Build the Mask wrapper: first child = mask shape, rest = content
    let mut children = Vec::with_capacity(content_items.len() + 1);
    children.push(mask_shape);
    children.extend(content_items);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: super::make_label("mask", counter),
        array_index: None,
        ty: "Mask".into(),
        props: Vec::new(),
        modifiers: Vec::new(),
        children,
        span: None,
    });
}
