use animatix_core::error::RenderError;
use kurbo::{Affine, BezPath, Shape};

use crate::font_context::{BUNDLED_FONTS, FontContext, bundled_face, resolve_font_family};
use crate::typst_world::{PathBuilder, center_text_paths, is_cjk_char};
use crate::{CompiledText, TextPath, f32_color_to_rgba8};

use super::wrapped::compile_text_fast_wrapped;
use super::{
    FastPathFace, HardwareGlyph, HardwareGlyphRun, HardwarePendingGlyph, HardwareTextLayout,
    shape_token,
};

/// Compile plain text into glyph paths using `rustybuzz` and `ttf_parser` directly,
/// bypassing Typst entirely. This is the fast path.
///
/// The returned paths use the same coordinate convention as `extract_glyphs`
/// (centered around origin), so `measure_text_paths` works unchanged.
pub fn compile_text_fast(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
) -> Result<CompiledText, RenderError> {
    // Early return for empty string: no glyphs, zero metrics
    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: 0.0,
            descent: 0.0,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    if content.contains('\n') {
        return compile_text_fast_wrapped(
            content,
            family,
            weight,
            style,
            size,
            1.2,
            color,
            letter_spacing,
            word_spacing,
            font_ctx,
            0.0,
            None,
            "left",
            "visible",
        );
    }

    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;
    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;

    let mut faces = vec![primary_face];
    faces.extend(
        BUNDLED_FONTS
            .iter()
            .filter(|bf| bf.family != resolved_family)
            .filter_map(|bf| FastPathFace::from_data(bf.data, 0, bf.family.to_string())),
    );

    let rgba = f32_color_to_rgba8(&color);

    let mut char_indices = content.char_indices().peekable();
    let mut pending_space = false;
    let mut word_start: Option<usize> = None;
    let mut raw_tokens: Vec<(&str, bool)> = Vec::new();

    while let Some((idx, ch)) = char_indices.next() {
        if ch == ' ' || ch == '\t' {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
            }
            pending_space = true;
        } else if is_cjk_char(ch) {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
                pending_space = false;
            }
            let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(content.len());
            raw_tokens.push((&content[idx..next_idx], pending_space));
            pending_space = false;
        } else if word_start.is_none() {
            word_start = Some(idx);
        }
    }
    if let Some(start) = word_start {
        raw_tokens.push((&content[start..], pending_space));
    }

    let space_advance = {
        let (w, _) = shape_token(" ", &faces, size, letter_spacing);
        if w > 0.0 {
            w + word_spacing as f64
        } else {
            (size as f64 * 0.25) + word_spacing as f64
        }
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut x_curr: f64 = 0.0;
    let mut pending_hardware: Vec<HardwarePendingGlyph> = Vec::new();

    for (wi_idx, (w, space_before)) in raw_tokens.into_iter().enumerate() {
        if wi_idx > 0 && space_before {
            x_curr += space_advance;
        }
        let (token_width, shaped_glyphs) = shape_token(w, &faces, size, letter_spacing);
        for g in shaped_glyphs {
            let face = &faces[g.face_idx];
            let glyph_scale = size / (face.ttf.units_per_em() as f32);

            pending_hardware.push(HardwarePendingGlyph {
                face_idx: g.face_idx,
                id: g.glyph_id as u32,
                x: (x_curr + g.x_offset) as f32,
                y: g.y_offset as f32,
            });

            let mut builder = PathBuilder(BezPath::new());
            if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                let path = builder.0;
                let scale_affine =
                    Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                let translate =
                    Affine::translate(kurbo::Vec2::new(x_curr + g.x_offset, g.y_offset));
                let final_affine = translate * scale_affine;
                let mut final_path = path;
                final_path.apply_affine(final_affine);

                let c_idx = glyphs.len() as u16;
                let bounds = final_path.bounding_box();
                glyphs.push(TextPath {
                    path: final_path,
                    color: rgba,
                    opacity: 1.0,
                    line_idx: 0,
                    word_idx: wi_idx as u16,
                    char_idx: c_idx,
                    glyph_center: [0.0, 0.0],
                    bounds,
                });
            }
        }
        x_curr += token_width;
    }

    // Compute bounding box BEFORE centering to find center_x and center_y
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

    // Center paths around origin (same as center_text_paths)
    // capture baseline offset: was at Y=0 before centering
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    let hardware_layout = if !pending_hardware.is_empty() {
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
    } else {
        None
    };

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
        hardware_layout,
    })
}
