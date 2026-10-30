#![cfg(feature = "rich-text")]

use super::*;

fn white() -> typst::visualize::Color {
    typst::visualize::Color::from_u8(255, 255, 255, 255)
}

/// The shared `emit_text_item` helper assigns char/word/line indices in
/// emission order; this pins the indices the renderer uses for per-word and
/// per-line effects.
#[cfg(feature = "rich-text")]
#[test]
fn extract_glyphs_preserves_char_word_and_line_indices() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "Hello world\nSecond line",
        24.0,
        white(),
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("text compiles");

    let glyphs = extract_glyphs(&frame);
    assert!(!glyphs.is_empty(), "fixture must emit glyphs");

    for (index, glyph) in glyphs.iter().enumerate() {
        assert_eq!(glyph.char_idx as usize, index, "char_idx must follow emission order");
        assert_eq!(glyph.opacity, 1.0, "extracted glyphs are fully opaque");
        assert_eq!(glyph.color, [255, 255, 255, 255], "default paint must be white");
    }
    assert_eq!(glyphs[0].word_idx, 0, "first glyph belongs to the first word");
    assert!(
        glyphs.iter().any(|g| g.word_idx > 0),
        "expected a word boundary between the two words"
    );
    assert!(
        glyphs.iter().any(|g| g.line_idx == 1),
        "expected a second line, got line indices {:?}",
        glyphs.iter().map(|g| g.line_idx).collect::<Vec<_>>()
    );
    assert!(
        glyphs.iter().all(|g| g.line_idx <= 1),
        "fixture must not produce more than two lines"
    );
}

/// `FrameItem::Text` paint is converted through `paint_to_rgba`; the grouped
/// extractor must keep that paint and its per-box ranges while sharing the
/// index assignment with the flat extractors.
#[cfg(feature = "rich-text")]
#[test]
fn extract_grouped_glyphs_preserves_paint_and_ranges() {
    let font_ctx = test_font_ctx();
    let frame = compile_typst(
        "#box()[#text(fill: rgb(\"#ff0000\"))[Red]] #box()[#text(fill: rgb(\"#0000ff\"))[Blue]]",
        24.0,
        white(),
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("colored text compiles");

    let (glyphs, ranges) = extract_glyphs_grouped(&frame);
    assert!(!glyphs.is_empty(), "fixture must emit glyphs");
    assert_eq!(ranges.len(), 2, "one range per non-empty #box() group");
    assert_eq!(ranges[0].start, 0, "ranges must start at the first glyph");
    assert!(
        ranges.windows(2).all(|pair| pair[0].end == pair[1].start),
        "ranges must be contiguous: {ranges:?}"
    );
    assert_eq!(
        ranges.last().map(|range| range.end),
        Some(glyphs.len()),
        "the last range must cover the final glyph"
    );

    for (index, glyph) in glyphs.iter().enumerate() {
        assert_eq!(glyph.char_idx as usize, index, "grouped char_idx must follow emission order");
        assert_eq!(glyph.opacity, 1.0);
    }
    assert!(
        glyphs.iter().any(|g| g.color == [255, 0, 0, 255]),
        "red #text(fill:) must be preserved, got {:?}",
        glyphs.iter().map(|g| g.color).collect::<Vec<_>>()
    );
    assert!(
        glyphs.iter().any(|g| g.color == [0, 0, 255, 255]),
        "blue #text(fill:) must be preserved, got {:?}",
        glyphs.iter().map(|g| g.color).collect::<Vec<_>>()
    );

    // The flat extractors share the same walker and paint conversion.
    for flat in [
        extract_glyphs(&frame),
        extract_glyphs_with_metrics(&frame).glyphs,
    ] {
        assert!(flat.iter().any(|g| g.color == [255, 0, 0, 255]));
        assert!(flat.iter().any(|g| g.color == [0, 0, 255, 255]));
    }
}

/// `#box()` is optional: top-level text items (raw text, not markup groups)
/// still form one implicit range covering their glyphs.
#[test]
fn grouped_extraction_collects_top_level_text_runs() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "Top level text",
        24.0,
        white(),
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("text compiles");

    let (glyphs, ranges) = extract_glyphs_grouped(&frame);
    assert!(!glyphs.is_empty(), "fixture must emit glyphs");
    assert_eq!(ranges.len(), 1, "top-level text should form one implicit group");
    assert_eq!(ranges[0], 0..glyphs.len(), "the implicit range must cover every glyph");
    for (index, glyph) in glyphs.iter().enumerate() {
        assert_eq!(glyph.char_idx as usize, index, "char_idx must follow emission order");
        assert_eq!(glyph.color, [255, 255, 255, 255]);
    }
}
