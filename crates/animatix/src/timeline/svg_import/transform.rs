//! SVG transform parsing and representation.

use crate::ast::{Expr, Property};

/// A parsed 2D transform from an SVG `transform` attribute.
#[derive(Clone, Debug, Default)]
pub(crate) struct Transform {
    pub(crate) tx: f64,
    pub(crate) ty: f64,
    pub(crate) rotation_deg: f64,
    pub(crate) sx: f64,
    pub(crate) sy: f64,
}

impl Transform {
    pub(crate) fn identity() -> Self {
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
    pub(crate) fn compose(&self, other: &Transform) -> Self {
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
pub(crate) fn parse_transform(value: &str) -> Transform {
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

pub(crate) fn parse_single_transform<'a>(s: &'a str, t: &mut Transform) -> Option<&'a str> {
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
pub(crate) fn parenthesized_args<'a>(s: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
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
pub(crate) fn parse_numbers(s: &str) -> Vec<f64> {
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

/// Extract position/rotation/scale from a transform into `Property` items.
pub(crate) fn transform_to_props(t: &Transform) -> Vec<Property> {
    let mut props = Vec::new();
    if t.tx != 0.0 || t.ty != 0.0 {
        props.push(Property::new("at", Expr::Tuple(vec![Expr::Num(t.tx), Expr::Num(t.ty)])));
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
