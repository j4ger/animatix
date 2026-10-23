//! Plain-text fast path: direct `rustybuzz` shaping and `ttf-parser` outlines,
//! bypassing Typst entirely.
//!
//! The glyph types and token shaper are shared by both entry points, so they
//! stay here; the split adds no visibility changes.

mod shaping;
mod wrapped;

pub use shaping::compile_text_fast;
pub use wrapped::compile_text_fast_wrapped;

/// Font face with both `ttf-parser` and `rustybuzz` representations for fast-path shaping.
pub struct FastPathFace {
    pub ttf: ttf_parser::Face<'static>,
    pub rb: rustybuzz::Face<'static>,
    pub family: String,
    pub font_data: &'static [u8],
    pub font_index: u32,
}

impl FastPathFace {
    pub fn from_data(data: &'static [u8], index: u32, family: String) -> Option<Self> {
        let ttf = ttf_parser::Face::parse(data, index).ok()?;
        let rb = rustybuzz::Face::from_slice(data, index)?;
        Some(Self {
            ttf,
            rb,
            family,
            font_data: data,
            font_index: index,
        })
    }
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

fn shape_token(
    token_text: &str,
    faces: &[FastPathFace],
    size: f32,
    letter_spacing: f32,
) -> (f64, Vec<ShapedGlyph>) {
    if token_text.is_empty() || faces.is_empty() {
        return (0.0, Vec::new());
    }

    let mut best_face_idx = 0;
    let mut best_missing = usize::MAX;
    let mut best_buffer: Option<rustybuzz::GlyphBuffer> = None;

    for (idx, face) in faces.iter().enumerate() {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(token_text);
        let shaped = rustybuzz::shape(&face.rb, &[], buffer);
        let missing_count = shaped.glyph_infos().iter().filter(|g| g.glyph_id == 0).count();
        if missing_count == 0 {
            best_face_idx = idx;
            best_buffer = Some(shaped);
            break;
        }
        if missing_count < best_missing {
            best_missing = missing_count;
            best_face_idx = idx;
            best_buffer = Some(shaped);
        }
    }

    let shaped = match best_buffer {
        Some(b) => b,
        None => {
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(token_text);
            rustybuzz::shape(&faces[0].rb, &[], buffer)
        },
    };

    let face = &faces[best_face_idx];
    let units_per_em = face.ttf.units_per_em() as f32;
    let font_scale = (size / units_per_em) as f64;
    let letter_spacing_f64 = letter_spacing as f64;

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_width = 0.0f64;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        if info.glyph_id == 0 && token_text.trim().is_empty() {
            total_width += pos.x_advance as f64 * font_scale + letter_spacing_f64;
            continue;
        }
        let x_adv = pos.x_advance as f64 * font_scale + letter_spacing_f64;
        let x_off = pos.x_offset as f64 * font_scale;
        let y_off = pos.y_offset as f64 * font_scale;

        glyphs.push(ShapedGlyph {
            face_idx: best_face_idx,
            glyph_id: info.glyph_id as u16,
            x_offset: total_width + x_off,
            y_offset: y_off,
        });
        total_width += x_adv;
    }

    (total_width, glyphs)
}
