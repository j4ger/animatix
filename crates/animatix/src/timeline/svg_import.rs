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
//! - No support for SVG `<defs>`, `<use>`, `<clipPath>`, `<mask>`, gradients, patterns
//! - SVG `<path>` `d` attribute: supports M, L, Q, C, Z commands (absolute/relative)
//! - SVG `<polyline>` / `<polygon>`: should be added (fallback to path)
//! - SVG `viewBox` on root `<svg>`: not yet used to set scene dimensions
//! - SVG `currentColor`, `inherit`, `url(...)` fill types: not yet supported
//! - SVG `stroke-dasharray`, `stroke-linecap`, `stroke-linejoin`: not yet mapped

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
    convert_children(&root, &mut stmts, &mut counter, &Transform::identity())?;
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
        let nums: Vec<f64> = parse_numbers(&args);
        if nums.len() >= 2 {
            t.tx += nums[0];
            t.ty += nums[1];
        } else if nums.len() == 1 {
            t.tx += nums[0];
        }
        Some(rest)
    } else if s.starts_with("rotate(") {
        let (args, rest) = parenthesized_args(s, "rotate(")?;
        let nums: Vec<f64> = parse_numbers(&args);
        if !nums.is_empty() {
            t.rotation_deg += nums[0];
        }
        Some(rest)
    } else if s.starts_with("scale(") {
        let (args, rest) = parenthesized_args(s, "scale(")?;
        let nums: Vec<f64> = parse_numbers(&args);
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
        let nums: Vec<f64> = parse_numbers(&args);
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
    s.split(|c: char| c == ',' || c == ' ' || c == '\t' || c == '\n' || c == '\r')
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
// Color parsing
// ---------------------------------------------------------------------------

/// Parse SVG color/fill/stroke value into an Animatix property expression.
///
/// Returns `None` for `none` (transparent), or an `Expr` usable as a color value.
fn parse_svg_color(value: &str) -> Option<Expr> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return None; // caller can treat this as transparent / omit fill
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
        let nums: Vec<f64> = parse_numbers(&inner);
        if nums.len() >= 3 {
            let r = (nums[0] as u8).min(255);
            let g = (nums[1] as u8).min(255);
            let b = (nums[2] as u8).min(255);
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
        let r = hex.chars().nth(0)?;
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
                // Unknown command — skip
                break;
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
                if ch == '-' && current.chars().last().map_or(false, |c| c.is_ascii_digit()) {
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
            "g" => convert_group(&child, stmts, counter, &combined)?,
            "rect" => convert_rect(&child, stmts, counter, &combined)?,
            "circle" => convert_circle(&child, stmts, counter, &combined)?,
            "ellipse" => convert_ellipse(&child, stmts, counter, &combined)?,
            "path" => convert_path(&child, stmts, counter, &combined)?,
            "text" => convert_text(&child, stmts, counter, &combined, parent)?,
            "svg" => convert_children(&child, stmts, counter, &combined)?, // recurse into svg roots
            "line" => convert_line(&child, stmts, counter, &combined)?,
            "polyline" | "polygon" => {
                // For now, convert to a Path element with line commands
                convert_poly(&child, stmts, counter, &combined, tag)?;
            }
            "defs" | "clipPath" | "mask" | "pattern" | "linearGradient"
            | "radialGradient" | "filter" | "style" | "title" | "desc" => {
                // Silently skip non-rendering elements
            }
            _ => {
                // Unknown element: still recurse in case it has renderable children
                convert_children(&child, stmts, counter, &combined)?;
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
    convert_children(node, &mut children_stmts, counter, &Transform::identity())?;

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
) -> Result<(), SvgImportError> {
    let label = make_label("rect", counter);
    let x = attr_float(node, "x", 0.0);
    let y = attr_float(node, "y", 0.0);
    let w = attr_float(node, "width", 0.0);
    let h = attr_float(node, "height", 0.0);

    let center_x = x + w / 2.0 + transform.tx;
    let center_y = y + h / 2.0 + transform.ty;

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![Expr::Num(w as f64), Expr::Num(h as f64)])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(center_x), Expr::Num(center_y)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        // Aspect-preserving: use uniform scale that matches x
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
) -> Result<(), SvgImportError> {
    let label = make_label("circle", counter);
    let cx = attr_float(node, "cx", 0.0) + transform.tx;
    let cy = attr_float(node, "cy", 0.0) + transform.ty;
    let r = attr_float(node, "r", 0.0);
    let diameter = r * 2.0;

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![Expr::Num(diameter as f64), Expr::Num(diameter as f64)])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
) -> Result<(), SvgImportError> {
    let label = make_label("ellipse", counter);
    let cx = attr_float(node, "cx", 0.0) + transform.tx;
    let cy = attr_float(node, "cy", 0.0) + transform.ty;
    let rx = attr_float(node, "rx", 0.0);
    let ry = attr_float(node, "ry", 0.0);

    let mut props = vec![
        Property::new("size", Expr::Tuple(vec![
            Expr::Num((rx * 2.0) as f64),
            Expr::Num((ry * 2.0) as f64),
        ])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    if transform.rotation_deg != 0.0 {
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
            Expr::Num((length / 2.0) as f64),
            Expr::Num(1.0), // minimal height
        ])),
        Property::new("at", Expr::Tuple(vec![Expr::Num(cx), Expr::Num(cy)])),
    ];

    // Rotation to align with direction
    if length > 0.0 {
        let angle = dy.atan2(dx).to_degrees();
        props.push(Property::new("rotation", Expr::Num(angle as f64)));
    }

    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
        props.push(Property::new("rotation", Expr::Num(transform.rotation_deg as f64)));
    }
    if (transform.sx - 1.0).abs() > 1e-6 || (transform.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (transform.sx + transform.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }

    add_fill_stroke_props(node, &mut props);

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
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
        props.push(Property::new("rotation", Expr::Num(t.rotation_deg as f64)));
    }
    if (t.sx - 1.0).abs() > 1e-6 || (t.sy - 1.0).abs() > 1e-6 {
        let uniform_scale = (t.sx + t.sy) / 2.0;
        props.push(Property::new("scale", Expr::Num(uniform_scale as f64)));
    }
    props
}

/// Add `fill`, `stroke`, `stroke-width`, `opacity` properties from SVG attributes.
fn add_fill_stroke_props(node: &Node, props: &mut Vec<Property>) {
    // Fill color
    if let Some(fill) = node.attribute("fill") {
        if let Some(color_expr) = parse_svg_color(fill) {
            props.push(Property::new("color", color_expr));
        } else {
            // fill="none" → transparent
            props.push(Property::new("fill_opacity", Expr::Num(0.0)));
        }
    }

    // Fill opacity
    if let Some(fill_opacity) = node.attribute("fill-opacity") {
        if let Ok(op) = fill_opacity.parse::<f64>() {
            props.push(Property::new("fill_opacity", Expr::Num(op)));
        }
    }

    // Stroke color
    if let Some(stroke) = node.attribute("stroke") {
        if let Some(color_expr) = parse_svg_color(stroke) {
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

    // Overall opacity
    if let Some(opacity) = node.attribute("opacity") {
        if let Ok(op) = opacity.parse::<f64>() {
            props.push(Property::new("opacity", Expr::Num(op)));
        }
    }
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
        let expr = parse_svg_color("#ff0000").unwrap();
        assert_eq!(
            expr,
            Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
        );
    }

    #[test]
    fn test_parse_short_hex_color() {
        let expr = parse_svg_color("#f00").unwrap();
        assert_eq!(
            expr,
            Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
        );
    }

    #[test]
    fn test_parse_named_color() {
        let expr = parse_svg_color("red").unwrap();
        assert_eq!(expr, Expr::Ident("red".into()));
    }

    #[test]
    fn test_parse_none_color() {
        let expr = parse_svg_color("none");
        assert!(expr.is_none());
    }

    #[test]
    fn test_parse_rgb_color() {
        let expr = parse_svg_color("rgb(255, 0, 0)").unwrap();
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

        // Should have rect_0, circle_1, group_2 (with rect_3 inside)
        assert_eq!(stmts.len(), 3, "expected 3 root statements, got {}", stmts.len());

        // First should be a Rect
        if let Stmt::ActorDecl { label, ty, props, children, .. } = &stmts[0] {
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

        // Second should be an Ellipse (for circle)
        if let Stmt::ActorDecl { label, ty, .. } = &stmts[1] {
            assert_eq!(label, "circle_1");
            assert_eq!(ty, "Ellipse");
        }

        // Third should be a Group
        if let Stmt::ActorDecl { label, ty, children, .. } = &stmts[2] {
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
}