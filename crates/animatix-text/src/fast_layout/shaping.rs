use animatix_core::error::RenderError;

use crate::font_context::FontContext;
use crate::{CompiledText, TextPath, f32_color_to_rgba8};

use super::wrapped::compile_text_fast_wrapped;
use super::{
    emit_shaped_glyphs, finalize_layout, resolve_faces, shape_token, space_advance, tokenize_words,
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

    let resolved = resolve_faces(family, weight, style, size, font_ctx, "fast path")?;
    let rgba = f32_color_to_rgba8(&color);
    let space_width = space_advance(&resolved.faces, size, letter_spacing, word_spacing);

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut pending_hardware = Vec::new();
    let mut x_curr: f64 = 0.0;

    for (wi_idx, (word, space_before)) in tokenize_words(content).into_iter().enumerate() {
        if wi_idx > 0 && space_before {
            x_curr += space_width;
        }
        let (token_width, shaped_glyphs) = shape_token(word, &resolved.faces, size, letter_spacing);
        emit_shaped_glyphs(
            &mut glyphs,
            &mut pending_hardware,
            &resolved.faces,
            &shaped_glyphs,
            size,
            rgba,
            0,
            wi_idx as u16,
            x_curr,
            0.0,
        );
        x_curr += token_width;
    }

    Ok(finalize_layout(
        glyphs,
        pending_hardware,
        &resolved.faces,
        size,
        resolved.ascent * resolved.font_scale,
        resolved.descent * resolved.font_scale,
    ))
}
