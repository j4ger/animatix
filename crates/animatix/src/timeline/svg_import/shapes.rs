//! SVG shape converters: rect, circle, ellipse, path, line, poly, text, use, group.

use std::collections::HashMap;

use roxmltree::Node;

use super::defs::{GradientDef, MaskDef, parse_svg_color_with_gradients, svg_gradient_paint};
use super::transform::{Transform, parenthesized_args, parse_numbers, transform_to_props};
use super::{SvgImportError, attr_float, children_stmts_to_inline, convert_children, make_label};
use crate::ast::{Expr, Property, Stmt};
use crate::timeline::path_data::parse_svg_path_data;

/// Parse SVG color/fill/stroke value into an Animatix property expression.
///
/// Returns `None` for `none` (transparent), or an `Expr` usable as a color value.
/// `current_color` is the resolved color from the parent element's `color` attribute,
/// used when the value is `currentColor`.
pub(crate) fn parse_svg_color(value: &str, current_color: Option<&Expr>) -> Option<Expr> {
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

pub(crate) fn parse_hex_color(hex: &str) -> Option<Expr> {
    let hex = hex.trim();
    let rgb = if hex.len() == 3 {
        // Short form #RGB → #RRGGBB
        let r = hex.chars().next()?;
        let g = hex.chars().nth(1)?;
        let b = hex.chars().nth(2)?;
        u32::from_str_radix(&format!("{r}{r}{g}{g}{b}{b}"), 16).ok()
    } else if hex.len() == 6 {
        u32::from_str_radix(hex, 16).ok()
    } else {
        return None;
    }?;

    let r = ((rgb >> 16) & 0xFF) as f64;
    let g = ((rgb >> 8) & 0xFF) as f64;
    let b = (rgb & 0xFF) as f64;

    Some(Expr::Call("rgb".into(), vec![Expr::Num(r), Expr::Num(g), Expr::Num(b)]))
}

/// Convert a `<use>` element by resolving its href and cloning the referenced element.
pub(crate) fn convert_use(
    node: &Node,
    root: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
    masks: &HashMap<String, MaskDef>,
) -> Result<(), SvgImportError> {
    // Resolve href (try both href and xlink:href)
    let href = node.attribute("href").or_else(|| node.attribute("xlink:href"));

    let Some(href) = href else {
        return Ok(()); // No href — skip silently
    };

    // Remove leading # to get the id
    let id = href.trim_start_matches('#');

    // Find the referenced element by id
    let referenced = find_element_by_id(root, id);
    let Some(referenced) = referenced else {
        tracing::warn!("SVG import: <use> references unknown id '{}'", id);
        return Ok(());
    };

    // Convert the referenced element as if it were inline
    let tag = referenced.tag_name().name();
    match tag {
        "g" => convert_group(&referenced, stmts, counter, transform, gradients, masks)?,
        "rect" => convert_rect(&referenced, stmts, counter, transform, gradients)?,
        "circle" => convert_circle(&referenced, stmts, counter, transform, gradients)?,
        "ellipse" => convert_ellipse(&referenced, stmts, counter, transform, gradients)?,
        "path" => convert_path(&referenced, stmts, counter, transform, gradients)?,
        "line" => convert_line(&referenced, stmts, counter, transform, gradients)?,
        "polyline" | "polygon" => {
            convert_poly(&referenced, stmts, counter, transform, tag, gradients)?
        },
        _ => {
            // Unsupported element type for <use> — skip
            tracing::warn!("SVG import: <use> references unsupported element type '{}'", tag);
        },
    }

    Ok(())
}

/// Find an element by its id attribute in the SVG tree.
pub(crate) fn find_element_by_id<'a>(node: &Node<'a, 'a>, id: &str) -> Option<Node<'a, 'a>> {
    if node.attribute("id") == Some(id) {
        return Some(*node);
    }
    for child in node.children() {
        if let Some(found) = find_element_by_id(&child, id) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn convert_group(
    node: &Node,
    stmts: &mut Vec<Stmt>,
    counter: &mut u64,
    transform: &Transform,
    gradients: &HashMap<String, GradientDef>,
    masks: &HashMap<String, MaskDef>,
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
    convert_children(node, &mut children_stmts, counter, &Transform::identity(), gradients, masks)?;

    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label,
        array_index: None,
        ty: "Group".into(),
        props,
        modifiers: Vec::new(),
        children: children_stmts_to_inline(&children_stmts),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_rect(
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
        array_index: None,
        ty: "Rect".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_circle(
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
        array_index: None,
        ty: "Ellipse".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_ellipse(
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
        Property::new("size", Expr::Tuple(vec![Expr::Num(rx * 2.0), Expr::Num(ry * 2.0)])),
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
        array_index: None,
        ty: "Ellipse".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_path(
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
        Property::new("at", Expr::Tuple(vec![Expr::Num(transform.tx), Expr::Num(transform.ty)])),
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
        array_index: None,
        ty: "Path".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_line(
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
        Property::new(
            "size",
            Expr::Tuple(vec![
                Expr::Num(length / 2.0),
                Expr::Num(1.0), // minimal height
            ]),
        ),
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
        array_index: None,
        ty: "Line".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_poly(
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
        Property::new("commands", Expr::List(commands)),
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
        array_index: None,
        ty: "Path".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

pub(crate) fn convert_text(
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
        array_index: None,
        ty: "Text".into(),
        props,
        modifiers: Vec::new(),
        children: Vec::new(),
        span: None,
    });
    Ok(())
}

/// Walk up the element tree to find the nearest ancestor with the given attribute.
pub(crate) fn find_ancestor_attr<'a>(node: &Node<'a, 'a>, attr: &str) -> Option<&'a str> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if let Some(val) = parent.attribute(attr) {
            return Some(val);
        }
        current = parent.parent();
    }
    None
}

/// Add `fill`, `stroke`, `stroke-width`, `opacity` properties from SVG attributes.
/// Also handles `stroke-dasharray` parsing and `url(#...)` gradient references.
pub(crate) fn add_fill_stroke_props(
    node: &Node,
    props: &mut Vec<Property>,
    gradients: &HashMap<String, GradientDef>,
) {
    // Resolve parent color for currentColor support
    let current_color: Option<Expr> = node
        .attribute("color")
        .and_then(|c| parse_svg_color(c, None))
        .or_else(|| find_ancestor_attr(node, "color").and_then(|c| parse_svg_color(c, None)));

    // Fill color — handles hex, named, rgb(), url(#id) gradient references, and inherit
    let fill_value = node.attribute("fill").or_else(|| {
        if node.attribute("fill").is_some_and(|v| v.eq_ignore_ascii_case("inherit")) {
            find_ancestor_attr(node, "fill")
        } else {
            None
        }
    });
    if let Some(fill) = fill_value {
        if let Some(grad) = svg_gradient_paint(fill, gradients) {
            props.push(Property::new("fill_gradient", grad));
        } else if let Some(color_expr) =
            parse_svg_color_with_gradients(fill, gradients, current_color.as_ref())
        {
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

    // Stroke color — same as fill, supports url(#id) and inherit
    let stroke_value = node.attribute("stroke").or_else(|| {
        if node.attribute("stroke").is_some_and(|v| v.eq_ignore_ascii_case("inherit")) {
            find_ancestor_attr(node, "stroke")
        } else {
            None
        }
    });
    if let Some(stroke) = stroke_value {
        if let Some(grad) = svg_gradient_paint(stroke, gradients) {
            props.push(Property::new("stroke_gradient", grad));
        } else if let Some(color_expr) =
            parse_svg_color_with_gradients(stroke, gradients, current_color.as_ref())
        {
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

    // Stroke line cap: butt=0, round=1, square=2
    if let Some(linecap) = node.attribute("stroke-linecap") {
        let cap = match linecap {
            "round" => 1.0,
            "square" => 2.0,
            _ => 0.0,
        };
        props.push(Property::new("line_cap", Expr::Num(cap)));
    }

    // Stroke line join: miter=0, round=1, bevel=2
    if let Some(linejoin) = node.attribute("stroke-linejoin") {
        let join = match linejoin {
            "round" => 1.0,
            "bevel" => 2.0,
            _ => 0.0,
        };
        props.push(Property::new("line_join", Expr::Num(join)));
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
