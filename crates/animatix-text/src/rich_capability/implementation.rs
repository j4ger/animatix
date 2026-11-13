//! This entire implementation is reachable only through the rich factory.

use crate::typst_world::{
    compile_code, compile_math, compile_typst, extract_glyphs_grouped, extract_glyphs_with_metrics,
};
use crate::{CachedGroupedText, CompiledText, FontContext, TextCompileRequest, TextKind};
use animatix_core::error::RenderError;
use std::collections::BTreeSet;
use typst::layout::{Frame, FrameItem};

fn validate_missing_glyphs(frame: &Frame) -> Result<(), RenderError> {
    let mut missing = BTreeSet::new();
    let mut stack = vec![frame];
    while let Some(current) = stack.pop() {
        for (_, item) in current.items() {
            match item {
                FrameItem::Group(group) => stack.push(&group.frame),
                FrameItem::Text(text) => {
                    for glyph in text.glyphs.iter().filter(|glyph| glyph.id == 0) {
                        let Some(cluster) = text.text.as_str().get(glyph.range()) else {
                            tracing::debug!(range = ?glyph.range(), "Typst tofu glyph range did not map to a text cluster");
                            continue;
                        };
                        missing.extend(
                            cluster
                                .chars()
                                .filter(|ch| {
                                    !ch.is_whitespace()
                                        && !crate::fast_layout::default_ignorable(*ch)
                                })
                                .map(u32::from),
                        );
                    }
                },
                _ => {}, // Non-text frame items do not contain font glyphs.
            }
        }
    }

    if missing.is_empty() {
        return Ok(());
    }
    let codepoints = missing
        .iter()
        .map(|codepoint| format!("U+{codepoint:04X}"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(RenderError::TextCompilation(format!(
        "Typst output contains unsupported glyphs ({codepoints}); make a prepared font covering these characters available"
    )))
}

fn color(request: &TextCompileRequest<'_>) -> typst::visualize::Color {
    let [r, g, b, a] = request.solid_rgba();
    typst::visualize::Color::from_u8(r, g, b, a)
}

pub(super) fn compile(
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<CompiledText, RenderError> {
    let style = &request.style;
    if request.max_lines.is_some() && request.max_width > 0.0 {
        tracing::warn!("max_lines is unavailable for rich text; the line cap is ignored");
    }
    let frame = match request.kind {
        TextKind::Math => compile_math(
            request.content,
            style.font_size,
            color(request),
            style.font_family,
            fonts,
            request.max_width,
            request.text_align,
            request.overflow,
        )?,
        TextKind::Code => compile_code(
            request.content,
            request.language,
            request.palette,
            style.font_size,
            color(request),
            style.font_family,
            fonts,
            style.font_weight,
            style.font_style,
            style.line_height,
            style.letter_spacing,
            style.word_spacing,
            request.max_width,
            request.text_align,
            request.overflow,
        )?,
        TextKind::Typst => compile_typst(
            request.content,
            style.font_size,
            color(request),
            style.font_family,
            fonts,
            style.font_weight,
            style.font_style,
            style.line_height,
            style.letter_spacing,
            style.word_spacing,
            request.max_width,
            request.text_align,
            request.overflow,
        )?,
        TextKind::Text => {
            return Err(RenderError::TextCompilation(
                "literal Text does not use the rich compiler".to_string(),
            ));
        },
    };
    validate_missing_glyphs(&frame)?;
    Ok(extract_glyphs_with_metrics(&frame))
}

pub(super) fn grouped(
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<CachedGroupedText, RenderError> {
    let style = &request.style;
    let frame = compile_typst(
        request.content,
        style.font_size,
        color(request),
        style.font_family,
        fonts,
        style.font_weight,
        style.font_style,
        style.line_height,
        style.letter_spacing,
        style.word_spacing,
        0.0,
        "left",
        "visible",
    )?;
    validate_missing_glyphs(&frame)?;
    let (glyphs, ranges) = extract_glyphs_grouped(&frame)?;
    Ok(CachedGroupedText {
        glyphs: glyphs.into(),
        ranges: ranges.into(),
    })
}
