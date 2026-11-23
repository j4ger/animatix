//! This entire implementation is reachable only through the rich factory.

use crate::typst_world::{
    compile_code, compile_typst, compile_typst_with_sources, extract_glyphs_grouped,
    extract_glyphs_with_metrics,
};
use crate::{CachedGroupedText, CompiledText, FontContext, TextCompileRequest, TextKind};
use animatix_core::error::RenderError;
use std::collections::BTreeSet;
use typst::layout::{Frame, FrameItem};

fn validate_frame(frame: &Frame) -> Result<(), RenderError> {
    use typst::visualize::{FillRule, Geometry, Paint};

    let mut missing = BTreeSet::new();
    let mut stack = vec![frame];
    while let Some(current) = stack.pop() {
        for (_, item) in current.items() {
            match item {
                FrameItem::Group(group) => stack.push(&group.frame),
                FrameItem::Text(text) => {
                    if !matches!(text.fill, Paint::Solid(_)) {
                        return Err(RenderError::TextCompilation(
                            "Typst text requires a solid fill in Animatix".into(),
                        ));
                    }
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
                FrameItem::Shape(shape, _) => {
                    let non_solid =
                        shape.fill.as_ref().is_some_and(|paint| !matches!(paint, Paint::Solid(_)))
                            || shape
                                .stroke
                                .as_ref()
                                .is_some_and(|stroke| !matches!(stroke.paint, Paint::Solid(_)));
                    if non_solid {
                        return Err(RenderError::TextCompilation(
                            "Typst shapes require solid fills and strokes in Animatix".into(),
                        ));
                    }
                    if shape.fill.is_some()
                        && shape.fill_rule == FillRule::EvenOdd
                        && matches!(shape.geometry, Geometry::Curve(_))
                    {
                        return Err(RenderError::TextCompilation(
                            "Typst even-odd curve fills are unavailable in Animatix".into(),
                        ));
                    }
                },
                FrameItem::Image(..) => {
                    return Err(RenderError::TextCompilation(
                        "Typst image output is unavailable in Animatix; use an Image or Svg actor"
                            .into(),
                    ));
                },
                FrameItem::Link(..) | FrameItem::Tag(_) => {}, // Metadata has no visual ink.
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

fn fit_rich_ellipsis(
    compiled: &mut CompiledText,
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<(), RenderError> {
    let width = f64::from(request.max_width);
    let style = &request.style;
    let mut marker = None;
    let mut fitted = Vec::with_capacity(compiled.glyphs.len());
    let glyphs = std::mem::take(&mut compiled.glyphs);

    for row in glyphs.chunk_by(|a, b| a.line_idx == b.line_idx) {
        let ink = row.iter().filter(|g| g.opacity > 0.0);
        let Some(bounds) = ink.clone().map(|g| g.bounds).reduce(|a, b| a.union(b)) else {
            continue;
        };
        let start = fitted.len();
        if bounds.width() <= width {
            fitted.extend(ink.cloned());
        } else {
            if marker.is_none() {
                let display_size = if request.kind == TextKind::Code {
                    style.font_size * 0.8
                } else {
                    style.font_size
                };
                let frame = compile_typst(
                    "…",
                    display_size,
                    color(request),
                    style.font_family,
                    fonts,
                    style.font_weight,
                    style.font_style,
                    style.line_height,
                    0.0,
                    0.0,
                    0.0,
                    "left",
                    "visible",
                )?;
                validate_frame(&frame)?;
                marker = Some(extract_glyphs_with_metrics(&frame).glyphs);
            }
            let dots = marker.as_deref().unwrap_or_default();
            let dot_width = f64::from(crate::measure_text_paths(dots)[0]) * 2.0;
            let available_end = bounds.x0 + width - dot_width;
            fitted.extend(ink.take_while(|g| g.bounds.x1 <= available_end).cloned());
            if dot_width <= width {
                let last_x = fitted[start..].iter().fold(bounds.x0, |x, g| x.max(g.bounds.x1));
                for dot in dots {
                    let mut dot = dot.clone();
                    dot.translate((last_x + dot_width / 2.0, row[0].baseline_y - dot.baseline_y));
                    dot.line_idx = row[0].line_idx;
                    fitted.push(dot);
                }
            }
        }
        if let Some(ink) = fitted[start..].iter().map(|g| g.bounds).reduce(|a, b| a.union(b)) {
            let shift = match request.text_align {
                "center" => (width - ink.width()) / 2.0,
                "right" => width - ink.width(),
                _ => 0.0,
            };
            for glyph in &mut fitted[start..] {
                glyph.translate((shift - ink.x0, 0.0));
            }
        }
    }
    compiled.baseline_offset += crate::center_text_paths(&mut fitted);
    compiled.glyphs = fitted;
    Ok(())
}

pub(super) fn compile(
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<CompiledText, RenderError> {
    let style = &request.style;
    let ellipsis = request.overflow == "ellipsis" && request.max_width > 0.0;
    let max_width = if ellipsis { 0.0 } else { request.max_width };
    let text_align = if ellipsis { "left" } else { request.text_align };
    if request.max_lines.is_some() && request.max_width > 0.0 {
        tracing::warn!("max_lines is unavailable for rich text; the line cap is ignored");
    }
    let frame = match request.kind {
        TextKind::Code => compile_code(request, color(request), fonts, max_width, text_align)?,
        TextKind::Math | TextKind::Typst => compile_typst_with_sources(
            &math_or_markup(request),
            style.font_size,
            color(request),
            style.font_family,
            fonts,
            style.font_weight,
            style.font_style,
            style.line_height,
            style.letter_spacing,
            style.word_spacing,
            max_width,
            text_align,
            request.overflow,
            request.typst_sources,
        )?,
        TextKind::Text => {
            return Err(RenderError::TextCompilation(
                "literal Text does not use the rich compiler".to_string(),
            ));
        },
    };
    validate_frame(&frame)?;
    let mut compiled = extract_glyphs_with_metrics(&frame);
    if request.kind == TextKind::Code && request.code.track_lines {
        let (mut paths, ranges) = extract_glyphs_grouped(&frame)?;
        for (line, range) in ranges.into_iter().enumerate() {
            for path in &mut paths[range] {
                path.source_line = u32::try_from(line).ok();
            }
        }
        compiled.glyphs = paths;
    }
    if ellipsis {
        fit_rich_ellipsis(&mut compiled, request, fonts)?;
    }
    Ok(compiled)
}

fn math_or_markup<'a>(request: &TextCompileRequest<'a>) -> std::borrow::Cow<'a, str> {
    match request.kind {
        TextKind::Math => format!("$ {} $", request.content).into(),
        _ => request.content.into(),
    }
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
    validate_frame(&frame)?;
    let (glyphs, ranges) = extract_glyphs_grouped(&frame)?;
    Ok(CachedGroupedText {
        glyphs: glyphs.into(),
        ranges: ranges.into(),
    })
}
