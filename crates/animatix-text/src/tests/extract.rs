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

    let (glyphs, ranges) = extract_glyphs_grouped(&frame).expect("legacy grouped extraction");
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

    let (glyphs, ranges) = extract_glyphs_grouped(&frame).expect("legacy grouped extraction");
    assert!(!glyphs.is_empty(), "fixture must emit glyphs");
    assert_eq!(ranges.len(), 1, "top-level text should form one implicit group");
    assert_eq!(ranges[0], 0..glyphs.len(), "the implicit range must cover every glyph");
    for (index, glyph) in glyphs.iter().enumerate() {
        assert_eq!(glyph.char_idx as usize, index, "char_idx must follow emission order");
        assert_eq!(glyph.color, [255, 255, 255, 255]);
    }
}

/// Explicit frame metadata keeps Equation fragment ranges stable through
/// Typst's transparent page and inline-math groups without changing layout.
#[test]
fn grouped_extraction_preserves_marked_math_fragments_and_geometry() {
    let mut fonts = FontContext::empty();
    fonts.load_fonts([
        include_bytes!("../../assets/fonts/OpenSans-Variable.ttf").to_vec(),
        include_bytes!("../../assets/fonts/FiraMath-Regular.otf").to_vec(),
    ]);
    let content = ["$E$", "$=$", "$m$", "$c^2$"];
    let unmarked = content
        .iter()
        .map(|fragment| format!("#box()[{fragment}]"))
        .collect::<Vec<_>>()
        .join(" ");
    let marked = content
        .iter()
        .enumerate()
        .map(|(index, fragment)| {
            format!(
                "#metadata(\"animatix:equation-fragment:start:{index}\") #box()[{fragment}] #metadata(\"animatix:equation-fragment:end:{index}\")"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let compile = |markup: &str| {
        compile_typst(
            markup,
            48.0,
            white(),
            "Open Sans",
            &fonts,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            0.0,
            "left",
            "visible",
        )
        .expect("math fragments compile with explicit fonts")
    };
    let unmarked_frame = compile(&unmarked);
    let marked_frame = compile(&marked);
    let flat = extract_glyphs(&unmarked_frame);
    let (glyphs, ranges) =
        extract_glyphs_grouped(&marked_frame).expect("Equation markers are balanced");

    assert_eq!(ranges.len(), 4, "one range per authored Fragment");
    assert_eq!(ranges.last().map(|range| range.end), Some(glyphs.len()));
    assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
    assert!(ranges.iter().all(|range| !range.is_empty()));
    assert_eq!(glyphs.len(), 5, "math relation and superscript must be glyphs");
    let bounds = |paths: &[TextPath]| {
        paths
            .iter()
            .map(|path| kurbo::Shape::bounding_box(&path.path))
            .collect::<Vec<_>>()
    };
    assert_eq!(bounds(&glyphs), bounds(&flat), "metadata must not alter glyph layout");
}

/// Nested groups created by styling a fragment remain inside its one authored
/// range; the metadata, not incidental Group depth, defines the boundary.
#[test]
fn grouped_extraction_keeps_nested_styled_content_in_one_range() {
    let mut fonts = FontContext::empty();
    fonts.load_fonts([
        include_bytes!("../../assets/fonts/OpenSans-Variable.ttf").to_vec(),
        include_bytes!("../../assets/fonts/FiraMath-Regular.otf").to_vec(),
    ]);
    let frame = compile_typst(
        "#metadata(\"animatix:equation-fragment:start:0\") #box()[#text(fill: rgb(\"#ff0000\"))[x #box()[y]]] #metadata(\"animatix:equation-fragment:end:0\")",
        24.0,
        white(),
        "Open Sans",
        &fonts,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("styled fragment compiles");
    let (glyphs, ranges) = extract_glyphs_grouped(&frame).expect("fragment marker is balanced");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], 0..glyphs.len());
    assert_eq!(glyphs.len(), 2);
}

#[test]
fn grouped_extraction_rejects_mismatched_fragment_ordinals() {
    let frame = compile_typst(
        "#metadata(\"animatix:equation-fragment:start:0\") #box()[x] #metadata(\"animatix:equation-fragment:end:1\")",
        24.0,
        white(),
        "Open Sans",
        &test_font_ctx(),
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("mismatched marker fixture compiles");
    let error = extract_glyphs_grouped(&frame).expect_err("mismatched ordinals must fail");
    assert!(error.to_string().contains("metadata"), "unexpected error: {error}");
}

#[test]
fn grouped_extraction_retains_empty_marked_fragment_ranges() {
    let frame = compile_typst(
        "#metadata(\"animatix:equation-fragment:start:0\") #box()[] #metadata(\"animatix:equation-fragment:end:0\")",
        24.0,
        white(),
        "Open Sans",
        &test_font_ctx(),
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        0.0,
        "left",
        "visible",
    )
    .expect("empty fragment compiles");
    let (glyphs, ranges) = extract_glyphs_grouped(&frame).expect("empty marker pair is balanced");
    assert!(glyphs.is_empty());
    assert_eq!(ranges.len(), 1, "the empty authored fragment keeps its ordinal");
    assert_eq!(ranges[0], 0..0);
}
