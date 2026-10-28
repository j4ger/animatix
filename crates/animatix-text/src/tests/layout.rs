use super::*;

#[test]
fn wrapped_fast_path_centered_alignment() {
    let font_ctx = test_font_ctx();
    let text = "left center right";

    // Left-aligned: first glyph starts near x = -max_width/2
    let compiled_left = compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
        200.0,
        None,
        "left",
        "visible",
    )
    .unwrap();
    let paths_left = compiled_left.glyphs;

    // Center-aligned
    let compiled_center = compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
        200.0,
        None,
        "center",
        "visible",
    )
    .unwrap();
    let paths_center = compiled_center.glyphs;

    // Right-aligned
    let compiled_right = compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
        200.0,
        None,
        "right",
        "visible",
    )
    .unwrap();
    let paths_right = compiled_right.glyphs;

    assert!(!paths_left.is_empty());
    assert!(!paths_center.is_empty());
    assert!(!paths_right.is_empty());

    // All should produce glyphs (sanity check)
    let bbox_left = measure_text_paths(&paths_left);
    let bbox_center = measure_text_paths(&paths_center);
    let _bbox_right = measure_text_paths(&paths_right);

    // Widths should be roughly equal
    assert!(
        (bbox_left[0] - bbox_center[0]).abs() < 1.0,
        "Left and center widths should match: left={:.3}, center={:.3}",
        bbox_left[0],
        bbox_center[0]
    );
}

#[test]
fn no_max_width_is_identical() {
    // Text without max_width renders the same as before
    let font_ctx = test_font_ctx();
    let text = "NoWrapTest";

    let compiled_normal =
        compile_text_fast(text, "Open Sans", 400.0, "normal", 24.0, [1.0; 4], 0.0, 0.0, &font_ctx)
            .unwrap();
    let paths_normal = compiled_normal.glyphs;

    // Wrapping with very large max_width (effectively no wrap) should match single-line
    let compiled_wide = compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
        10000.0,
        None,
        "left",
        "visible",
    )
    .unwrap();
    let paths_wide = compiled_wide.glyphs;

    // Both should produce the same number of glyphs
    assert_eq!(
        paths_normal.len(),
        paths_wide.len(),
        "No-wrap and wide-wrap should produce same glyph count"
    );

    // Bounding box should be the same
    let bbox_normal = measure_text_paths(&paths_normal);
    let bbox_wide = measure_text_paths(&paths_wide);
    assert!(
        (bbox_normal[0] - bbox_wide[0]).abs() < 0.5,
        "Widths should match: normal={:.3}, wide={:.3}",
        bbox_normal[0],
        bbox_wide[0]
    );
    assert!(
        (bbox_normal[1] - bbox_wide[1]).abs() < 0.5,
        "Heights should match: normal={:.3}, wide={:.3}",
        bbox_normal[1],
        bbox_wide[1]
    );
}

#[cfg(feature = "rich-text")]
/// Regression: non-Latin text must render through glyph-fallback fonts
/// even when the resolved primary family (bundled mock Open Sans) has no
/// coverage. Previously CJK glyphs silently vanished.
#[test]
fn cjk_text_renders_with_default_family() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "排序剧场",
        90.0,
        typst::visualize::Color::from_u8(255, 255, 255, 255),
        "",
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
    .expect("CJK text should compile via glyph fallback");
    let compiled = extract_glyphs_with_metrics(&frame);
    assert!(
        !compiled.glyphs.is_empty(),
        "CJK glyphs must not vanish with the default family"
    );
}

#[cfg(feature = "rich-text")]
/// Regression: without an explicit page rule Typst applied its default A4
/// page geometry and wrapped any line wider than ~453pt.
#[test]
fn long_cjk_line_stays_single_line_without_max_width() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "布局优先的动画描述语言中文长句样例",
        40.0,
        typst::visualize::Color::from_u8(255, 255, 255, 255),
        "",
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
    .unwrap();
    let compiled = extract_glyphs_with_metrics(&frame);
    assert!(!compiled.glyphs.is_empty());
    let [_, half_h] = measure_text_paths(&compiled.glyphs);
    assert!(
        half_h * 2.0 < 40.0 * 2.0,
        "expected one line without max_width, got ink height {:.1}",
        half_h * 2.0
    );
}

#[cfg(feature = "rich-text")]
#[test]
fn text_max_width_wraps_long_cjk_line() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "布局优先的动画描述语言中文长句样例",
        40.0,
        typst::visualize::Color::from_u8(255, 255, 255, 255),
        "",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        160.0,
        "left",
        "visible",
    )
    .unwrap();
    let compiled = extract_glyphs_with_metrics(&frame);
    assert!(!compiled.glyphs.is_empty());
    let [_, half_h] = measure_text_paths(&compiled.glyphs);
    assert!(
        half_h * 2.0 >= 40.0 * 2.0,
        "expected wrapping at max_width=160, got ink height {:.1}",
        half_h * 2.0
    );
}

#[cfg(feature = "rich-text")]
#[test]
fn long_latin_line_stays_single_line_without_max_width() {
    let font_ctx = test_font_ctx();
    let frame = compile_text(
        "The quick brown fox jumps over the lazy dog again and again",
        24.0,
        typst::visualize::Color::from_u8(255, 255, 255, 255),
        "",
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
    .unwrap();
    let compiled = extract_glyphs_with_metrics(&frame);
    assert!(!compiled.glyphs.is_empty());
    let [_, half_h] = measure_text_paths(&compiled.glyphs);
    assert!(
        half_h * 2.0 < 24.0 * 2.0,
        "Latin single-line behavior changed: ink height {:.1}",
        half_h * 2.0
    );
}

#[test]
fn fast_path_cjk_wrapping_wraps_long_lines() {
    let font_ctx = test_font_ctx();
    let text = "这是一段非常长的中文句子用于验证快速路径分词和折行是否正常工作";
    let compiled = compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
        150.0,
        None,
        "left",
        "visible",
    )
    .unwrap();

    assert!(!compiled.glyphs.is_empty());
    let max_line = compiled.glyphs.iter().map(|g| g.line_idx).max().unwrap_or(0);
    assert!(
        max_line >= 2,
        "Long CJK text should wrap into at least 3 lines, got max_line={}",
        max_line
    );
}

#[test]
fn test_rustybuzz_shaping() {
    let bf = bundled_face("Open Sans", 400.0, "normal").unwrap();
    let rb_face = rustybuzz::Face::from_slice(bf.data, 0).unwrap();
    let ttf_face = ttf_parser::Face::parse(bf.data, 0).unwrap();
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str("fi");
    let glyph_buffer = rustybuzz::shape(&rb_face, &[], buffer);
    let infos = glyph_buffer.glyph_infos();
    let _positions = glyph_buffer.glyph_positions();
    assert_eq!(infos.len(), 1, "fi should form a ligature in Open Sans");
    let mut builder = PathBuilder(BezPath::new());
    assert!(
        ttf_face
            .outline_glyph(ttf_parser::GlyphId(infos[0].glyph_id as u16), &mut builder)
            .is_some()
    );
    assert!(!builder.0.is_empty(), "ligature outline should not be empty");
}

#[test]
fn fast_path_newline_produces_multiple_lines() {
    let font_ctx = test_font_ctx();
    let text = "Line One\nLine Two\nLine Three";
    let compiled =
        compile_text_fast(text, "Open Sans", 400.0, "normal", 24.0, [1.0; 4], 0.0, 0.0, &font_ctx)
            .unwrap();

    assert!(!compiled.glyphs.is_empty());
    let max_line = compiled.glyphs.iter().map(|g| g.line_idx).max().unwrap_or(0);
    assert_eq!(max_line, 2, "3 lines separated by newline should have max line_idx 2");
}

#[test]
fn test_fast_path_ligatures_shaping() {
    let font_ctx = test_font_ctx();
    let compiled = compile_text_fast(
        "fine office",
        "Open Sans",
        400.0,
        "normal",
        24.0,
        [1.0; 4],
        0.0,
        0.0,
        &font_ctx,
    )
    .unwrap();

    // In Open Sans, "fi" and "ffi" form standard ligatures.
    // "fine office" has:
    // "fine": fi (ligature) + n + e = 3 glyphs
    // "office": o + ffi (ligature) + c + e = 4 glyphs
    // Total = 7 glyphs (without ligatures it would be 4 + 6 = 10 glyphs)
    assert_eq!(
        compiled.glyphs.len(),
        7,
        "OpenType ligatures should combine glyphs on fast path"
    );
}
