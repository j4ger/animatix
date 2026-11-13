use super::*;

// Tests exercise the request-based entry point directly.
use crate::compiler::compile_text_cached;
use crate::request::{TextCompileRequest, TextStyle};

#[test]
fn non_latin_text_uses_literal_fast_path() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Plain Text is literal Unicode and stays on the lightweight shaper.
    let paths =
        compile_request_with_defaults(&mut compiler, "中文测试", TextKind::Text, "", &font_ctx);
    assert_eq!(paths.len(), 4, "bundled Noto Sans SC should shape every Han glyph");
}

#[test]
fn text_fast_path_setting_does_not_change_literal_text_semantics() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = false; // disabled

    // The legacy switch cannot route plain Text through Typst anymore.
    let paths =
        compile_request_with_defaults(&mut compiler, "Plain Text", TextKind::Text, "", &font_ctx);

    assert!(!paths.is_empty(), "literal text should produce glyph paths");
}

#[cfg(feature = "rich-text")]
#[test]
fn code_kind_uses_typst_not_fast_path() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Even plain text should use Typst for Code kind
    let paths =
        compile_request_with_defaults(&mut compiler, "fn main()", TextKind::Code, "", &font_ctx);

    assert!(!paths.is_empty(), "Should produce glyph paths");
}

#[cfg(feature = "rich-text")]
#[test]
fn compile_cache_keys_on_wrapping_params() {
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let common = |max_width: f32| {
        let request = TextCompileRequest {
            max_width,
            ..TextCompileRequest::new(TextKind::Typst, "$ x^2 $", TextStyle::new("Open Sans", 24.0))
        };
        compile_text_cached(&request, &font_ctx).expect("cached typst compile should succeed")
    };

    let unwrapped = common(0.0);
    let wrapped = common(200.0);

    // Distinct max_width must not collide: the old per-instance key omitted
    // wrapping parameters, which could serve the wrong layout.
    assert_ne!(
        unwrapped.paths.as_ptr(),
        wrapped.paths.as_ptr(),
        "different max_width must produce distinct cache entries"
    );
    // Same inputs hit the same entry.
    let again = common(0.0);
    assert_eq!(
        unwrapped.paths.as_ptr(),
        again.paths.as_ptr(),
        "identical inputs must reuse the memoized entry"
    );

    clear_text_compile_cache();
    assert_eq!(text_compile_cache_len(), 0, "clear must empty the cache");
}

/// The request-based cache and [`TextCompiler::compile_request`] must agree: a
/// compiler compile is a cache hit on the same complete request a caller builds.
#[test]
fn request_and_compiler_share_the_cache_entry() {
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let request = TextCompileRequest {
        allow_fast_path: true,
        ..TextCompileRequest::new(
            TextKind::Text,
            "Shared Cache Entry",
            TextStyle::new("Open Sans", 24.0),
        )
    };
    let via_request = compile_text_cached(&request, &font_ctx).expect("request compile");

    let mut compiler = TextCompiler::new();
    let via_compiler = compile_request_with_defaults(
        &mut compiler,
        "Shared Cache Entry",
        TextKind::Text,
        "",
        &font_ctx,
    );
    assert!(
        std::sync::Arc::ptr_eq(&via_request.paths, &via_compiler),
        "the TextCompiler adapter must hit the request-built cache entry"
    );

    clear_text_compile_cache();
}

/// Regression for the removed flat 15-argument adapter: requests with
/// non-default `font_size`, `letter_spacing`, and `max_width` used to be passed
/// positionally, where swapping adjacent values still compiled but changed the
/// layout. `compile_request` must hand every request field to the cache
/// unchanged; a pointer match against the directly compiled request proves the
/// effective cache keys (and therefore all layout inputs) are identical.
#[test]
fn compile_request_preserves_adjacent_typography_and_wrapping_fields() {
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let mut style = TextStyle::new("Open Sans", 30.0);
    style.letter_spacing = 3.0;
    let request = TextCompileRequest {
        allow_fast_path: true,
        max_width: 120.0,
        ..TextCompileRequest::new(TextKind::Text, "adjacent fields regression", style)
    };

    let direct = compile_text_cached(&request, &font_ctx).expect("direct request compile");
    let compiler = TextCompiler::new();
    let via_compiler =
        compiler.compile_request(&request, &font_ctx).expect("compiler request compile");

    assert!(
        std::sync::Arc::ptr_eq(&direct.paths, &via_compiler),
        "compile_request must preserve font_size=30, letter_spacing=3, max_width=120"
    );

    // A request with the same content but transposed font_size/letter_spacing
    // must not share the entry; otherwise the test above could pass even if the
    // request fields were ignored.
    let mut transposed_style = TextStyle::new("Open Sans", 3.0);
    transposed_style.letter_spacing = 30.0;
    let transposed = TextCompileRequest {
        allow_fast_path: true,
        max_width: 120.0,
        ..TextCompileRequest::new(TextKind::Text, "adjacent fields regression", transposed_style)
    };
    let transposed_paths = compile_text_cached(&transposed, &font_ctx).expect("transposed compile");
    assert!(
        !std::sync::Arc::ptr_eq(&direct.paths, &transposed_paths.paths),
        "transposed typography fields must not reuse the original cache entry"
    );

    clear_text_compile_cache();
}

#[cfg(feature = "rich-text")]
#[test]
fn typst_wrapping_supports_overflow_modes() {
    let font_ctx = test_font_ctx();

    // Ensure compile_text with overflow: ellipsis and overflow: clip succeed via Typst path
    let typst_color = typst::visualize::Color::from_u8(255, 255, 255, 255);
    let frame_ellipsis = compile_text(
        "Very long text truncated with ellipsis",
        18.0,
        typst_color.clone(),
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        200.0,
        "left",
        "ellipsis",
    );
    assert!(frame_ellipsis.is_ok(), "Typst compile with overflow 'ellipsis' must succeed");

    let frame_clip = compile_text(
        "Very long text clipped",
        18.0,
        typst_color,
        "Open Sans",
        &font_ctx,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        200.0,
        "left",
        "clip",
    );
    assert!(frame_clip.is_ok(), "Typst compile with overflow 'clip' must succeed");
}

#[cfg(feature = "rich-text")]
#[test]
fn grouped_typst_compile_is_memoized() {
    let mut font_ctx = FontContext::empty();
    font_ctx.load_fonts([
        include_bytes!("../../assets/fonts/OpenSans-Variable.ttf").to_vec(),
        include_bytes!("../../assets/fonts/FiraMath-Regular.otf").to_vec(),
    ]);
    clear_text_compile_cache();

    // Mirrors the Equation frame path: metadata preserves each authored
    // fragment through Typst's transparent layout wrappers.
    let markup = ["$E$", "$=$", "$m$", "$c^2$"]
        .iter()
        .enumerate()
        .map(|(index, fragment)| {
            format!(
                "#metadata(\"animatix:equation-fragment:start:{index}\") #box()[{fragment}] #metadata(\"animatix:equation-fragment:end:{index}\")"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let compile = || {
        compile_typst_grouped_cached(
            &markup,
            48.0,
            [1.0, 1.0, 1.0, 1.0],
            "",
            &font_ctx,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
        )
        .expect("grouped typst compile should succeed")
    };

    let first = compile();
    assert_eq!(grouped_text_compile_cache_len(), 1, "first compile memoizes");
    let second = compile();
    assert_eq!(
        grouped_text_compile_cache_len(),
        1,
        "identical inputs must reuse the memoized entry, not add one"
    );
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "memo hit must hand back the same entry"
    );
    // Every Equation Fragment retains its identity, including math markup.
    assert!(!first.glyphs.is_empty(), "compile should emit glyphs");
    assert_eq!(first.ranges.len(), 4, "one range per Equation Fragment");
    assert!(first.ranges.iter().all(|range| !range.is_empty()));

    clear_text_compile_cache();
    assert_eq!(grouped_text_compile_cache_len(), 0, "clear must empty the grouped cache");
}

/// Rich kinds cannot silently downgrade to literal text when unavailable.
#[cfg(not(feature = "rich-text"))]
#[test]
fn rich_kinds_fail_clearly_without_the_capability() {
    let ctx = test_font_ctx();
    for kind in [TextKind::Typst, TextKind::Math, TextKind::Code] {
        let request = TextCompileRequest::new(kind, "x", TextStyle::new("Open Sans", 24.0));
        let error = compile_text_cached(&request, &ctx)
            .err()
            .expect("rich kind must fail")
            .to_string();
        assert!(error.contains("rich") || error.contains("capability"), "{kind:?}: {error}");
    }
    assert!(
        compile_typst_grouped_cached(
            "hello",
            24.0,
            [1.0; 4],
            "Open Sans",
            &ctx,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0
        )
        .is_err()
    );
}

#[test]
fn font_set_content_identity_separates_custom_overlays() {
    clear_text_compile_cache();
    let compile = |font_ctx: &FontContext| {
        let request = TextCompileRequest::new(
            TextKind::Text,
            "font overlay isolation",
            TextStyle::new("Open Sans", 24.0),
        );
        compile_text_cached(&request, font_ctx).expect("cached text compile should succeed")
    };

    let mut first_ctx = test_font_ctx();
    let mut second_ctx = test_font_ctx();
    first_ctx.load_font_bytes(BUNDLED_FONTS[0].data.to_vec());
    second_ctx.load_font_bytes(BUNDLED_FONTS[1].data.to_vec());
    assert_ne!(first_ctx.font_set_id(), second_ctx.font_set_id());
    let before = compile(&first_ctx);
    let after = compile(&second_ctx);
    assert_ne!(
        before.paths.as_ptr(),
        after.paths.as_ptr(),
        "different font content must not share a compiled entry"
    );
    let again = compile(&first_ctx);
    assert_eq!(before.paths.as_ptr(), again.paths.as_ptr());
    clear_text_compile_cache();
}

#[test]
fn compile_cache_survives_new_compiler_instances() {
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let compile_with = || {
        let mut compiler = TextCompiler::new();
        compile_request_with_defaults(
            &mut compiler,
            "Survives Rebuild",
            TextKind::Text,
            "",
            &font_ctx,
        )
    };

    let first = compile_with();
    let second = compile_with();
    // A fresh TextCompiler (as created by every Timeline rebuild) must see
    // the entries produced by earlier instances.
    assert_eq!(
        first.as_ptr(),
        second.as_ptr(),
        "memoized results must survive across compiler instances"
    );
    clear_text_compile_cache();
}

#[test]
fn fast_path_letter_spacing() {
    // Verify that letter_spacing affects the total width.
    let font_ctx = test_font_ctx();

    let compiled_no_spacing =
        compile_text_fast("AB", "Open Sans", 400.0, "normal", 48.0, [1.0; 4], 0.0, 0.0, &font_ctx)
            .unwrap();
    let paths_no_spacing = compiled_no_spacing.glyphs;

    let compiled_with_spacing = compile_text_fast(
        "AB",
        "Open Sans",
        400.0,
        "normal",
        48.0,
        [1.0; 4],
        10.0,
        0.0,
        &font_ctx, // 10pt letter spacing
    )
    .unwrap();
    let paths_with_spacing = compiled_with_spacing.glyphs;

    let bbox_no = measure_text_paths(&paths_no_spacing);
    let bbox_with = measure_text_paths(&paths_with_spacing);

    // Width should be larger with letter spacing
    assert!(
        bbox_with[0] > bbox_no[0],
        "Letter spacing should increase width: no_spacing={:.3}, with_spacing={:.3}",
        bbox_no[0],
        bbox_with[0]
    );
}

#[test]
fn fast_path_resolves_bundled_and_system_fonts() {
    let font_ctx = test_font_ctx();

    // Open Sans is bundled, should always work
    let face = font_ctx.load_face("Open Sans", 400.0, "normal");
    assert!(face.is_some(), "Open Sans should resolve");

    // Try system font (may or may not exist, but shouldn't crash)
    let _ = font_ctx.load_face("Arial", 400.0, "normal");
    let _ = font_ctx.load_face("sans-serif", 400.0, "normal");
}

#[test]
fn compile_text_fast_empty_string() {
    let font_ctx = test_font_ctx();
    let compiled =
        compile_text_fast("", "Open Sans", 400.0, "normal", 48.0, [1.0; 4], 0.0, 0.0, &font_ctx)
            .unwrap();

    assert!(compiled.glyphs.is_empty(), "Empty string should produce no paths");
    assert!(compiled.ascent == 0.0, "Empty string should have zero ascent");
}

#[test]
fn wrapped_fast_path_produces_multiple_lines() {
    // A long string wrapped at narrow width should produce multiple lines
    let font_ctx = test_font_ctx();
    let text = "Hello world this is a long string that should wrap";
    let compiled_single =
        compile_text_fast(text, "Open Sans", 400.0, "normal", 24.0, [1.0; 4], 0.0, 0.0, &font_ctx)
            .unwrap();
    let paths_single = compiled_single.glyphs;

    // With a narrow max_width, wrapping should produce more glyphs due to vertical layout
    let compiled_wrapped = compile_text_fast_wrapped(
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
        50.0,
        None,
        "left",
        "visible",
    )
    .unwrap();
    let paths_wrapped = compiled_wrapped.glyphs;

    // Wrapped text should produce glyphs
    assert!(!paths_wrapped.is_empty(), "Wrapped text should produce glyphs");

    // The bounding box height should be taller (multiple lines) vs single line width
    let single_bbox = measure_text_paths(&paths_single);
    let wrapped_bbox = measure_text_paths(&paths_wrapped);

    // Wrapped text should have a taller bounding box (multiple lines)
    assert!(
        wrapped_bbox[1] > single_bbox[1],
        "Wrapped text should be taller (multi-line): single_h={:.3}, wrapped_h={:.3}",
        single_bbox[1],
        wrapped_bbox[1]
    );
}

/// Coverage diagnostics use exactly the same resolved faces as literal Text.
#[test]
fn missing_glyph_probe_reports_uncovered_chars() {
    let ctx = FontContext::with_database(std::sync::Arc::new(fontdb::Database::new()));
    let probe = |text: &str, fast| {
        missing_text_glyphs(TextKind::Text, text, "Open Sans", 400.0, "normal", fast, &ctx)
    };
    assert!(probe("softmax(QK / sqrt(d))", true).is_empty());
    assert!(probe("你好", true).is_empty(), "native bundle includes Chinese fallback");
    assert!(probe("multi\nline", true).is_empty());
    assert!(probe("e\u{301}\u{200d}", true).is_empty(), "shaping controls are not tofu");
    assert_eq!(probe("x\u{10ffff}", true), vec!['\u{10ffff}']);
    assert_eq!(probe("x\u{10ffff}", false), probe("x\u{10ffff}", true));
}
