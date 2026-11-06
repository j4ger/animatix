//! Plain-text fast path: direct `rustybuzz` shaping and `ttf-parser` outlines,
//! bypassing Typst entirely.
//!
//! The two entry points ([`compile_text_fast`] and
//! [`compile_text_fast_wrapped`]) keep their own layout loops, so wrapping and
//! the single-line path do not pay for each other's bookkeeping. Everything
//! around those loops — face resolution, tokenization, glyph emission, and
//! centering — is shared here so the two paths cannot drift apart.

mod face;
mod fallback;
mod layout;
mod shaping;
mod wrapped;

pub use shaping::compile_text_fast;
pub use wrapped::compile_text_fast_wrapped;

use layout::{
    emit_shaped_glyphs, finalize_layout, resolve_faces, shape_token, space_advance, tokenize_words,
};

/// Font face with both `ttf-parser` and `rustybuzz` representations for fast-path shaping.
pub struct FastPathFace {
    pub ttf: ttf_parser::Face<'static>,
    pub rb: rustybuzz::Face<'static>,
    pub family: String,
    pub font_data: &'static [u8],
    pub font_index: u32,
}

/// A compact hardware glyph descriptor for direct Vello hardware glyph run rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareGlyph {
    /// Font glyph ID.
    pub id: u32,
    /// X position in local scene coordinates (points) relative to text center.
    pub x: f32,
    /// Y position in local scene coordinates (points) relative to text center.
    pub y: f32,
}

/// A contiguous run of glyphs sharing the same font face and font size.
#[derive(Debug, Clone)]
pub struct HardwareGlyphRun {
    /// Pointer to static font bytes.
    pub font_data: &'static [u8],
    /// Font index inside font collection (ttc).
    pub font_index: u32,
    /// Font size in points.
    pub font_size: f32,
    /// Stream of positioned glyphs.
    pub glyphs: Vec<HardwareGlyph>,
}

/// A complete hardware glyph layout comprised of one or more runs (supporting font fallback).
#[derive(Debug, Clone, Default)]
pub struct HardwareTextLayout {
    /// Sequence of runs in draw order.
    pub runs: Vec<HardwareGlyphRun>,
}

struct HardwarePendingGlyph {
    face_idx: usize,
    id: u32,
    x: f32,
    y: f32,
}

struct ShapedGlyph {
    face_idx: usize,
    glyph_id: u16,
    x_offset: f64,
    y_offset: f64,
}

/// Faces resolved for one fast-path compilation.
struct ResolvedFaces {
    /// Primary face first, then every bundled fallback face in bundle order.
    faces: Vec<FastPathFace>,
    /// `size / units_per_em` of the primary face.
    font_scale: f32,
    /// Primary-face ascent in font units (positive).
    ascent: f32,
    /// Primary-face descent in font units (negative).
    descent: f32,
    /// Primary-face line gap in font units.
    line_gap: f32,
}
