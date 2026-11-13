use animatix_core::error::RenderError;

use crate::font_context::FontContext;
use crate::{CompiledText, TextPath, f32_color_to_rgba8};

use super::layout::{ParagraphBidi, ShapedTextFragment, VisualLayoutItem};
use super::{
    FastPathFace, HardwarePendingGlyph, emit_shaped_glyphs, finalize_layout, parse_faces,
    resolve_faces, shape_fragment, space_advance, tokenize_words,
};

/// Container width used for per-line alignment when one exists.
fn alignment_width(max_width: f32, line_width: f64) -> f64 {
    if max_width.is_finite() && max_width > 0.0 {
        max_width as f64
    } else {
        line_width
    }
}

/// One measured word: total advance in scene coords, its shaped glyphs, and
/// whether a space separated it from the previous word in the paragraph.
struct WordInfo {
    width: f64,
    fragments: Vec<ShapedTextFragment>,
    space_before: bool,
    source_start: usize,
    source_end: usize,
}

/// One laid-out line: indices into the word table plus its total advance.
struct LineInfo {
    words: Vec<usize>,
    visual_items: Vec<VisualLayoutItem>,
    total_width: f64,
    source_start: usize,
    source_end: usize,
}

/// Typography shared by the measure and emit phases of the wrapped fast path.
struct LayoutParams<'a> {
    faces: &'a [FastPathFace<'a>],
    size: f32,
    letter_spacing: f32,
    space_width: f64,
    max_width: f32,
    line_height_pts: f32,
    rgba: [u8; 4],
}

/// Shape every word and break the paragraphs into lines at `max_width`.
fn measure_and_break(
    content: &str,
    params: &LayoutParams<'_>,
) -> Result<(Vec<WordInfo>, Vec<LineInfo>), RenderError> {
    let wrap_threshold = if params.max_width > 0.0 {
        params.max_width as f64
    } else {
        f64::INFINITY
    };

    let mut word_infos: Vec<WordInfo> = Vec::new();
    let mut lines: Vec<LineInfo> = Vec::new();

    let mut paragraph_char_start = 0usize;
    for paragraph in content.split('\n') {
        let p = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        let raw_tokens = tokenize_words(p);
        let paragraph_line_start = lines.len();
        let bidi = ParagraphBidi::new(p);

        if raw_tokens.is_empty() {
            lines.push(LineInfo {
                words: Vec::new(),
                visual_items: Vec::new(),
                total_width: 0.0,
                source_start: 0,
                source_end: 0,
            });
            paragraph_char_start += paragraph.chars().count() + 1;
            continue;
        }

        let mut paragraph_word_indices = Vec::with_capacity(raw_tokens.len());
        for token in &raw_tokens {
            let mut fragments = Vec::new();
            let mut token_width = 0.0;
            for fragment in bidi.text_fragments(token.byte_start..token.byte_end) {
                let local = fragment.range.start - token.byte_start;
                let source_char =
                    paragraph_char_start + token.char_start + token.text[..local].chars().count();
                let direction = if fragment.rtl {
                    rustybuzz::Direction::RightToLeft
                } else {
                    rustybuzz::Direction::LeftToRight
                };
                let (width, glyphs) = shape_fragment(
                    &p[fragment.range.clone()],
                    source_char,
                    params.faces,
                    params.size,
                    params.letter_spacing,
                    direction,
                );
                token_width += width;
                fragments.push(ShapedTextFragment {
                    range: fragment.range,
                    width,
                    glyphs,
                });
            }
            let wi_idx = word_infos.len();
            word_infos.push(WordInfo {
                width: token_width,
                fragments,
                space_before: token.space_before,
                source_start: token.byte_start,
                source_end: token.byte_end,
            });
            paragraph_word_indices.push(wi_idx);
        }

        let mut current_line = LineInfo {
            words: Vec::new(),
            visual_items: Vec::new(),
            total_width: 0.0,
            source_start: 0,
            source_end: 0,
        };

        for &wi_idx in &paragraph_word_indices {
            let wi = &word_infos[wi_idx];
            let space_needed = if current_line.words.is_empty() {
                0.0
            } else if wi.space_before {
                params.space_width
            } else {
                0.0
            };

            if current_line.total_width + space_needed + wi.width > wrap_threshold
                && !current_line.words.is_empty()
            {
                lines.push(current_line);
                current_line = LineInfo {
                    words: Vec::new(),
                    visual_items: Vec::new(),
                    total_width: 0.0,
                    source_start: 0,
                    source_end: 0,
                };
            }

            if !current_line.words.is_empty() && wi.space_before {
                current_line.total_width += params.space_width;
            }
            if current_line.words.is_empty() {
                current_line.source_start = wi.source_start;
            }
            current_line.source_end = wi.source_end;
            current_line.words.push(wi_idx);
            current_line.total_width += wi.width;
        }
        if !current_line.words.is_empty() {
            lines.push(current_line);
        }

        for line in &mut lines[paragraph_line_start..] {
            let mut items = Vec::new();
            let mut ranges = Vec::new();
            for (line_word_idx, &word_idx) in line.words.iter().enumerate() {
                let word = &word_infos[word_idx];
                if line_word_idx > 0 && word.space_before {
                    let previous = &word_infos[line.words[line_word_idx - 1]];
                    let gap = previous.source_end..word.source_start;
                    if !gap.is_empty() {
                        if !bidi.range_has_single_level(gap.clone()) {
                            return Err(RenderError::TextCompilation(
                                "inter-word whitespace crosses bidi levels".into(),
                            ));
                        }
                        items.push(VisualLayoutItem::Gap);
                        ranges.push((gap.start, gap.end));
                    }
                }
                for (fragment_idx, fragment) in word.fragments.iter().enumerate() {
                    items.push(VisualLayoutItem::Text {
                        word_idx,
                        fragment_idx,
                    });
                    ranges.push((fragment.range.start, fragment.range.end));
                }
            }
            let order =
                bidi.visual_order_for_ranges(&ranges, line.source_start..line.source_end)?;
            line.visual_items = order.into_iter().map(|index| items[index]).collect();
        }
        paragraph_char_start += paragraph.chars().count() + 1;
    }

    Ok((word_infos, lines))
}

/// Emit centered glyph paths for the laid-out lines: per-line alignment,
/// justification, truncation by `max_lines`/overflow guard, and the ellipsis
/// marker on the last surviving line.
fn emit_lines(
    word_infos: &[WordInfo],
    lines: &[LineInfo],
    params: &LayoutParams<'_>,
    max_lines: Option<usize>,
    text_align: &str,
    overflow: &str,
) -> (Vec<TextPath>, Vec<HardwarePendingGlyph>) {
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
            lines
        }
    } else {
        lines
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut y_curr: f64 = -(truncated_lines.len() as f64 * params.line_height_pts as f64) / 2.0; // start at top, centered vertically
    let mut pending_hardware = Vec::new();

    for (line_idx, line) in truncated_lines.iter().enumerate() {
        let mut x_curr: f64 = 0.0;
        let line_width = line.total_width;
        let alignment_width = alignment_width(params.max_width, line_width);
        let x_offset = match text_align {
            "center" => (alignment_width - line_width) / 2.0,
            "right" => alignment_width - line_width,
            "justify" => 0.0, // handled per-word below
            _ => 0.0,         // left
        };
        let is_last_line = line_idx == truncated_lines.len() - 1;
        let gap_count = line
            .visual_items
            .iter()
            .filter(|item| matches!(item, VisualLayoutItem::Gap))
            .count();
        let is_justify = text_align == "justify" && !is_last_line && gap_count > 0;
        let extra_space_per_gap = if is_justify {
            (alignment_width - line_width) / gap_count as f64
        } else {
            0.0
        };

        for item in &line.visual_items {
            match *item {
                VisualLayoutItem::Gap => {
                    x_curr += params.space_width + extra_space_per_gap;
                },
                VisualLayoutItem::Text {
                    word_idx,
                    fragment_idx,
                } => {
                    let word = &word_infos[word_idx];
                    let fragment = &word.fragments[fragment_idx];
                    emit_shaped_glyphs(
                        &mut glyphs,
                        &mut pending_hardware,
                        params.faces,
                        &fragment.glyphs,
                        params.size,
                        params.rgba,
                        line_idx as u16,
                        word_idx as u16,
                        x_curr + x_offset,
                        y_curr,
                    );
                    x_curr += fragment.width;
                },
            }
        }

        // Add ellipsis for overflow: "ellipsis" on last visible line if truncated
        if overflow == "ellipsis"
            && line_idx == truncated_lines.len() - 1
            && lines.len() > truncated_lines.len()
        {
            let (dot_w, dot_glyphs) = shape_fragment(
                "…",
                0,
                params.faces,
                params.size,
                0.0,
                rustybuzz::Direction::LeftToRight,
            );
            let (_ell_w, ell_glyphs) = if dot_glyphs.is_empty() || dot_glyphs[0].glyph_id == 0 {
                shape_fragment(
                    ".",
                    0,
                    params.faces,
                    params.size,
                    0.0,
                    rustybuzz::Direction::LeftToRight,
                )
            } else {
                (dot_w, dot_glyphs)
            };
            let last_w_idx = line.words.last().copied().unwrap_or(0) as u16;
            emit_shaped_glyphs(
                &mut glyphs,
                &mut pending_hardware,
                params.faces,
                &ell_glyphs,
                params.size,
                params.rgba,
                line_idx as u16,
                last_w_idx,
                x_curr + x_offset,
                y_curr,
            );
        }

        y_curr += params.line_height_pts as f64;
    }

    (glyphs, pending_hardware)
}

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
    let sources = resolve_faces(content, family, weight, style, font_ctx, "fast path wrapped")?;
    let faces = parse_faces(&sources, weight, style);
    let Some(primary) = faces.first().map(|face| &face.ttf) else {
        return Err(RenderError::TextCompilation("No usable font face for ordinary text".into()));
    };
    let font_scale = size / primary.units_per_em() as f32;
    let ascent = primary.ascender() as f32;
    let descent = primary.descender() as f32;
    let line_gap = primary.line_gap() as f32;

    // Line height in absolute points
    let lh_mult = if line_height.is_finite() && line_height > 0.0 {
        line_height
    } else {
        1.2
    };
    let line_height_pts = (ascent - descent + line_gap) * font_scale * lh_mult;

    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    let rgba = f32_color_to_rgba8(&color);
    let space_width = space_advance(&faces, size, letter_spacing, word_spacing);
    let params = LayoutParams {
        faces: &faces,
        size,
        letter_spacing,
        space_width,
        max_width,
        line_height_pts,
        rgba,
    };

    let (word_infos, lines) = measure_and_break(content, &params)?;
    if word_infos.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    let (glyphs, pending_hardware) =
        emit_lines(&word_infos, &lines, &params, max_lines, text_align, overflow);

    Ok(finalize_layout(
        glyphs,
        pending_hardware,
        &faces,
        size,
        ascent * font_scale,
        descent * font_scale,
    ))
}
