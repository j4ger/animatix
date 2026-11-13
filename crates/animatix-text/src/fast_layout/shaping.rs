use animatix_core::error::RenderError;

use crate::font_context::FontContext;
use crate::{CompiledText, TextPath, f32_color_to_rgba8};

use super::layout::{ParagraphBidi, ShapedTextFragment, VisualLayoutItem};
use super::wrapped::compile_text_fast_wrapped;
use super::{
    emit_shaped_glyphs, finalize_layout, parse_faces, resolve_faces, shape_fragment, space_advance,
    tokenize_words,
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

    let sources = resolve_faces(content, family, weight, style, font_ctx, "fast path")?;
    let faces = parse_faces(&sources, weight, style);
    let Some(primary) = faces.first().map(|face| &face.ttf) else {
        return Err(RenderError::TextCompilation("No usable font face for ordinary text".into()));
    };
    let font_scale = size / primary.units_per_em() as f32;
    let ascent = primary.ascender() as f32 * font_scale;
    let descent = primary.descender() as f32 * font_scale;
    let rgba = f32_color_to_rgba8(&color);
    let space_width = space_advance(&faces, size, letter_spacing, word_spacing);

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut pending_hardware = Vec::new();
    let mut x_curr: f64 = 0.0;

    let tokens = tokenize_words(content);
    let bidi = ParagraphBidi::new(content);
    let mut shaped_tokens: Vec<Vec<ShapedTextFragment>> = Vec::with_capacity(tokens.len());
    for token in &tokens {
        let mut shaped_fragments = Vec::new();
        for fragment in bidi.text_fragments(token.byte_start..token.byte_end) {
            let local = fragment.range.start - token.byte_start;
            let source_char = token.char_start + token.text[..local].chars().count();
            let direction = if fragment.rtl {
                rustybuzz::Direction::RightToLeft
            } else {
                rustybuzz::Direction::LeftToRight
            };
            let (width, fragment_glyphs) = shape_fragment(
                &content[fragment.range.clone()],
                source_char,
                &faces,
                size,
                letter_spacing,
                direction,
            );
            shaped_fragments.push(ShapedTextFragment {
                range: fragment.range,
                width,
                glyphs: fragment_glyphs,
            });
        }
        shaped_tokens.push(shaped_fragments);
    }

    let mut items = Vec::new();
    let mut ranges = Vec::new();
    for (token_idx, token) in tokens.iter().enumerate() {
        if token_idx > 0 && token.space_before {
            let gap = tokens[token_idx - 1].byte_end..token.byte_start;
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
        for (fragment_idx, fragment) in shaped_tokens[token_idx].iter().enumerate() {
            items.push(VisualLayoutItem::Text {
                word_idx: token_idx,
                fragment_idx,
            });
            ranges.push((fragment.range.start, fragment.range.end));
        }
    }
    for item_idx in bidi.visual_order_for_ranges(&ranges, 0..content.len())? {
        match items[item_idx] {
            VisualLayoutItem::Gap => x_curr += space_width,
            VisualLayoutItem::Text {
                word_idx,
                fragment_idx,
            } => {
                let fragment = &shaped_tokens[word_idx][fragment_idx];
                emit_shaped_glyphs(
                    &mut glyphs,
                    &mut pending_hardware,
                    &faces,
                    &fragment.glyphs,
                    size,
                    rgba,
                    0,
                    word_idx.min(u16::MAX as usize) as u16,
                    x_curr,
                    0.0,
                );
                x_curr += fragment.width;
            },
        }
    }

    Ok(finalize_layout(glyphs, pending_hardware, &faces, size, ascent, descent))
}
