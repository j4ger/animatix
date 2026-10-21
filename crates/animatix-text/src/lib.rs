//! Text and math compilation: the typst/font stack as a pure service.
//!
//! Inputs are plain data (content strings, font settings, a font database);
//! outputs are glyph paths and metrics. No engine types, no scene graph, no
//! GPU — the engine calls this during build (text measurement feeds layout)
//! and frame evaluation (glyph runs), and the render layer only ever sees the
//! produced paths.

use kurbo::BezPath;

pub mod compiler;
pub mod fast_layout;
pub mod font_context;
pub mod transcript;
pub mod typst_world;

pub use compiler::*;
pub use fast_layout::*;
pub use font_context::*;
pub use typst_world::*;

pub use kurbo::Shape;

// Crate-internal re-exports for existing unit tests
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use font_context::{
    BUNDLED_FONTS, BundledFont, bundled_face, font_env_epoch, leaked_face_data, system_fonts_db,
};

/// A glyph path extracted from text, with color and opacity.
#[derive(Debug, Clone)]
pub struct TextPath {
    /// The glyph path.
    pub path: BezPath,
    /// The solid glyph fill as RGBA bytes. (Rich-text paints degrade to
    /// their solid RGBA; gradients already fell back to white.)
    pub color: [u8; 4],
    /// The opacity of the glyph (0.0–1.0).
    pub opacity: f32,
    /// Line index this glyph belongs to (0-indexed).
    pub line_idx: u16,
    /// Word index this glyph belongs to (0-indexed).
    pub word_idx: u16,
    /// Character index within the text content (0-indexed).
    pub char_idx: u16,
    /// Bounding box center of this glyph in local coordinates.
    pub glyph_center: [f32; 2],
    /// Cached bounding box of this glyph in local coordinates.
    pub bounds: kurbo::Rect,
}

impl TextPath {
    /// Create a new TextPath with basic fields and default layout indices.
    pub fn new(path: BezPath, color: [u8; 4], opacity: f32) -> Self {
        use kurbo::Shape as _;
        let b = path.bounding_box();
        let center = if b.width().is_finite() && b.height().is_finite() {
            [((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32]
        } else {
            [0.0, 0.0]
        };
        Self {
            path,
            color,
            opacity,
            line_idx: 0,
            word_idx: 0,
            char_idx: 0,
            glyph_center: center,
            bounds: b,
        }
    }
}

/// Convert an f32 RGBA color (0.0–1.0 components) to RGBA bytes.
pub fn f32_color_to_rgba8(color: &[f32; 4]) -> [u8; 4] {
    [
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    ]
}

/// Degrade a rich-text paint to solid RGBA bytes (gradients fall back to
/// white — matching the engine's previous behaviour for non-solid paints).
#[cfg(feature = "rich-text")]
pub fn paint_to_rgba(paint: &typst::visualize::Paint) -> [u8; 4] {
    match paint {
        typst::visualize::Paint::Solid(c) => {
            let v = c.to_vec4_u8();
            [v[0], v[1], v[2], v[3]]
        },
        _ => [255, 255, 255, 255],
    }
}

// ─────────────────────────────────────────────────────────────
// Text metrics for the plain-text fast path
// ─────────────────────────────────────────────────────────────

/// Font metrics extracted from `ttf_parser::Face`.
/// Used by the plain-text fast path to compute line layout.
pub struct TextMetrics {
    /// Ascent in font units (positive).
    pub ascent: f32,
    /// Descent in font units (negative).
    pub descent: f32,
    /// Line gap in font units.
    pub line_gap: f32,
    /// Units per em from the font face.
    pub units_per_em: f32,
}

/// Compiled text output containing glyph paths and font metrics.
///
/// The glyph paths are centered around (0, 0) for layout positioning.
/// The metrics provide baseline information for vertical alignment.
#[derive(Clone)]
pub struct CompiledText {
    /// Glyph paths, centered around the origin.
    pub glyphs: Vec<TextPath>,
    /// Font ascent in scene units (points), i.e. distance from baseline to top of em.
    pub ascent: f32,
    /// Font descent in scene units (points), i.e. distance from baseline to bottom of em
    /// (negative).
    pub descent: f32,
    /// Offset of the baseline from the text's center (0, 0) after centering.
    /// A positive value means the baseline is above the center.
    pub baseline_offset: f32,
    /// Optional hardware glyph layout for direct Vello draw_glyphs execution.
    pub hardware_layout: Option<fast_layout::HardwareTextLayout>,
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[path = "tests/text_tests.rs"]
#[cfg(test)]
mod tests;
