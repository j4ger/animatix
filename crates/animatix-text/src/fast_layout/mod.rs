//! Plain-text fast path: direct `rustybuzz` shaping and `ttf-parser` outlines,
//! bypassing Typst entirely.
//!
//! The two entry points ([`compile_text_fast`] and
//! [`compile_text_fast_wrapped`]) keep their own layout loops, so wrapping and
//! the single-line path do not pay for each other's bookkeeping. Everything
//! around those loops — face resolution, tokenization, glyph emission, and
//! centering — is shared here so the two paths cannot drift apart.

use animatix_core::error::RenderError;
use kurbo::{Affine, BezPath, Shape};

use crate::font_context::{FontContext, bundled_face, resolve_font_family};
use crate::typst_world::{PathBuilder, center_text_paths, is_cjk_char};
use crate::{CompiledText, TextPath};

mod fallback;
mod shaping;
mod wrapped;

use fallback::fallback_faces;

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

/// Resolve `family` to a primary face plus the bundled fallback faces.
///
/// `caller` names the requesting entry point in error messages (`"fast path"`,
/// `"fast path wrapped"`), so a load failure still says which layout path
/// asked for the face.
fn resolve_faces(
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    font_ctx: &FontContext,
    caller: &str,
) -> Result<ResolvedFaces, RenderError> {
    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for {}",
            resolved_family, weight, style, caller
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for {}",
            resolved_family, weight, style, caller
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;
    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;
    let line_gap = primary_face.ttf.line_gap() as f32;

    let mut faces = vec![primary_face];
    faces.extend(fallback_faces(font_ctx, &resolved_family));

    Ok(ResolvedFaces {
        faces,
        font_scale,
        ascent,
        descent,
        line_gap,
    })
}

/// Split one line into words and single CJK characters.
///
/// The `bool` records whether whitespace separated the token from the previous
/// one. Callers pass a single line: newlines are the wrapped path's paragraph
/// boundary, not a token separator.
fn tokenize_words(content: &str) -> Vec<(&str, bool)> {
    let mut char_indices = content.char_indices().peekable();
    let mut pending_space = false;
    let mut word_start: Option<usize> = None;
    let mut tokens: Vec<(&str, bool)> = Vec::new();

    while let Some((idx, ch)) = char_indices.next() {
        if ch == ' ' || ch == '\t' {
            if let Some(start) = word_start.take() {
                tokens.push((&content[start..idx], pending_space));
            }
            pending_space = true;
        } else if is_cjk_char(ch) {
            if let Some(start) = word_start.take() {
                tokens.push((&content[start..idx], pending_space));
                pending_space = false;
            }
            let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(content.len());
            tokens.push((&content[idx..next_idx], pending_space));
            pending_space = false;
        } else if word_start.is_none() {
            word_start = Some(idx);
        }
    }
    if let Some(start) = word_start {
        tokens.push((&content[start..], pending_space));
    }
    tokens
}

/// Advance for one inter-word space, including `word_spacing`.
///
/// Falls back to a quarter em when the resolved faces cannot shape a space.
#[inline]
fn space_advance(faces: &[FastPathFace], size: f32, letter_spacing: f32, word_spacing: f32) -> f64 {
    let (w, _) = shape_token(" ", faces, size, letter_spacing);
    if w > 0.0 {
        w + word_spacing as f64
    } else {
        (size as f64 * 0.25) + word_spacing as f64
    }
}

/// Append one shaped token's glyphs at `(x, y)`.
///
/// Every shaped glyph gets a hardware descriptor (even when the face has no
/// outline for it, so Vello can draw a fallback); outline glyphs additionally
/// become [`TextPath`]s. Shared by both layout paths and by the wrapped path's
/// ellipsis so the scale/translate convention exists once.
#[inline]
#[allow(clippy::too_many_arguments)] // One glyph run needs position, ids, and target lists.
fn emit_shaped_glyphs(
    glyphs: &mut Vec<TextPath>,
    pending_hardware: &mut Vec<HardwarePendingGlyph>,
    faces: &[FastPathFace],
    shaped: &[ShapedGlyph],
    size: f32,
    rgba: [u8; 4],
    line_idx: u16,
    word_idx: u16,
    x: f64,
    y: f64,
) {
    for g in shaped {
        let face = &faces[g.face_idx];
        let glyph_scale = size / (face.ttf.units_per_em() as f32);

        pending_hardware.push(HardwarePendingGlyph {
            face_idx: g.face_idx,
            id: g.glyph_id as u32,
            x: (x + g.x_offset) as f32,
            y: (y + g.y_offset) as f32,
        });

        let mut builder = PathBuilder(BezPath::new());
        if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
            let path = builder.0;
            let scale_affine = Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
            let translate = Affine::translate(kurbo::Vec2::new(x + g.x_offset, y + g.y_offset));
            let final_affine = translate * scale_affine;
            let mut final_path = path;
            final_path.apply_affine(final_affine);

            let c_idx = glyphs.len() as u16;
            let bounds = final_path.bounding_box();
            glyphs.push(TextPath {
                path: final_path,
                color: rgba,
                opacity: 1.0,
                line_idx,
                word_idx,
                char_idx: c_idx,
                glyph_center: [0.0, 0.0],
                bounds,
            });
        }
    }
}

/// Center compiled paths and package them with metrics and hardware runs.
///
/// `ascent`/`descent` are already scaled to points. Both layout paths finish
/// through here so the centering convention and hardware-run grouping cannot
/// drift between them.
fn finalize_layout(
    mut glyphs: Vec<TextPath>,
    pending_hardware: Vec<HardwarePendingGlyph>,
    faces: &[FastPathFace],
    size: f32,
    ascent: f32,
    descent: f32,
) -> CompiledText {
    // Bounding box before centering: the hardware runs are shifted by the same
    // center `center_text_paths` applies to the outlines.
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    let baseline_offset = center_text_paths(&mut glyphs);

    let hardware_layout = if pending_hardware.is_empty() {
        None
    } else {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut() {
                if std::ptr::eq(last_run.font_data, face.font_data)
                    && last_run.font_index == face.font_index
                {
                    last_run.glyphs.push(HardwareGlyph {
                        id: pg.id,
                        x: centered_x,
                        y: centered_y,
                    });
                    continue;
                }
            }
            runs.push(HardwareGlyphRun {
                font_data: face.font_data,
                font_index: face.font_index,
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    };

    CompiledText {
        glyphs,
        ascent,
        descent,
        baseline_offset,
        hardware_layout,
    }
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
