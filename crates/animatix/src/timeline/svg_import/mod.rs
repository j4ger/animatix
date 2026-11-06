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
//! - SVG `<clipPath>` and `clip-path` references are unsupported: clip
//!   definitions and references are ignored, so imported shapes are not clipped
//! - SVG patterns are unsupported
//! - SVG `<path>` `d` attribute: supports M/m, L/l, H/h, V/v, Q/q, C/c, S/s, T/t, A/a, Z/z; `A`/`a`
//!   arcs are approximated as a straight chord
//! - SVG `currentColor`, `inherit` fill types: not yet supported
//! - SVG `stroke-linecap`, `stroke-linejoin`: not yet mapped

pub(crate) mod defs;
mod import;
pub(crate) mod shapes;
pub(crate) mod transform;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub(crate) use self::defs::{GradientDef, GradientStop, MaskDef};
pub use self::import::import_svg;
pub(crate) use self::import::{attr_float, children_stmts_to_inline, convert_children, make_label};
#[allow(unused_imports)]
pub(crate) use self::transform::Transform;

// The test module reaches these through `use super::*`; production code in this
// file no longer needs them, so they are gated to avoid unused-import warnings.
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use crate::ast::{Expr, Stmt};

// Error type

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
