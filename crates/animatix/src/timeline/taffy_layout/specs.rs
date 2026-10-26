//! Child sizing specs and parsing.

use super::*;

/// Describes how a single dimension (width or height) of a child should be sized.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SizeSpec {
    /// Fixed absolute size in logical pixels.
    Fixed(f32),
    /// Percentage of the parent's content box dimension (0.0–1.0).
    Percent(f32),
    /// Auto / intrinsic — size from content (container shrink-wrap).
    Auto,
    /// Fill — 100% of the parent's content box (same as Percent(1.0)).
    Fill,
    /// Fit-content — size to content, clamped by available space.
    Fit,
}

impl SizeSpec {
    /// Resolve this SizeSpec against a parent dimension (content box size).
    pub fn resolve(&self, _parent_dim: f32) -> Dimension {
        match self {
            SizeSpec::Fixed(v) => Dimension::length(*v),
            SizeSpec::Percent(pct) => Dimension::percent(*pct),
            SizeSpec::Auto | SizeSpec::Fit => Dimension::auto(),
            SizeSpec::Fill => Dimension::percent(1.0),
        }
    }

    /// Resolve to an absolute pixel value, given the parent content box size.
    pub fn resolve_absolute(&self, parent_dim: f32) -> f32 {
        match self {
            SizeSpec::Fixed(v) => *v,
            SizeSpec::Percent(pct) => parent_dim * pct,
            SizeSpec::Fill => parent_dim,
            SizeSpec::Auto | SizeSpec::Fit => 0.0, // unresolved
        }
    }
}

/// Full size specification for a child node (width and height).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChildSizeSpec {
    /// Width specification.
    pub width: SizeSpec,
    /// Height specification.
    pub height: SizeSpec,
}

impl ChildSizeSpec {
    /// Create a fixed size spec from half-extents (legacy).
    pub fn fixed(half_size: [f32; 2]) -> Self {
        Self {
            width: SizeSpec::Fixed(half_size[0] * 2.0),
            height: SizeSpec::Fixed(half_size[1] * 2.0),
        }
    }

    /// Create a size spec from percentage strings (e.g. "50%") and auto.
    pub fn from_parts(w_spec: SizeSpec, h_spec: SizeSpec) -> Self {
        Self {
            width: w_spec,
            height: h_spec,
        }
    }

    /// Return true if both dimensions are fixed absolute sizes.
    pub fn is_fixed(&self) -> bool {
        matches!(self.width, SizeSpec::Fixed(_)) && matches!(self.height, SizeSpec::Fixed(_))
    }

    /// Return true when a dimension is a share of the parent content box
    /// (`Percent`/`Fill`) and therefore needs a declared parent size.
    ///
    /// `Auto`/`Fit` size to content and resolve fine inside a shrink-wrapped
    /// container, so they do not count here — and neither does an unrecognized
    /// expression, which parses to `Auto`.
    pub fn needs_parent_box(&self) -> bool {
        matches!(self.width, SizeSpec::Percent(_) | SizeSpec::Fill)
            || matches!(self.height, SizeSpec::Percent(_) | SizeSpec::Fill)
    }
}

/// Parse a single dimension expression (Expr) into a SizeSpec.
/// Supports: numeric literal (fixed), string "50%" (percent), "auto", "fill", "fit", and ident
/// auto/fill/fit.
pub fn parse_dimension_spec(expr: &crate::ast::Expr) -> SizeSpec {
    use crate::ast::Expr;
    match expr {
        Expr::Num(n) => SizeSpec::Fixed(*n as f32),
        Expr::Percent(pct) => SizeSpec::Percent(*pct as f32 / 100.0),
        Expr::Str(s) => {
            if let Some(pct_str) = s.strip_suffix('%') {
                if let Ok(pct) = pct_str.parse::<f32>() {
                    return SizeSpec::Percent(pct / 100.0);
                }
            }
            match s.as_str() {
                "auto" => SizeSpec::Auto,
                "fill" => SizeSpec::Fill,
                "fit" => SizeSpec::Fit,
                _ => SizeSpec::Auto,
            }
        },
        Expr::Ident(s) => match s.as_str() {
            "auto" => SizeSpec::Auto,
            "fill" => SizeSpec::Fill,
            "fit" => SizeSpec::Fit,
            _ => SizeSpec::Auto,
        },
        _ => SizeSpec::Auto,
    }
}

/// Parse a `size` property expression into a ChildSizeSpec.
/// Supports: `(width, height)`, `fill`, `auto`, `fit`.
pub fn parse_size_spec(expr: &crate::ast::Expr) -> ChildSizeSpec {
    use crate::ast::Expr;
    match expr {
        Expr::Tuple(items) if items.len() == 2 => ChildSizeSpec::from_parts(
            parse_dimension_spec(&items[0]),
            parse_dimension_spec(&items[1]),
        ),
        Expr::Percent(pct) => {
            ChildSizeSpec::from_parts(SizeSpec::Percent(*pct as f32 / 100.0), SizeSpec::Auto)
        },
        // Single value: `size: fill`, `size: auto`, `size: fit`
        Expr::Str(s) => match s.as_str() {
            "fill" => ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto),
            "auto" | "fit" => ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto),
            _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
        },
        Expr::Ident(s) => match s.as_str() {
            "fill" => ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto),
            "auto" | "fit" => ChildSizeSpec::from_parts(SizeSpec::Auto, SizeSpec::Auto),
            _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
        },
        _ => ChildSizeSpec::fixed(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE),
    }
}
