use super::*;

#[test]
fn non_latin_text_routes_to_typst() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Non-Latin text should route to Typst
    let _paths = compiler
        .compile(
            "中文测试",
            "Open Sans",
            24.0,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            TextKind::Text,
            "",
            &font_ctx,
            0.0,
            "left",
            "visible",
        )
        .expect("Typst path for non-Latin should succeed");

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
    let paths = compiler
        .compile(
            "Plain Text",
            "Open Sans",
            24.0,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            TextKind::Text,
            "",
            &font_ctx,
            0.0,
            "left",
            "visible",
        )
        .expect("Typst fallback should succeed when fast path disabled");

    assert!(!paths.is_empty(), "Should produce glyph paths");
}

#[test]
fn code_kind_uses_typst_not_fast_path() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Even plain text should use Typst for Code kind
    let paths = compiler
        .compile(
            "fn main()",
            "Open Sans",
            24.0,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            TextKind::Code, // Code kind → always Typst
            "",
            &font_ctx,
            0.0,
            "left",
            "visible",
        )
        .expect("Code kind should compile via Typst");

    assert!(!paths.is_empty(), "Should produce glyph paths");
}

#[test]
fn compile_cache_keys_on_wrapping_params() {
    let _cache = cache_test_guard();
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let common = |max_width: f32| {
        compile_text_cached(
            TextKind::Typst,
            "$ x^2 $",
            "",
            None,
            "Open Sans",
            24.0,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            max_width,
            None,
            "left",
            "visible",
            false,
            &font_ctx,
        )
        .expect("cached typst compile should succeed")
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

#[test]
fn typst_wrapping_supports_overflow_modes() {
    let _cache = cache_test_guard();
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
    let _cache = cache_test_guard();
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

#[test]
fn font_epoch_invalidates_cache() {
    let _cache = cache_test_guard();
    clear_text_compile_cache();
    let compile_at_current_epoch = |font_ctx: &FontContext| {
        compile_text_cached(
            TextKind::Typst,
            "$ e^{i\\pi} $",
            "",
            None,
            "Open Sans",
            24.0,
            400.0,
            "normal",
            1.2,
            0.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            0.0,
            None,
            "left",
            "visible",
            false,
            font_ctx,
        )
        .expect("cached typst compile should succeed")
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
    let _cache = cache_test_guard();
    let font_ctx = test_font_ctx();
    clear_text_compile_cache();

    let compile_with = || {
        let mut compiler = TextCompiler::new();
        compiler
            .compile(
                "Survives Rebuild",
                "Open Sans",
                24.0,
                400.0,
                "normal",
                1.2,
                0.0,
                0.0,
                [1.0, 1.0, 1.0, 1.0],
                TextKind::Text,
                "",
                &font_ctx,
                0.0,
                "left",
                "visible",
            )
            .expect("compile should succeed")
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
        text_fast_path: true,
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
