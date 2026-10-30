use animatix_core::error::RenderError;

use crate::font_context::FontContext;
use crate::{CompiledText, TextPath, f32_color_to_rgba8};

use super::{
    ShapedGlyph, emit_shaped_glyphs, finalize_layout, resolve_faces, shape_token, space_advance,
    tokenize_words,
};

/// Compile plain text into glyph paths with word wrapping (fast path).
///
/// Like `compile_text_fast`, but wraps text at `max_width` points.
/// Honors `text_align` per line and `overflow` mode.
#[allow(clippy::too_many_arguments)]
pub fn compile_text_fast_wrapped(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    line_height: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
    max_width: f32,
    max_lines: Option<usize>,
    text_align: &str,
    overflow: &str,
) -> Result<CompiledText, RenderError> {
    let resolved = resolve_faces(family, weight, style, size, font_ctx, "fast path wrapped")?;

    // Line height in absolute points
    let lh_mult = if line_height.is_finite() && line_height > 0.0 {
        line_height
    } else {
        1.2
    };
    let line_height_pts =
        (resolved.ascent - resolved.descent + resolved.line_gap) * resolved.font_scale * lh_mult;

    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: resolved.ascent * resolved.font_scale,
            descent: resolved.descent * resolved.font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    struct WordInfo {
        width: f64, // total advance in scene coords
        glyphs: Vec<ShapedGlyph>,
        space_before: bool,
    }

    struct LineInfo {
        words: Vec<usize>, // indices into word_infos
        total_width: f64,
    }

    let mut word_infos: Vec<WordInfo> = Vec::new();
    let mut lines: Vec<LineInfo> = Vec::new();
    let wrap_threshold = if max_width > 0.0 {
        max_width as f64
    } else {
        f64::INFINITY
    };

    let rgba = f32_color_to_rgba8(&color);
    let space_width = space_advance(&resolved.faces, size, letter_spacing, word_spacing);

    for paragraph in content.split('\n') {
        let p = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        let raw_tokens = tokenize_words(p);

        if raw_tokens.is_empty() {
            lines.push(LineInfo {
                words: Vec::new(),
                total_width: 0.0,
            });
            continue;
        }

        let mut paragraph_word_indices = Vec::with_capacity(raw_tokens.len());
        for (w, space_before) in raw_tokens {
            let (token_width, shaped_glyphs) =
                shape_token(w, &resolved.faces, size, letter_spacing);
            let wi_idx = word_infos.len();
            word_infos.push(WordInfo {
                width: token_width,
                glyphs: shaped_glyphs,
                space_before,
            });
            paragraph_word_indices.push(wi_idx);
        }

        let mut current_line = LineInfo {
            words: Vec::new(),
            total_width: 0.0,
        };

        for &wi_idx in &paragraph_word_indices {
            let wi = &word_infos[wi_idx];
            let space_needed = if current_line.words.is_empty() {
                0.0
            } else if wi.space_before {
                space_width
            } else {
                0.0
            };

            if current_line.total_width + space_needed + wi.width > wrap_threshold
                && !current_line.words.is_empty()
            {
                lines.push(current_line);
                current_line = LineInfo {
                    words: Vec::new(),
                    total_width: 0.0,
                };
            }

            if !current_line.words.is_empty() && wi.space_before {
                current_line.total_width += space_width;
            }
            current_line.words.push(wi_idx);
            current_line.total_width += wi.width;
        }
        if !current_line.words.is_empty() {
            lines.push(current_line);
        }
    }

    if word_infos.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: resolved.ascent * resolved.font_scale,
            descent: resolved.descent * resolved.font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    // Handle overflow. An explicit `max_lines` (the `max_lines` property) wins;
    // otherwise ellipsis/clip keeps the old "one page worth" guard so unbounded
    // growth is still prevented.
    let line_limit = max_lines.or(if overflow == "ellipsis" || overflow == "clip" {
        // Limit to roughly the visible area: one page worth (no explicit height limit yet)
        // For now, just prevent unbounded growth
        Some(100usize)
    } else {
        None
    });

    let truncated_lines: &[LineInfo] = if let Some(limit) = line_limit {
        if lines.len() > limit {
            &lines[..limit]
        } else {
            &lines
        }
    } else {
        &lines
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut y_curr: f64 = -(truncated_lines.len() as f64 * line_height_pts as f64) / 2.0; // start at top, centered vertically
    let mut pending_hardware = Vec::new();

    for (line_idx, line) in truncated_lines.iter().enumerate() {
        let mut x_curr: f64 = 0.0;

        // Compute line width from word widths
        let line_width = line.total_width;

        // Alignment needs a container width. With no wrap width there is no
        // container: using the wrap threshold (infinity) here produced
        // infinite offsets for center/right and infinite justify gaps. Align
        // against the line itself instead, which makes every offset zero and
        // leaves the block centered by `finalize_layout`.
        let alignment_width = if max_width.is_finite() && max_width > 0.0 {
            max_width as f64
        } else {
            line_width
        };

        // Compute x offset based on text_align
        let x_offset = match text_align {
            "center" => (alignment_width - line_width) / 2.0,
            "right" => alignment_width - line_width,
            "justify" => 0.0, // handled per-word below
            _ => 0.0,         // left
        };

        let is_last_line = line_idx == truncated_lines.len() - 1;
        let is_justify = text_align == "justify" && !is_last_line && line.words.len() > 1;
        let extra_space_per_gap = if is_justify {
            (alignment_width - line_width) / (line.words.len() - 1) as f64
        } else {
            0.0
        };

        for (wi_idx_in_line, &wi_idx) in line.words.iter().enumerate() {
            let wi = &word_infos[wi_idx];

            // Add space before word (except first)
            if wi_idx_in_line > 0 && wi.space_before {
                if is_justify {
                    x_curr += space_width + extra_space_per_gap;
                } else {
                    x_curr += space_width;
                }
            }

            // Render each glyph in the word using pre-computed offsets that include kerning
            emit_shaped_glyphs(
                &mut glyphs,
                &mut pending_hardware,
                &resolved.faces,
                &wi.glyphs,
                size,
                rgba,
                line_idx as u16,
                wi_idx as u16,
                x_curr + x_offset,
                y_curr,
            );
            // Advance x_curr by the total word width (includes kerning)
            x_curr += wi.width;
        }

        // Add ellipsis for overflow: "ellipsis" on last visible line if truncated
        if overflow == "ellipsis"
            && line_idx == truncated_lines.len() - 1
            && lines.len() > truncated_lines.len()
        {
            let (dot_w, dot_glyphs) = shape_token("…", &resolved.faces, size, 0.0);
            let (_ell_w, ell_glyphs) = if dot_glyphs.is_empty() || dot_glyphs[0].glyph_id == 0 {
                shape_token(".", &resolved.faces, size, 0.0)
            } else {
                (dot_w, dot_glyphs)
            };
            let last_w_idx = line.words.last().copied().unwrap_or(0) as u16;
            emit_shaped_glyphs(
                &mut glyphs,
                &mut pending_hardware,
                &resolved.faces,
                &ell_glyphs,
                size,
                rgba,
                line_idx as u16,
                last_w_idx,
                x_curr + x_offset,
                y_curr,
            );
        }

        y_curr += line_height_pts as f64;
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
