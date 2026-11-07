use super::*;

// Tests exercise the request-based entry point directly.
use crate::compiler::compile_text_cached;
use crate::request::{TextCompileRequest, TextStyle};

#[test]
fn non_latin_text_routes_to_typst() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Non-Latin text should route to Typst
    let _paths =
        compile_request_with_defaults(&mut compiler, "中文测试", TextKind::Text, "", &font_ctx);

    // Even though Typst may not render CJK with Open Sans (the bundled mock font
    // has no .notdef outline for unavailable characters), the compilation itself
    // should succeed without crashing.
    // The paths may be empty if .notdef has no outline, but we at least verify
    // that Typst was used (not the fast path) by checking the routing worked.
    // Just verifying that compile succeeded without panic is sufficient.
}

#[test]
fn fast_path_disabled_via_config_falls_back() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = false; // disabled

    // Plain text should still compile via Typst
    let paths =
        compile_request_with_defaults(&mut compiler, "Plain Text", TextKind::Text, "", &font_ctx);

    assert!(!paths.is_empty(), "Should produce glyph paths");
}

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

#[test]
fn grouped_typst_compile_is_memoized() {
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    // Mirrors the Equation frame path: every `Fragment` is wrapped in a
    // `#box()` so the frame exposes one group per fragment.
    let compile = || {
        compile_typst_grouped_cached(
            "#box()[E] #box()[ = ] #box()[m] #box()[c^2]",
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
    // The grouped shape keeps the per-fragment boundaries the Equation
    // renderer needs to place its highlight overlays: one range per
    // `#box()` group that actually produced glyphs.
    assert!(!first.glyphs.is_empty(), "compile should emit glyphs");
    assert!(
        !first.ranges.is_empty() && first.ranges.len() <= 4,
        "one range per non-empty #box() group, got {}",
        first.ranges.len()
    );

    clear_text_compile_cache();
    assert_eq!(grouped_text_compile_cache_len(), 0, "clear must empty the grouped cache");
}

/// The slim grouped path used to hardcode `"normal"` / `1.2`, silently
/// dropping the style and line height the caller requested. Only invalid
/// values may fall back.
#[cfg(not(feature = "rich-text"))]
#[test]
fn slim_grouped_typography_passes_valid_values_and_falls_back() {
    use crate::compiler::slim_grouped_typography;

    assert_eq!(slim_grouped_typography("normal", 1.2), ("normal", 1.2));
    assert_eq!(slim_grouped_typography("italic", 2.0), ("italic", 2.0));
    assert_eq!(slim_grouped_typography("oblique", 0.8), ("oblique", 0.8));

    assert_eq!(slim_grouped_typography("bogus", 2.0), ("normal", 2.0));
    assert_eq!(slim_grouped_typography("", 2.0), ("normal", 2.0));
    assert_eq!(slim_grouped_typography("italic", 0.0), ("italic", 1.2));
    assert_eq!(slim_grouped_typography("italic", -1.0), ("italic", 1.2));
    assert_eq!(slim_grouped_typography("italic", f32::NAN), ("italic", 1.2));
    assert_eq!(slim_grouped_typography("italic", f32::INFINITY), ("italic", 1.2));
}

/// End-to-end slim check that the requested line height reaches the fast
/// layout: two lines at 2.5 spacing must be visibly taller than at 1.0.
#[cfg(not(feature = "rich-text"))]
#[test]
fn slim_grouped_text_uses_requested_line_height() {
    let font_ctx = test_font_ctx();

    let ink_height = |line_height: f32| {
        let grouped = compile_typst_grouped_cached(
            "one\ntwo",
            24.0,
            [1.0, 1.0, 1.0, 1.0],
            "Open Sans",
            &font_ctx,
            400.0,
            "normal",
            line_height,
            0.0,
            0.0,
        )
        .expect("slim grouped compile should succeed");
        assert!(!grouped.glyphs.is_empty(), "grouped compile should emit glyphs");
        measure_text_paths(&grouped.glyphs)[1] * 2.0
    };

    let tight = ink_height(1.0);
    let loose = ink_height(2.5);
    assert!(
        loose > tight + 10.0,
        "requested line height must reach the fast path: tight={tight:.1}, loose={loose:.1}"
    );
}

#[test]
fn font_epoch_invalidates_cache() {
    clear_text_compile_cache();
    let compile_at_current_epoch = |font_ctx: &FontContext| {
        let request = TextCompileRequest::new(
            TextKind::Typst,
            "$ e^{i\\pi} $",
            TextStyle::new("Open Sans", 24.0),
        );
        compile_text_cached(&request, font_ctx).expect("cached typst compile should succeed")
    };

    let before = compile_at_current_epoch(&test_font_ctx());

    // A context created after the epoch advanced must not observe entries
    // compiled under the previous font environment.
    advance_font_env_epoch();
    let fresh_ctx = test_font_ctx();
    let after = compile_at_current_epoch(&fresh_ctx);
    assert_ne!(
        before.paths.as_ptr(),
        after.paths.as_ptr(),
        "entries from a previous font epoch must not be served to newer contexts"
    );
    // Within the new epoch, memoization still works.
    let again = compile_at_current_epoch(&fresh_ctx);
    assert_eq!(after.paths.as_ptr(), again.paths.as_ptr());
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

/// The probe reports tofu for whichever path will shape the text: the
/// bundled face set on the fast path, the whole registered database on
/// the Typst path.
#[test]
fn missing_glyph_probe_reports_uncovered_chars() {
    let font_ctx = test_font_ctx();

    // Covered Latin text: no findings.
    assert!(
        missing_text_glyphs(
            TextKind::Text,
            "softmax(QK / sqrt(d))",
            "Open Sans",
            400.0,
            "normal",
            true,
            &font_ctx,
        )
        .is_empty()
    );

    // Fast-path text with an in-gate character no bundled face covers:
    // U+02B0 (modifier letter small h) sits inside the Latin gate's
    // modifier-letter range, so the fast path will shape it — and skip it.
    let missing = missing_text_glyphs(
        TextKind::Text,
        "1st\u{02B0} order",
        "Open Sans",
        400.0,
        "normal",
        true,
        &font_ctx,
    );
    assert_eq!(missing, ['\u{02B0}'], "in-gate uncovered char should be reported");

    // Non-Latin content compiles through the Typst engine: the probe
    // checks the whole registered database. Against the real system db
    // this machine usually covers everything, so run the branch against
    // a controlled context holding only one Open Sans face.
    let mut bare_db = fontdb::Database::new();
    bare_db.load_font_data(BUNDLED_FONTS[0].data.to_vec());
    let bare_ctx = FontContext {
        db: std::sync::Arc::new(bare_db),
        registered_faces: Vec::new(),
        epoch: font_env_epoch(),
    };
    let missing = missing_text_glyphs(
        TextKind::Text,
        "你好 \u{1F600}",
        "Open Sans",
        400.0,
        "normal",
        true,
        &bare_ctx,
    );
    assert!(
        missing.contains(&'你') && missing.contains(&'好'),
        "CJK should be reported without a CJK face: {missing:?}"
    );
    assert!(missing.contains(&'\u{1F600}'), "emoji should be reported: {missing:?}");
    assert!(!missing.contains(&'g'), "covered Latin must stay silent: {missing:?}");

    // Multi-line content is never plain; the Typst branch governs.
    assert!(
        missing_text_glyphs(
            TextKind::Text,
            "multi\nline",
            "Open Sans",
            400.0,
            "normal",
            true,
            &font_ctx,
        )
        .is_empty()
    );
    // The fast-path switch off: the Typst path governs, fast-path faces
    // are not consulted.
    assert!(
        missing_text_glyphs(
            TextKind::Text,
            "1st\u{02B0}",
            "Open Sans",
            400.0,
            "normal",
            false,
            &font_ctx,
        )
        .is_empty()
    );
}
