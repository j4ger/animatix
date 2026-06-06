//! # SVG Import
//!
//! Imports SVG files as static trees of Animatix AST statements.
//! Supports `<g>`, `<rect>`, `<circle>`, `<ellipse>`, `<path>`, `<text>` elements
//! with basic `transform`, `fill`, `stroke`, and `opacity` handling.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use animatix::timeline::import_svg;
//! let stmts = import_svg(std::path::Path::new("icon.svg")).unwrap();
//! // stmts can be inserted into a scene or compiled through the normal pipeline
//! ```
//!
//! ## Limitations / TODOs
//!
//! - No support for SVG `<use>`, `<clipPath>`, `<mask>`, patterns
//! - SVG `<path>` `d` attribute: supports M, L, Q, C, Z commands (absolute/relative)
//! - SVG `currentColor`, `inherit` fill types: not yet supported
//! - SVG `stroke-linecap`, `stroke-linejoin`: not yet mapped

use std::collections::HashMap;
use std::path::Path;

use roxmltree::{Document, Node};

use crate::ast::{Expr, Property, Stmt};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during SVG import.
#[derive(Debug)]
pub enum SvgImportError {
    /// I/O error reading the file.
    Io(std::io::Error),
    /// XML parse error.
    Xml(String),
    /// An SVG element type is not (yet) supported.
    UnsupportedElement(String),
}

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

// ---------------------------------------------------------------------------
// Gradient definition types (used for url(#id) fill/stroke fallback)
// ---------------------------------------------------------------------------

/// A single color stop in a gradient.
///
/// `opacity` is parsed but not used because the importer approximates
/// gradients as a single solid color (see [`GradientDef::approximate_solid_color`]).
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct GradientStop {
    offset: f64,
    r: u8,
    g: u8,
    b: u8,
    opacity: f64,
}

/// A parsed SVG gradient definition (linear or radial).
///
/// Geometric fields (`x1`, `y1`, …) are parsed for completeness but not used
/// because the importer flattens gradients to a single averaged color.
#[derive(Clone, Debug)]
#[allow(dead_code)]
enum GradientDef {
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
    /// Approximate this gradient as a single solid RGB color by averaging
    /// stops weighted by their offset spans.
    fn approximate_solid_color(&self) -> (u8, u8, u8) {
        let stops = match self {
            GradientDef::Linear { stops, .. } => stops,
            GradientDef::Radial { stops, .. } => stops,
        };
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
            // All stops at same offset — use the last stop
            let last = stops.last().unwrap();
            return (last.r, last.g, last.b);
        }

        (
            (r_acc / total_weight).round().clamp(0.0, 255.0) as u8,
            (g_acc / total_weight).round().clamp(0.0, 255.0) as u8,
            (b_acc / total_weight).round().clamp(0.0, 255.0) as u8,
        )
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

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

    // Collect gradient definitions from <defs> elements
    let gradients = collect_gradients(&root);

    // Parse viewBox and width/height for scene configuration
    if let Some(config_stmt) = parse_viewbox_config(&root) {
        stmts.push(config_stmt);
    }

    convert_children(&root, &mut stmts, &mut counter, &Transform::identity(), &gradients)?;
    Ok(stmts)
}

// ---------------------------------------------------------------------------
// Transform representation (2D affine)
// ---------------------------------------------------------------------------

/// A parsed 2D transform from an SVG `transform` attribute.
#[derive(Clone, Debug, Default)]
struct Transform {
    tx: f64,
    ty: f64,
    rotation_deg: f64,
    sx: f64,
    sy: f64,
}

impl Transform {
    fn identity() -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            rotation_deg: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }

    /// Compose `self * other` (other is applied first in SVG convention,
    /// but we treat transforms as building from parent to child).
    fn compose(&self, other: &Transform) -> Self {
        // We apply child's local transform and then parent's accumulated transform.
        // For simplicity, we add translation offsets and multiply scales.
        // Rotation composition is more complex; for commonly-separate cases this works.
        Self {
            tx: self.tx + other.tx * self.sx,
            ty: self.ty + other.ty * self.sy,
            rotation_deg: self.rotation_deg + other.rotation_deg,
            sx: self.sx * other.sx,
            sy: self.sy * other.sy,
        }
    }
}

/// Parse an SVG `transform` attribute string into a `Transform`.
fn parse_transform(value: &str) -> Transform {
    let mut result = Transform::identity();
    // Match translate(…), rotate(…), scale(…), matrix(…)
    let mut remaining = value.trim();
    while !remaining.is_empty() {
        remaining = remaining.trim();
        if let Some(rest) = parse_single_transform(remaining, &mut result) {
            remaining = rest;
        } else {
            break; // stop on parse failure; gracefully degrade
        }
    }
    result
}

fn parse_single_transform<'a>(s: &'a str, t: &mut Transform) -> Option<&'a str> {
    let s = s.trim_start();
    if s.starts_with("translate(") {
        let (args, rest) = parenthesized_args(s, "translate(")?;
        let nums: Vec<f64> = parse_numbers(args);
        if nums.len() >= 2 {
            t.tx += nums[0];
            t.ty += nums[1];
        } else if nums.len() == 1 {
            t.tx += nums[0];
        }
        Some(rest)
    } else if s.starts_with("rotate(") {
        let (args, rest) = parenthesized_args(s, "rotate(")?;
        let nums: Vec<f64> = parse_numbers(args);
        if !nums.is_empty() {
            t.rotation_deg += nums[0];
        }
        Some(rest)
    } else if s.starts_with("scale(") {
        let (args, rest) = parenthesized_args(s, "scale(")?;
        let nums: Vec<f64> = parse_numbers(args);
        if nums.len() >= 2 {
            t.sx *= nums[0];
            t.sy *= nums[1];
        } else if nums.len() == 1 {
            t.sx *= nums[0];
            t.sy *= nums[0];
        }
        Some(rest)
    } else if s.starts_with("matrix(") {
        let (args, rest) = parenthesized_args(s, "matrix(")?;
        let nums: Vec<f64> = parse_numbers(args);
        if nums.len() >= 6 {
            // matrix(a, b, c, d, e, f) = affine transform
            // For simplicity, extract translate(e, f) and a rough scale/rotation
            t.tx += nums[4];
            t.ty += nums[5];
            let sx = (nums[0] * nums[0] + nums[2] * nums[2]).sqrt();
            let sy = (nums[1] * nums[1] + nums[3] * nums[3]).sqrt();
            if sx > 0.0 {
                t.sx *= sx;
            }
            if sy > 0.0 {
                t.sy *= sy;
            }
            // Rough rotation estimate from first column
            if sx > 0.0 && nums[0].abs() > 1e-10 {
                let angle = nums[2].atan2(nums[0]).to_degrees();
                t.rotation_deg += angle;
            }
        }
        Some(rest)
    } else {
        None
    }
}

/// Extract content between parentheses after a function name prefix.
fn parenthesized_args<'a>(s: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = s.strip_prefix(prefix)?;
    let mut depth = 1i32;
    let mut end = 0;
    for (i, ch) in rest.char_indices() {
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                end = i;
                break;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    Some((&rest[..end], &rest[end + 1..]))
}

/// Parse comma/whitespace-separated numbers from a string.
fn parse_numbers(s: &str) -> Vec<f64> {
    s.split([',', ' ', '\t', '\n', '\r'])
        .filter_map(|tok| {
            let tok = tok.trim();
            if tok.is_empty() {
                None
            } else {
                tok.parse::<f64>().ok()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// viewBox parsing
// ---------------------------------------------------------------------------

/// Parse the `viewBox` attribute on a root `<svg>` element and return an
/// optional `@config { size: (width, height) }` statement.
///
/// Falls back to `width` / `height` attributes if `viewBox` is absent.
fn parse_viewbox_config(svg_node: &Node) -> Option<Stmt> {
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
    if let (Some(w), Some(h)) = (w, h) {
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

    None
}

// ---------------------------------------------------------------------------
// Gradient collection from <defs>
// ---------------------------------------------------------------------------

/// Scan the entire SVG tree for `<defs>` elements and collect gradient
/// definitions into a map keyed by their `id` attribute.
fn collect_gradients(node: &Node) -> HashMap<String, GradientDef> {
    let mut gradients = HashMap::new();
    collect_gradients_recursive(node, &mut gradients);
    gradients
}

fn collect_gradients_recursive(node: &Node, gradients: &mut HashMap<String, GradientDef>) {
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

fn collect_gradients_from_defs(defs_node: &Node, gradients: &mut HashMap<String, GradientDef>) {
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
            }
            "radialGradient" => {
                let stops = parse_gradient_stops(&child);
                if stops.is_empty() {
                    continue;
                }
                let cx = attr_float(&child, "cx", 0.5);
                let cy = attr_float(&child, "cy", 0.5);
                let r = attr_float(&child, "r", 0.5);
                gradients.insert(
                    id,
                    GradientDef::Radial { cx, cy, r, stops },
                );
            }
            _ => {
                // Recurse into child elements of defs (e.g., nested groups)
                collect_gradients_from_defs(&child, gradients);
            }
        }
    }
}

/// Parse `<stop>` elements inside a gradient definition.
fn parse_gradient_stops(grad_node: &Node) -> Vec<GradientStop> {
    let mut stops = Vec::new();
    for child in grad_node.children() {
        if !child.is_element() {
            continue;
        }
        if child.tag_name().name() != "stop" {
            continue;
        }

        let offset_str = child
            .attribute("offset")
            .unwrap_or("0");
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
fn parse_stop_color(value: &str) -> (u8, u8, u8) {
    let value = value.trim();
    // Hex colors
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 3 {
            if let (Some(rc), Some(gc), Some(bc)) = (hex.chars().next(), hex.chars().nth(1), hex.chars().nth(2)) {
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

/// Try to resolve a `url(#id)` reference in a fill/stroke attribute.
/// If the id refers to a known gradient, return an approximate solid color.
/// Returns `None` if the value is not a url reference or the gradient is unknown.
fn resolve_gradient_url(value: &str, gradients: &HashMap<String, GradientDef>) -> Option<(u8, u8, u8)> {
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
            tracing::warn!(
                "SVG import: gradient '{}' referenced but not found in <defs>",
                id
            );
            None
        }
    } else {
        None
    }
}

/// Parse SVG color/fill/stroke value into an Animatix property expression.
///
/// Returns `None` for `none` (transparent), or an `Expr` usable as a color value.
/// `current_color` is the resolved color from the parent element's `color` attribute,
/// used when the value is `currentColor`.
fn parse_svg_color(value: &str, current_color: Option<&Expr>) -> Option<Expr> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return None; // caller can treat this as transparent / omit fill
    }

    // currentColor — resolve to parent element's color attribute
    if value.eq_ignore_ascii_case("currentColor") {
        return current_color.cloned();
    }

    // Named colors → pass through as identifier
    if !value.starts_with('#') && !value.starts_with("rgb") && !value.starts_with("hsl") {
        // Use the color name directly as an identifier (e.g., `red`, `blue`)
        // Animatix resolves named colors via its color scheme or CSS named colors.
        return Some(Expr::Ident(value.to_lowercase()));
    }

    // Hex colors: #fff or #ffffff
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    // rgb(r, g, b) or rgba(r, g, b, a)
    if value.starts_with("rgba(") || value.starts_with("rgb(") {
        let is_rgba = value.starts_with("rgba(");
        let prefix = if is_rgba { "rgba(" } else { "rgb(" };
        let (inner, _) = parenthesized_args(value, prefix)?;
        let nums: Vec<f64> = parse_numbers(inner);
        if nums.len() >= 3 {
            let r = nums[0] as u8;
            let g = nums[1] as u8;
            let b = nums[2] as u8;
            // Return as rgb(r, g, b) call expression for Animatix to resolve
            return Some(Expr::Call(
                "rgb".into(),
                vec![
                    Expr::Num(r as f64),
                    Expr::Num(g as f64),
                    Expr::Num(b as f64),
                ],
            ));
        }
    }

    // Fallback: pass through as identifier
    Some(Expr::Ident(value.to_lowercase()))
}

fn parse_hex_color(hex: &str) -> Option<Expr> {
    let hex = hex.trim();
    let rgb = if hex.len() == 3 {
        // Short form #RGB → #RRGGBB
        let r = hex.chars().next()?;
        let g = hex.chars().nth(1)?;
        let b = hex.chars().nth(2)?;
        u32::from_str_radix(
            &format!("{r}{r}{g}{g}{b}{b}"),
            16,
        )
        .ok()
    } else if hex.len() == 6 {
        u32::from_str_radix(hex, 16).ok()
    } else {
        return None;
    }?;

    let r = ((rgb >> 16) & 0xFF) as f64;
    let g = ((rgb >> 8) & 0xFF) as f64;
    let b = (rgb & 0xFF) as f64;

    Some(Expr::Call(
        "rgb".into(),
        vec![Expr::Num(r), Expr::Num(g), Expr::Num(b)],
    ))
}

// ---------------------------------------------------------------------------
// SVG path data (d attribute) parsing
// ---------------------------------------------------------------------------

/// Parse an SVG `d` attribute into Animatix path command expressions.
///
/// Supports absolute and relative commands: M/m, L/l, Q/q, C/c, Z/z.
/// Returns a tuple of commands: `(move_to(...), line_to(...), ...)`
fn parse_svg_path_data(d: &str) -> Expr {
    let tokens = tokenize_path_data(d);
    let mut commands: Vec<Expr> = Vec::new();
    let mut i = 0;
    let mut current_pos = (0.0, 0.0); // for relative commands

    while i < tokens.len() {
        let cmd = tokens[i].as_str();
        i += 1;

        match cmd {
            "M" | "m" => {
                let abs = cmd == "M";
                while i + 1 < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    let y = parse_token_num(&tokens[i + 1]);
                    i += 2;
                    let (abs_x, abs_y) = if abs {
                        (x, y)
                    } else {
                        (current_pos.0 + x, current_pos.1 + y)
                    };
                    commands.push(Expr::Call(
                        "move_to".into(),
                        vec![Expr::Num(abs_x), Expr::Num(abs_y)],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            }
            "L" | "l" => {
                let abs = cmd == "L";
                while i + 1 < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    let y = parse_token_num(&tokens[i + 1]);
                    i += 2;
                    let (abs_x, abs_y) = if abs {
                        (x, y)
                    } else {
                        (current_pos.0 + x, current_pos.1 + y)
                    };
                    commands.push(Expr::Call(
                        "line_to".into(),
                        vec![Expr::Num(abs_x), Expr::Num(abs_y)],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            }
            "Q" | "q" => {
                let abs = cmd == "Q";
                while i + 3 < tokens.len() && is_number(&tokens[i]) {
                    let x1 = parse_token_num(&tokens[i]);
                    let y1 = parse_token_num(&tokens[i + 1]);
                    let x = parse_token_num(&tokens[i + 2]);
                    let y = parse_token_num(&tokens[i + 3]);
                    i += 4;
                    let (abs_x1, abs_y1, abs_x, abs_y) = if abs {
                        (x1, y1, x, y)
                    } else {
                        (
                            current_pos.0 + x1,
                            current_pos.1 + y1,
                            current_pos.0 + x,
                            current_pos.1 + y,
                        )
                    };
                    commands.push(Expr::Call(
                        "quad_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            }
            "C" | "c" => {
                let abs = cmd == "C";
                while i + 5 < tokens.len() && is_number(&tokens[i]) {
                    let x1 = parse_token_num(&tokens[i]);
                    let y1 = parse_token_num(&tokens[i + 1]);
                    let x2 = parse_token_num(&tokens[i + 2]);
                    let y2 = parse_token_num(&tokens[i + 3]);
                    let x = parse_token_num(&tokens[i + 4]);
                    let y = parse_token_num(&tokens[i + 5]);
                    i += 6;
                    let (abs_x1, abs_y1, abs_x2, abs_y2, abs_x, abs_y) = if abs {
                        (x1, y1, x2, y2, x, y)
                    } else {
                        (
                            current_pos.0 + x1,
                            current_pos.1 + y1,
                            current_pos.0 + x2,
                            current_pos.1 + y2,
                            current_pos.0 + x,
                            current_pos.1 + y,
                        )
                    };
                    commands.push(Expr::Call(
                        "curve_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x2),
                            Expr::Num(abs_y2),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            }
            "Z" | "z" => {
                commands.push(Expr::Call("close".into(), vec![]));
            }
            _ => {
                // Unknown command — skip and continue parsing rest of path
                continue;
            }
        }
    }

    Expr::Tuple(commands)
}

/// Tokenize an SVG path `d` attribute into command letters and numbers.
fn tokenize_path_data(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut is_prev_num = false;

    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
            is_prev_num = false;
        } else if ch == '-' || ch == '+' || ch.is_ascii_digit() || ch == '.' {
            if !is_prev_num && !current.is_empty() {
                // e.g., "5-3" → push "5" then start "-3"
                // This handles minus signs in scientific notation vs. negation
                // Actually, in SVG path data, a minus sign starts a new number
                // after a completed one.
                // Check: if previous char was a digit and current is '-', it's a new number
                if ch == '-' && current.chars().last().is_some_and(|c| c.is_ascii_digit()) {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            current.push(ch);
            is_prev_num = true;
        } else if ch == ',' || ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            is_prev_num = false;
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn is_number(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

fn parse_token_num(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Element conversion
// ---------------------------------------------------------------------------

/// Recursively convert SVG child elements into Animatix statements.
fn convert_children(
    parent: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    parent_transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    for child in parent.children() {
        if !child.is_element() {
            continue;
        }
        let tag = child.tag_name().name();
        let local_transform = parse_transform(
            child.attribute("transform").unwrap_or(""),
        );
        let combined = parent_transform.compose(&local_transform);

        match tag {
            "g" => convert_group(&child, stmts, counter, &combined, gradients)?,
            "rect" => convert_rect(&child, stmts, counter, &combined, gradients)?,
            "circle" => convert_circle(&child, stmts, counter, &combined, gradients)?,
            "ellipse" => convert_ellipse(&child, stmts, counter, &combined, gradients)?,
            "path" => convert_path(&child, stmts, counter, &combined, gradients)?,
            "text" => convert_text(&child, stmts, counter, &combined, parent, gradients)?,
            "svg" => convert_children(&child, stmts, counter, &combined, gradients)?, // recurse into svg roots
            "line" => convert_line(&child, stmts, counter, &combined, gradients)?,
            "polyline" | "polygon" => {
                convert_poly(&child, stmts, counter, &combined, tag, gradients)?;
            }
            "defs" => {
                // <defs> contents (gradients etc.) were already collected;
                // skip silently — nothing to render directly.
            }
            "clipPath" | "mask" | "pattern" | "linearGradient"
            | "radialGradient" | "filter" | "style" | "title" | "desc" => {
                // Silently skip non-rendering elements
            }
            _ => {
                // Unknown element: still recurse in case it has renderable children
                convert_children(&child, stmts, counter, &combined, gradients)?;
            }
        }
    }
    Ok(())
}

fn make_label(base: &str, counter: &mut u64) -> String {
    let label = format!("{}_{}", base, *counter);
    *counter += 1;
    label
}

fn convert_group(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("group", counter);
    let mut props = transform_to_props(transform);
    // Opacity
    if let Some(opacity_str) = node.attribute("opacity") {
        if let Ok(opacity) = opacity_str.parse::<f64>() {
            props.push(Property::new("opacity", Expr::Num(opacity)));
        }
    }

    let mut children_stmts = Vec::new();
    convert_children(node, &mut children_stmts, counter, &Transform::identity(), gradients)?;

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Group".into(),
        props,
        modifiers: Vec::new(),
        children: children_stmts_to_inline(&children_stmts),
        span: None,
    });
    Ok(())
}

fn convert_rect(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("rect", counter);
    let x = attr_float(node, "x", 0.0);
    let y = attr_float(node, "y", 0.0);
    let w = attr_float(node, "width", 0.0);
    let h = attr_float(node, "height", 0.0);

    let center_x = x + w / 2.0 + transform.tx;
    let center_y = y + h / 2.0 + transform.ty;

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![Expr::Num(w), Expr::Num(h)])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(center_x), Expr::Num(center_y)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        // Aspect-preserving: use uniform scale that matches x
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Rect".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_circle(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("circle", counter);
    let cx = attr_float(node, "cx", 0.0) + transform.tx;
    let cy = attr_float(node, "cy", 0.0) + transform.ty;
    let r = attr_float(node, "r", 0.0);
    let diameter = r * 2.0;

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![Expr::Num(diameter), Expr::Num(diameter)])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Ellipse".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_ellipse(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("ellipse", counter);
    let cx = attr_float(node, "cx", 0.0) + transform.tx;
    let cy = attr_float(node, "cy", 0.0) + transform.ty;
    let rx = attr_float(node, "rx", 0.0);
    let ry = attr_float(node, "ry", 0.0);

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![
            Expr::Num(rx * 2.0),
            Expr::Num(ry * 2.0),
        ])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Ellipse".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_path(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("path", counter);

    let d = node.attribute("d").unwrap_or("");
    let commands = parse_svg_path_data(d);

    let mut props = vec![
        Property::new("commands", commands),
        Property::new("at", Expr::Tuple(vec![
            Expr::Num(transform.tx),
            Expr::Num(transform.ty),
        ])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Path".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_line(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("line", counter);
    let x1 = attr_float(node, "x1", 0.0) + transform.tx;
    let y1 = attr_float(node, "y1", 0.0) + transform.ty;
    let x2 = attr_float(node, "x2", 0.0) + transform.tx;
    let y2 = attr_float(node, "y2", 0.0) + transform.ty;

    let cx = (x1 + x2) / 2.0;
    let cy = (y1 + y2) / 2.0;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt();

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![
            Expr::Num(length / 2.0),
            Expr::Num(1.0), // minimal height
        ])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    // Rotation to align with direction
    if length > 0.0 {
        let angle = dy.atan2(dx).to_degrees();
        props.push(Property::new("rotation", Expr::Num(angle)));
    }

    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Line".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_poly(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    tag: &str,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label(tag, counter);

    let points_str = node.attribute("points").unwrap_or("");
    let nums: Vec<f64> = parse_numbers(points_str);
    let mut commands = Vec::new();
    let mut first = true;
    let mut i = 0;
    while i + 1 < nums.len() {
        let x = nums[i] + transform.tx;
        let y = nums[i + 1] + transform.ty;
        if first {
            commands.push(Expr::Call("move_to".into(), vec![Expr::Num(x), Expr::Num(y)]));
            first = false;
        } else {
            commands.push(Expr::Call("line_to".into(), vec![Expr::Num(x), Expr::Num(y)]));
        }
        i += 2;
    }
    if tag == "polygon" && !first {
        commands.push(Expr::Call("close".into(), vec![]));
    }

    let mut props = vec![
        Property::new("commands", Expr::Tuple(commands)),
        Property::new("at", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Path".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

fn convert_text(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    _parent: &Node,
    gradients: &HashMap<String, GradientDef>,
) -> Result<(), SvgImportError> {
    let label = make_label("text", counter);
    let x = attr_float(node, "x", 0.0) + transform.tx;
    let y = attr_float(node, "y", 0.0) + transform.ty;
    let content = node.text().unwrap_or("").trim().to_string();
    let font_size = attr_float(node, "font-size", 16.0);

    let mut props = vec![
        Property::new("text", Expr::Str(content)),
        Property::new("at", Expr::Tuple(vec![Expr::Num(x), Expr::Num(y)])),
        Property::new("font_size", Expr::Num(font_size)),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }

    add_fill_stroke_props(node, &mut props, gradients);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        ty: "Text".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Property helpers
// ---------------------------------------------------------------------------

/// Extract position/rotation/scale from a transform into `Property` items.
fn transform_to_props(t: &Transform) -> Vec<Property> {
    let mut props = Vec::new();
    if t.tx != 0.0 || t.ty != 0.0 {
        props.push(Property::new(
            "at",
            Expr::Tuple(vec![Expr::Num(t.tx), Expr::Num(t.ty)]),
        ));
    }
    if t.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(t.rotation_deg)));
    }
    if (t.sx - 1.0).abs() > 1e-6 || (t.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (t.sx + t.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale)));
    }
    props
}

/// Add `fill`, `stroke`, `stroke-width`, `opacity` properties from SVG attributes.
/// Also handles `stroke-dasharray` parsing and `url(#...)` gradient references.
fn add_fill_stroke_props(node: &Node, props: &mut Vec<Property>, gradients: &HashMap<String, GradientDef>) {
    // Resolve parent color for currentColor support
    let current_color: Option<Expr> = node.attribute("color")
        .and_then(|c| parse_svg_color(c, None));

    // Fill color — handles hex, named, rgb(), and url(#id) gradient references
    if let Some(fill) = node.attribute("fill") {
        if let Some(color_expr) = parse_svg_color_with_gradients(fill, gradients, current_color.as_ref()) {
            props.push(Property::new("color", color_expr));
        } else {
            // fill="none" → transparent; fill="url(...)" with unknown gradient → transparent
            props.push(Property::new("fill_opacity", Expr::Num(0.0)));
        }
    }

    // Fill opacity
    if let Some(fill_opacity) = node.attribute("fill-opacity") {
        if let Ok(op) = fill_opacity.parse::<f64>() {
            props.push(Property::new("fill_opacity", Expr::Num(op)));
        }
    }

    // Stroke color — same as fill, supports url(#id)
    if let Some(stroke) = node.attribute("stroke") {
        if let Some(color_expr) = parse_svg_color_with_gradients(stroke, gradients, current_color.as_ref()) {
            props.push(Property::new("stroke_color", color_expr));
        }
    }

    // Stroke width
    if let Some(stroke_width) = node.attribute("stroke-width") {
        if let Ok(w) = stroke_width.parse::<f64>() {
            props.push(Property::new("stroke_width", Expr::Num(w)));
        }
    }

    // Stroke opacity
    if let Some(stroke_opacity) = node.attribute("stroke-opacity") {
        if let Ok(op) = stroke_opacity.parse::<f64>() {
            props.push(Property::new("stroke_opacity", Expr::Num(op)));
        }
    }

    // stroke-dasharray: parse and store as custom property; warn not rendered
    if let Some(dasharray) = node.attribute("stroke-dasharray") {
        let values: Vec<f64> = parse_numbers(dasharray);
        if !values.is_empty() {
            let dash_expr = Expr::Tuple(values.into_iter().map(Expr::Num).collect());
            props.push(Property::new("stroke_dasharray", dash_expr));
        }
    }

    // Overall opacity
    if let Some(opacity) = node.attribute("opacity") {
        if let Ok(op) = opacity.parse::<f64>() {
            props.push(Property::new("opacity", Expr::Num(op)));
        }
    }
}

/// Parse an SVG color value, with support for `url(#...)` gradient references.
///
/// If the value is a `url(#id)` and the gradient is found in `gradients`,
/// the gradient is approximated as a solid color (average of stops). A warning
/// is emitted via tracing.
///
/// Returns `None` for `none`/`transparent`, or for unknown/unresolvable urls.
fn parse_svg_color_with_gradients(value: &str, gradients: &HashMap<String, GradientDef>, current_color: Option<&Expr>) -> Option<Expr> {
    let value = value.trim();

    // Check for url(#id) gradient reference
    if value.starts_with("url(#") {
        if let Some((r, g, b)) = resolve_gradient_url(value, gradients) {
            tracing::warn!(
                "SVG import: gradient '{}' approximated as solid rgb({}, {}, {})",
                &value[5..value.find(')').unwrap_or(value.len() - 1)],
                r, g, b
            );
            return Some(Expr::Call(
                "rgb".into(),
                vec![Expr::Num(r as f64), Expr::Num(g as f64), Expr::Num(b as f64)],
            ));
        }
        // Unknown or unresolvable gradient reference — return None (transparent)
        return None;
    }

    // Fall through to standard color parsing
    parse_svg_color(value, current_color)
}

/// Helper to convert nested `Vec<Stmt>` (from imported children) into `Vec<InlineItem>`.
/// Children imported from SVG `<g>` groups are wrapped as anonymous inline items
/// within the Group's children block.
fn children_stmts_to_inline(stmts: &[Stmt]) -> Vec<crate::ast::InlineItem> {
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
fn attr_float(node: &Node, name: &str, default: f64) -> f64 {
    node.attribute(name)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        let expr = parse_svg_color("#ff0000", None).unwrap();
        assert_eq!(
            expr,
            Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
        );
    }

    #[test]
    fn test_parse_short_hex_color() {
        let expr = parse_svg_color("#f00", None).unwrap();
        assert_eq!(
            expr,
            Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
        );
    }

    #[test]
    fn test_parse_named_color() {
        let expr = parse_svg_color("red", None).unwrap();
        assert_eq!(expr, Expr::Ident("red".into()));
    }

    #[test]
    fn test_parse_none_color() {
        let expr = parse_svg_color("none", None);
        assert!(expr.is_none());
    }

    #[test]
    fn test_parse_rgb_color() {
        let expr = parse_svg_color("rgb(255, 0, 0)", None).unwrap();
        assert_eq!(
            expr,
            Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
        );
    }

    #[test]
    fn test_parse_transform_translate() {
        let t = parse_transform("translate(10, 20)");
        assert!((t.tx - 10.0).abs() < 1e-6);
        assert!((t.ty - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_transform_rotate() {
        let t = parse_transform("rotate(45)");
        assert!((t.rotation_deg - 45.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_transform_scale() {
        let t = parse_transform("scale(2)");
        assert!((t.sx - 2.0).abs() < 1e-6);
        assert!((t.sy - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_transform_combined() {
        let t = parse_transform("translate(5, 10) rotate(30) scale(1.5)");
        assert!((t.tx - 5.0).abs() < 1e-6);
        assert!((t.ty - 10.0).abs() < 1e-6);
        assert!((t.rotation_deg - 30.0).abs() < 1e-6);
        assert!((t.sx - 1.5).abs() < 1e-6);
        assert!((t.sy - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_tokenize_path_data() {
        let tokens = tokenize_path_data("M10 20 L30 40");
        assert_eq!(tokens, vec!["M", "10", "20", "L", "30", "40"]);
    }

    #[test]
    fn test_parse_svg_path_simple() {
        let expr = parse_svg_path_data("M10 20 L30 40 Z");
        let expected = Expr::Tuple(vec![
            Expr::Call("move_to".into(), vec![Expr::Num(10.0), Expr::Num(20.0)]),
            Expr::Call("line_to".into(), vec![Expr::Num(30.0), Expr::Num(40.0)]),
            Expr::Call("close".into(), vec![]),
        ]);
        assert_eq!(expr, expected);
    }

    #[test]
    fn test_parse_svg_path_curve() {
        let expr = parse_svg_path_data("M0 0 C10 10 20 20 30 0 Z");
        let expected = Expr::Tuple(vec![
            Expr::Call("move_to".into(), vec![Expr::Num(0.0), Expr::Num(0.0)]),
            Expr::Call("curve_to".into(), vec![
                Expr::Num(10.0), Expr::Num(10.0),
                Expr::Num(20.0), Expr::Num(20.0),
                Expr::Num(30.0), Expr::Num(0.0),
            ]),
            Expr::Call("close".into(), vec![]),
        ]);
        assert_eq!(expr, expected);
    }

    #[test]
    fn test_import_simple_svg_file() {
        // Create a temporary SVG file
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="10" y="10" width="50" height="30" fill="red"/>
  <circle cx="70" cy="50" r="20" fill="blue"/>
  <g transform="translate(10, 0)">
    <rect x="0" y="0" width="20" height="20" fill="green"/>
  </g>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_import_simple.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // First statement should be a @config from width/height attributes
        assert_eq!(stmts.len(), 4, "expected 4 root statements (config + 3 actors), got {}", stmts.len());
        if let Stmt::Config { settings, .. } = &stmts[0] {
            assert_eq!(settings.len(), 1);
            assert_eq!(settings[0].name, "size");
            assert_eq!(settings[0].value, Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]));
        } else {
            panic!("Expected Config as first statement");
        }

        // Next should be a Rect
        if let Stmt::ActorDecl { label, ty, props, .. } = &stmts[1] {
            assert_eq!(label, "rect_0");
            assert_eq!(ty, "Rect");
            // size should be (50, 30)
            let size_prop = props.iter().find(|p| p.name == "size").unwrap();
            assert_eq!(size_prop.value, Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(30.0)]));
            // at should be centered: (10 + 25, 10 + 15) = (35, 25)
            let at_prop = props.iter().find(|p| p.name == "at").unwrap();
            assert_eq!(at_prop.value, Expr::Tuple(vec![Expr::Num(35.0), Expr::Num(25.0)]));
            // color should be red
            let color_prop = props.iter().find(|p| p.name == "color").unwrap();
            assert_eq!(color_prop.value, Expr::Ident("red".into()));
        } else {
            panic!("Expected ActorDecl");
        }

        // Next should be an Ellipse (for circle)
        if let Stmt::ActorDecl { label, ty, .. } = &stmts[2] {
            assert_eq!(label, "circle_1");
            assert_eq!(ty, "Ellipse");
        }

        // Next should be a Group
        if let Stmt::ActorDecl { label, ty, children, .. } = &stmts[3] {
            assert_eq!(label, "group_2");
            assert_eq!(ty, "Group");
            // Group should have one child (rect_3)
            assert_eq!(children.len(), 1, "Group should have 1 child");
        } else {
            panic!("Expected Group ActorDecl");
        }
    }

    #[test]
    fn test_import_with_transform() {
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="100" height="50"
        transform="translate(200, 150) rotate(45) scale(2)"
        fill="#ff8800" stroke="#333" stroke-width="3"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_transform.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { props, .. } = &stmts[0] {
            // at: (0 + 50 + 200, 0 + 25 + 150) = (250, 175) after translate
            let at_prop = props.iter().find(|p| p.name == "at").unwrap();
            assert_eq!(at_prop.value, Expr::Tuple(vec![Expr::Num(250.0), Expr::Num(175.0)]));

            // rotation should be 45
            let rot_prop = props.iter().find(|p| p.name == "rotation").unwrap();
            assert_eq!(rot_prop.value, Expr::Num(45.0));

            // scale should be ~2
            let scale_prop = props.iter().find(|p| p.name == "scale").unwrap();
            assert!((2.0 - match &scale_prop.value { Expr::Num(n) => *n, _ => 0.0 }).abs() < 1e-6);

            // stroke_color
            let stroke_prop = props.iter().find(|p| p.name == "stroke_color").unwrap();
            assert_eq!(
                stroke_prop.value,
                Expr::Call("rgb".into(), vec![Expr::Num(0x33 as f64), Expr::Num(0x33 as f64), Expr::Num(0x33 as f64)])
            );

            // stroke_width
            let sw_prop = props.iter().find(|p| p.name == "stroke_width").unwrap();
            assert_eq!(sw_prop.value, Expr::Num(3.0));
        }
    }

    #[test]
    fn test_import_file_not_found() {
        let result = import_svg(Path::new("/nonexistent/file.svg"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SvgImportError::Io(_) => {} // expected
            other => panic!("Expected Io error, got: {other}"),
        }
    }

    #[test]
    fn test_parse_numbers() {
        let nums = parse_numbers("10, 20, 30");
        assert_eq!(nums, vec![10.0, 20.0, 30.0]);

        let nums = parse_numbers("1.5 2.5 3.5");
        assert_eq!(nums, vec![1.5, 2.5, 3.5]);
    }

    #[test]
    fn test_compose_transforms() {
        let a = Transform { tx: 10.0, ty: 20.0, rotation_deg: 0.0, sx: 1.0, sy: 1.0 };
        let b = Transform { tx: 5.0, ty: 5.0, rotation_deg: 30.0, sx: 2.0, sy: 2.0 };
        let c = a.compose(&b);
        assert!((c.tx - 15.0).abs() < 1e-6); // 10 + 5*1
        assert!((c.ty - 25.0).abs() < 1e-6); // 20 + 5*1
        assert!((c.rotation_deg - 30.0).abs() < 1e-6);
        assert!((c.sx - 2.0).abs() < 1e-6);
        assert!((c.sy - 2.0).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // New feature tests: viewBox, stroke-dasharray, gradients, polyline/polygon
    // -----------------------------------------------------------------------

    #[test]
    fn test_viewbox_config() {
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600">
  <rect x="10" y="10" width="50" height="30" fill="red"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_viewbox.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // First statement should be the @config from viewBox
        assert!(stmts.len() >= 1);
        if let Stmt::Config { settings, .. } = &stmts[0] {
            assert_eq!(settings.len(), 1);
            assert_eq!(settings[0].name, "size");
            assert_eq!(
                settings[0].value,
                Expr::Tuple(vec![Expr::Num(800.0), Expr::Num(600.0)])
            );
        } else {
            panic!("Expected Config as first statement from viewBox");
        }
    }

    #[test]
    fn test_viewbox_fallback_width_height() {
        // No viewBox, but width/height attributes should also generate config
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480">
  <rect x="0" y="0" width="100" height="100" fill="blue"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_viewbox_fallback.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert!(stmts.len() >= 1);
        if let Stmt::Config { settings, .. } = &stmts[0] {
            assert_eq!(settings[0].name, "size");
            assert_eq!(
                settings[0].value,
                Expr::Tuple(vec![Expr::Num(640.0), Expr::Num(480.0)])
            );
        } else {
            panic!("Expected Config as first statement from width/height");
        }
    }

    #[test]
    fn test_no_viewbox_no_config() {
        // No viewBox, no width/height → no config emitted
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="100" height="100" fill="green"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_no_viewbox.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // First statement should be the rect, not a config
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { ty, .. } = &stmts[0] {
            assert_eq!(ty, "Rect");
        } else {
            panic!("Expected ActorDecl (Rect) without any config");
        }
    }

    #[test]
    fn test_stroke_dasharray() {
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="10" y="10" width="50" height="30"
        fill="none" stroke="black" stroke-width="2"
        stroke-dasharray="5,3,2,3"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_dasharray.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Second statement (after config) should be the rect with stroke_dasharray
        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { props, .. } = &stmts[1] {
            let dash_prop = props.iter().find(|p| p.name == "stroke_dasharray").unwrap();
            assert_eq!(
                dash_prop.value,
                Expr::Tuple(vec![
                    Expr::Num(5.0),
                    Expr::Num(3.0),
                    Expr::Num(2.0),
                    Expr::Num(3.0),
                ])
            );
        } else {
            panic!("Expected ActorDecl with stroke_dasharray");
        }
    }

    #[test]
    fn test_stroke_dasharray_space_separated() {
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <path d="M10 10 L100 10" fill="none" stroke="red"
        stroke-dasharray="10 5 2 5"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_dasharray_space.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { props, .. } = &stmts[0] {
            let dash_prop = props.iter().find(|p| p.name == "stroke_dasharray").unwrap();
            assert_eq!(
                dash_prop.value,
                Expr::Tuple(vec![
                    Expr::Num(10.0),
                    Expr::Num(5.0),
                    Expr::Num(2.0),
                    Expr::Num(5.0),
                ])
            );
        } else {
            panic!("Expected ActorDecl with stroke_dasharray");
        }
    }

    #[test]
    fn test_gradient_url_fill_approximation() {
        // SVG with gradient definition and a rect using url(#grad)
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
  <defs>
    <linearGradient id="grad">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="10" y="10" width="100" height="80" fill="url(#grad)"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_gradient_url.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Should have config + rect
        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { props, .. } = &stmts[1] {
            let color_prop = props.iter().find(|p| p.name == "color").unwrap();
            // Average of #ff0000 (255,0,0) and #0000ff (0,0,255) ≈ (128, 0, 128)
            assert_eq!(
                color_prop.value,
                Expr::Call(
                    "rgb".into(),
                    vec![Expr::Num(128.0), Expr::Num(0.0), Expr::Num(128.0)]
                )
            );
        } else {
            panic!("Expected ActorDecl with gradient-approximated color");
        }
    }

    #[test]
    fn test_gradient_url_stroke() {
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <defs>
    <linearGradient id="mygrad">
      <stop offset="0%" stop-color="red"/>
      <stop offset="50%" stop-color="green"/>
      <stop offset="100%" stop-color="blue"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="50" height="50" fill="none" stroke="url(#mygrad)" stroke-width="3"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_gradient_stroke.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { props, .. } = &stmts[1] {
            // Should have stroke_color (approximated) and no regular color (fill="none")
            let stroke_prop = props.iter().find(|p| p.name == "stroke_color").unwrap();
            // Average of red(255,0,0), green(0,128,0), blue(0,0,255):
            // spans: 0.0→0.5: avg(red,green) (127,64,0) × 0.5 + 0.5→1.0: avg(green,blue) (0,64,127) × 0.5
            // = (63.5, 64, 63.5) → (64, 64, 64)
            // Let's just verify it's an rgb call
            assert!(matches!(&stroke_prop.value, Expr::Call(name, ..) if name == "rgb"));
        } else {
            panic!("Expected ActorDecl with gradient stroke");
        }
    }

    #[test]
    fn test_unknown_gradient_url_fill_none() {
        // url(#nonexistent) — gradient not in defs, result should be transparent (no color)
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" fill="url(#missing)"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_unknown_gradient.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Unresolvable url(#...) → fill_opacity: 0.0 (transparent)
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { props, .. } = &stmts[0] {
            let fill_opacity = props.iter().find(|p| p.name == "fill_opacity").unwrap();
            assert_eq!(fill_opacity.value, Expr::Num(0.0));
            // There should be no 'color' property
            assert!(props.iter().find(|p| p.name == "color").is_none());
        } else {
            panic!("Expected ActorDecl");
        }
    }

    #[test]
    fn test_defs_skip_gracefully() {
        // <defs> should not crash and should not produce any actor statements
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="50" height="50">
  <defs>
    <linearGradient id="g1">
      <stop offset="0%" stop-color="#fff"/>
      <stop offset="100%" stop-color="#000"/>
    </linearGradient>
    <radialGradient id="g2">
      <stop offset="0%" stop-color="red"/>
      <stop offset="100%" stop-color="blue"/>
    </radialGradient>
    <clipPath id="clip">
      <circle cx="25" cy="25" r="20"/>
    </clipPath>
  </defs>
  <rect x="0" y="0" width="50" height="50" fill="black"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_defs_skip.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        // Should have config + rect (defs/clipPath produce no statements directly)
        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { ty, .. } = &stmts[1] {
            assert_eq!(ty, "Rect");
        }
    }

    #[test]
    fn test_polygon_conversion() {
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <polygon points="0,0 50,0 50,50 0,50" fill="#ff8800"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_polygon.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { ty, props, .. } = &stmts[1] {
            assert_eq!(ty, "Path");
            // Should have commands with move_to, line_to, line_to, line_to, close
            let cmd_prop = props.iter().find(|p| p.name == "commands").unwrap();
            if let Expr::Tuple(commands) = &cmd_prop.value {
                assert_eq!(commands.len(), 5, "polygon should have 5 commands (4 lines + close)");
                assert_eq!(
                    commands[0],
                    Expr::Call("move_to".into(), vec![Expr::Num(0.0), Expr::Num(0.0)])
                );
                assert_eq!(
                    commands[4],
                    Expr::Call("close".into(), vec![])
                );
            } else {
                panic!("Expected Tuple of commands");
            }
        } else {
            panic!("Expected Path actor for polygon");
        }
    }

    #[test]
    fn test_polyline_conversion() {
        let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <polyline points="10,20 30,40 50,60" fill="none" stroke="blue" stroke-width="2"/>
</svg>"#;
        let dir = std::env::temp_dir();
        let path = dir.join("test_polyline.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { ty, props, .. } = &stmts[1] {
            assert_eq!(ty, "Path");
            let cmd_prop = props.iter().find(|p| p.name == "commands").unwrap();
            if let Expr::Tuple(commands) = &cmd_prop.value {
                assert_eq!(commands.len(), 3, "polyline should have 3 commands (move + 2 lines, no close)");
                assert_eq!(
                    commands[0],
                    Expr::Call("move_to".into(), vec![Expr::Num(10.0), Expr::Num(20.0)])
                );
                // No Z/close command
                assert!(!commands.iter().any(|c| matches!(c, Expr::Call(name, ..) if name == "close")));
            } else {
                panic!("Expected Tuple of commands");
            }
        } else {
            panic!("Expected Path actor for polyline");
        }
    }

    #[test]
    fn test_radial_gradient_url_fill() {
        let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <defs>
    <radialGradient id="rgrad">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="100%" stop-color="#000000"/>
    </radialGradient>
  </defs>
  <circle cx="50" cy="50" r="40" fill="url(#rgrad)"/>
</svg>"##;
        let dir = std::env::temp_dir();
        let path = dir.join("test_radial_gradient.svg");
        std::fs::write(&path, svg_content).unwrap();

        let stmts = import_svg(&path).unwrap();
        std::fs::remove_file(&path).ok();

        assert_eq!(stmts.len(), 2);
        if let Stmt::ActorDecl { props, .. } = &stmts[1] {
            let color_prop = props.iter().find(|p| p.name == "color").unwrap();
            // Average of white (255,255,255) and black (0,0,0) ≈ (128,128,128)
            assert_eq!(
                color_prop.value,
                Expr::Call(
                    "rgb".into(),
                    vec![Expr::Num(128.0), Expr::Num(128.0), Expr::Num(128.0)]
                )
            );
        } else {
            panic!("Expected ActorDecl with radial gradient color");
        }
    }

    #[test]
    fn test_stop_color_parse_named() {
        assert_eq!(parse_stop_color("red"), (255, 0, 0));
        assert_eq!(parse_stop_color("Blue"), (0, 0, 255));
        assert_eq!(parse_stop_color("GREEN"), (0, 128, 0));
        assert_eq!(parse_stop_color("black"), (0, 0, 0));
        assert_eq!(parse_stop_color("white"), (255, 255, 255));
        assert_eq!(parse_stop_color("orange"), (255, 165, 0));
    }

    #[test]
    fn test_stop_color_parse_hex() {
        assert_eq!(parse_stop_color("#ff0000"), (255, 0, 0));
        assert_eq!(parse_stop_color("#00ff00"), (0, 255, 0));
        assert_eq!(parse_stop_color("#0000ff"), (0, 0, 255));
        assert_eq!(parse_stop_color("#f00"), (255, 0, 0));
        assert_eq!(parse_stop_color("#abc"), (170, 187, 204));
    }

    #[test]
    fn test_stop_color_fallback() {
        assert_eq!(parse_stop_color("unknown-color"), (0, 0, 0));
        assert_eq!(parse_stop_color(""), (0, 0, 0));
    }

    #[test]
    fn test_gradient_approximation_single_stop() {
        let def = GradientDef::Linear {
            x1: 0.0, y1: 0.0, x2: 1.0, y2: 0.0,
            stops: vec![GradientStop { offset: 0.0, r: 200, g: 150, b: 100, opacity: 1.0 }],
        };
        let (r, g, b) = def.approximate_solid_color();
        assert_eq!((r, g, b), (200, 150, 100));
    }

    #[test]
    fn test_gradient_approximation_two_stops() {
        let def = GradientDef::Linear {
            x1: 0.0, y1: 0.0, x2: 1.0, y2: 0.0,
            stops: vec![
                GradientStop { offset: 0.0, r: 255, g: 0, b: 0, opacity: 1.0 },
                GradientStop { offset: 1.0, r: 0, g: 0, b: 255, opacity: 1.0 },
            ],
        };
        let (r, g, b) = def.approximate_solid_color();
        // Average of (255,0,0) and (0,0,255) = (127.5, 0, 127.5) → (128, 0, 128)
        assert_eq!((r, g, b), (128, 0, 128));
    }

    #[test]
    fn test_gradient_approximation_three_stops() {
        let def = GradientDef::Linear {
            x1: 0.0, y1: 0.0, x2: 1.0, y2: 0.0,
            stops: vec![
                GradientStop { offset: 0.0, r: 0, g: 0, b: 0, opacity: 1.0 },
                GradientStop { offset: 0.5, r: 128, g: 128, b: 128, opacity: 1.0 },
                GradientStop { offset: 1.0, r: 255, g: 255, b: 255, opacity: 1.0 },
            ],
        };
        let (r, g, b) = def.approximate_solid_color();
        // spans: 0.0→0.5: avg(0,128)=64 each ×0.5 = 32
        //        0.5→1.0: avg(128,255)=191.5 each ×0.5 = 95.75
        // total: 32 + 95.75 = 127.75 ≈ 128
        assert_eq!((r, g, b), (128, 128, 128));
    }
}