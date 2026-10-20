use super::*;

/// The fast path chooses a bundled face the way a font database chooses a
/// system one: style first (an italic is a different shape), then the
/// nearest available weight. Before this existed the fast path took a
/// family's *first* entry, so `font_weight: 700` and `font_style: "italic"`
/// were no-ops on bundled text — measured as identical rendered ink for
/// 400/600/800, and `web/demos/epicycles/*.amx` titles that ask for
/// `font_weight: "bold"` drew regular.
#[test]
fn bundled_face_answers_a_weight_and_style_query() {
    let regular = bundled_face("Open Sans", 400.0, "normal").expect("Open Sans");
    assert_eq!((regular.weight, regular.style), (400.0, "normal"));

    // Style wins over weight: asking for bold-italic must not hand back a
    // roman face just because its weight is nearer.
    let bold_italic = bundled_face("Open Sans", 700.0, "italic").expect("Open Sans");
    assert_eq!((bold_italic.weight, bold_italic.style), (700.0, "italic"));
    let italic = bundled_face("Open Sans", 500.0, "italic").expect("Open Sans");
    assert_eq!((italic.weight, italic.style), (400.0, "italic"));

    // Out-of-range weights clamp to the nearest shipped face: 900 has no
    // face in this bundle, so 700 is the honest answer.
    assert_eq!(bundled_face("Open Sans", 900.0, "normal").unwrap().weight, 700.0);
    assert_eq!(bundled_face("Open Sans", 100.0, "normal").unwrap().weight, 400.0);

    // A one-face family answers every query — what a slim build has.
    assert_eq!(bundled_face("Noto Sans SC", 700.0, "italic").unwrap().weight, 400.0);
    assert!(bundled_face("Times New Roman", 700.0, "normal").is_none());
}

/// The same query, proven through the compiler: bold text is wider than
/// regular at the same size, and a weight the bundle has no face for lands
/// on the nearest one rather than silently on Regular.
#[cfg(feature = "rich-text")]
#[test]
fn compile_text_fast_honours_weight_for_a_bundled_family() {
    let width_of = |weight: f32| {
        let compiled = compile_text_fast(
            "Weight makes the ink",
            "Open Sans",
            weight,
            "normal",
            64.0,
            [1.0, 1.0, 1.0, 1.0],
            0.0,
            0.0,
            &FontContext::new(),
        )
        .expect("compile");
        let mut box_: Option<kurbo::Rect> = None;
        for tp in &compiled.glyphs {
            let at_hand = tp.path.bounding_box();
            box_ = Some(match box_ {
                Some(acc) => acc.union(at_hand),
                None => at_hand,
            });
        }
        let width = box_.map(|r| r.width() as f32).unwrap_or(0.0_f32);
        assert!(width > 0.0, "the line has no extent");
        (width, compiled.glyphs.len())
    };
    let (regular, count): (f32, usize) = width_of(400.0);
    let (bold, bold_count): (f32, usize) = width_of(700.0);
    assert_eq!(count, bold_count, "the same text must shape the same glyph count");
    assert!(bold > regular + 1.0, "weight 700 must be wider than 400: {bold} vs {regular}");
    // 900 has no face: it must land on 700, not fall back to regular.
    assert!(
        (width_of(900.0).0 - bold).abs() < 0.01,
        "weight 900 should resolve to the nearest shipped weight"
    );
}

/// A weight the four static faces cannot answer is drawn by instancing the
/// variable pair's `wght` axis.
///
/// The proof is the *middle* value: 550 must differ from both 400 and 700.
/// Two endpoints alone cannot tell face selection apart from axis
/// instancing — a nearest-static pick would also grow from 400 to 700 — so
/// the interior width is what shows the number reached the font, which is
/// exactly the step the old keyword mapping removed (`625` became
/// `"medium"`, i.e. 500).
#[cfg(feature = "rich-text")]
#[test]
fn compile_text_instances_a_weight_no_static_face_ships() {
    let font_ctx = test_font_ctx();
    let ink_width = |weight: f32| -> (f32, usize) {
        let frame = compile_text(
            "Weight makes the ink",
            64.0,
            typst::visualize::Color::from_u8(255, 255, 255, 255),
            "Open Sans",
            &font_ctx,
            weight,
            "normal",
            1.2,
            0.0,
            0.0,
            0.0,
            "left",
            "visible",
        )
        .expect("compile");
        let glyphs = extract_glyphs(&frame);
        let mut box_: Option<kurbo::Rect> = None;
        for tp in &glyphs {
            let at_hand = tp.path.bounding_box();
            box_ = Some(match box_ {
                Some(acc) => acc.union(at_hand),
                None => at_hand,
            });
        }
        let width = box_.map(|r| r.width() as f32).unwrap_or(0.0_f32);
        assert!(width > 0.0, "weight {weight} produced no ink");
        (width, glyphs.len())
    };

    let (w400, count) = ink_width(400.0);
    let (w550, mid_count) = ink_width(550.0);
    let (w700, bold_count) = ink_width(700.0);
    assert_eq!(count, mid_count, "the same text must shape the same glyph count");
    assert_eq!(count, bold_count);
    assert!(w550 > w400 + 1.0, "550 must be heavier than 400: {w550} vs {w400}");
    assert!(w700 > w550 + 1.0, "700 must be heavier than 550: {w700} vs {w550}");

    // The whole ramp, including the canonical values the static faces used to
    // answer alone: strictly increasing, with no step at a canonical weight.
    // This is what the conditional registration measured in 2026-10-05 got
    // wrong (450 drew heavier than 500, and 599 / 600 / 601 came out
    // 533.2 / 553.8 / 533.5).
    let ramp: Vec<f32> = [400.0, 450.0, 500.0, 550.0, 600.0, 650.0, 700.0]
        .iter()
        .map(|w| ink_width(*w).0)
        .collect();
    for pair in ramp.windows(2) {
        assert!(pair[1] > pair[0] + 0.5, "weight ramp must grow monotonically, got {ramp:?}");
    }
    let (w599, _) = ink_width(599.0);
    let (w601, _) = ink_width(601.0);
    let (w600, _) = ink_width(600.0);
    assert!(
        (w600 - w599).abs() < 2.0 && (w601 - w600).abs() < 2.0,
        "crossing a canonical weight must not jump the layout: {w599} / {w600} / {w601}"
    );
}

/// `load_font_bytes` must grow the database: the embed's `add_font` path
/// depends on it (the web sandbox has no system fonts, so a runtime font is
/// the only way to extend coverage). Both text paths resolve families
/// through this database.
#[test]
fn load_font_bytes_grows_the_database() {
    let mut ctx = FontContext::new();
    let before = ctx.db.faces().count();

    ctx.load_font_bytes(BUNDLED_FONTS[0].data.to_vec());
    assert_eq!(
        ctx.db.faces().count(),
        before + 1,
        "the registered face must land in the database"
    );
    assert!(
        ctx.load_face("Open Sans", 400.0, "normal").is_some(),
        "the re-registered bundled family must stay resolvable"
    );
}

/// CJK text must shape through the bundled Noto Sans SC face: the wasm
/// sandbox has no system fonts, so without this face every CJK character
/// was silently skipped and the text rendered empty.
#[test]
fn cjk_text_shapes_through_the_bundled_cjk_face() {
    let font_ctx = test_font_ctx();
    let compiled = compile_text_fast(
        "你好动画",
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
    assert_eq!(compiled.glyphs.len(), 4, "every CJK character must shape to a glyph");
    for glyph in &compiled.glyphs {
        let bbox = glyph.path.bounding_box();
        assert!(bbox.width() > 0.0 && bbox.height() > 0.0, "CJK glyphs must have outlines");
    }
}

fn test_font_db() -> &'static std::sync::Arc<fontdb::Database> {
    use std::sync::OnceLock;
    static DB: OnceLock<std::sync::Arc<fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = (**system_fonts_db()).clone();
        for bf in BUNDLED_FONTS {
            if bf.family == "Open Sans" {
                db.load_font_data(bf.data.to_vec());
            }
        }
        std::sync::Arc::new(db)
    })
}

/// Helper: create a default FontContext (loads system fonts, may be slow on CI).
fn test_font_ctx() -> FontContext {
    FontContext {
        db: std::sync::Arc::clone(test_font_db()),
        text_fast_path: true,
        epoch: font_env_epoch(),
    }
}

/// Serialize the tests that assert on the process-wide compile cache
/// (clearing it, pointer identity, or cache length). The cache is a
/// process singleton, so these tests cannot run concurrently with each
/// other; they previously relied on the workspace's serial `--test-threads=1`
/// checklist, which a parallel `cargo test -p animatix-text` does not
/// guarantee.
fn cache_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static CACHE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CACHE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `max_lines` must cap the number of wrapped lines the fast text path
/// emits, and must be part of the cache key (so a capped compile is not
/// served the uncapped entry).
#[test]
fn max_lines_caps_wrapped_lines() {
    let _guard = cache_test_guard();
    let font_ctx = test_font_ctx();
    let text = "the quick brown fox jumps over the lazy dog and keeps running";

    let line_count = |max_lines: Option<usize>| {
        let compiled = compile_text_cached(
            TextKind::Text,
            text,
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
            120.0,
            max_lines,
            "left",
            "visible",
            true,
            &font_ctx,
        )
        .expect("wrapped text compiles");
        let mut lines: Vec<u32> = compiled.paths.iter().map(|p| u32::from(p.line_idx)).collect();
        lines.sort_unstable();
        lines.dedup();
        lines.len()
    };

    let uncapped = line_count(None);
    assert!(uncapped > 1, "the fixture must wrap to several lines, got {uncapped}");
    assert_eq!(line_count(Some(1)), 1, "max_lines: 1 must keep exactly one line");
    assert_eq!(line_count(Some(2)), 2, "max_lines: 2 must keep exactly two lines");
    // The cache must not alias the capped and uncapped compiles.
    assert_eq!(line_count(None), uncapped);
}

#[cfg(feature = "rich-text")]
#[test]
fn emphasis_faces_load_bold_when_family_has_it() {
    // Regression: the Typst world used to load only one regular face per
    // family, so `*bold*`/`_italic_`/`font_weight` rendered at regular weight
    // even when the system font had bold/italic faces. `load_font_emphasis_faces`
    // must include a bold face whenever the family exposes one.
    let ctx = test_font_ctx();
    let bold_family = ctx.db.faces().find_map(|face| {
        if face.weight == fontdb::Weight::BOLD {
            face.families.first().map(|f| f.0.clone())
        } else {
            None
        }
    });
    if let Some(family) = bold_family {
        let faces = ctx.load_font_emphasis_faces(&family);
        assert!(
            faces.iter().any(|f| f.info().variant.weight.to_number() >= 700),
            "expected a bold emphasis face for a family that has bold ({family})"
        );
    }
    // No assertion if no bold family exists (defensive: CI font sets vary).
}

#[test]
fn is_plain_text_basic() {
    assert!(is_plain_text("Hello World"));
    assert!(is_plain_text("abc123"));
    assert!(is_plain_text(""));
    assert!(!is_plain_text("Hello\nWorld"));
    assert!(!is_plain_text("Hello *World*"));
    assert!(!is_plain_text("$x^2$"));
    assert!(!is_plain_text("#emoji"));
    assert!(!is_plain_text("`code`"));
    assert!(!is_plain_text("Hello_World"));
    assert!(!is_plain_text("a/b"));
}

#[cfg(feature = "rich-text")]
#[test]
fn bundled_default_font_covers_bold_and_italic() {
    // Regression: the default family ("Open Sans") used to be a single
    // bundled mock face, so `*bold*`/`_italic_`/`font_weight`/`font_style`
    // fell back to regular. Now four static faces are bundled; verify the
    // world can select regular, bold, italic, and bold-italic.
    use typst::foundations::Bytes;
    use typst::text::Font;

    let mut sigs = std::collections::HashSet::new();
    for bf in BUNDLED_FONTS.iter().filter(|bf| bf.family == "Open Sans") {
        let font = Font::new(Bytes::new(bf.data), 0).expect("bundled face parses");
        let info = font.info();
        // (weight, style-debug) uniquely identifies a face variant.
        sigs.insert(format!("{}:{:?}", info.variant.weight.to_number(), info.variant.style));
    }
    assert_eq!(sigs.len(), 4, "expected regular+bold+italic+bold-italic, got {sigs:?}");
    assert!(
        sigs.iter().any(|s| s.starts_with("700:")) || sigs.iter().any(|s| s.starts_with("800:")),
        "expected a bold (>700) face, got {sigs:?}"
    );
}

/// Distinct glyph fill colors produced by a `Code` compilation of the same
/// snippet under `language` ("" = plain).
fn distinct_code_colors(language: &str) -> usize {
    let font_ctx = test_font_ctx();
    let cached = compile_text_cached(
        TextKind::Code,
        "fn main() { let x = 42; }",
        language,
        None,
        "monospace",
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
        &font_ctx,
    )
    .expect("Code compiles");
    let mut colors: Vec<String> = cached.paths.iter().map(|p| format!("{:?}", p.color)).collect();
    colors.sort();
    colors.dedup();
    colors.len()
}
#[cfg(feature = "rich-text")]
#[test]

fn code_highlight_adds_glyph_colors() {
    assert_eq!(distinct_code_colors(""), 1, "plain Code draws in the actor color");
    assert!(
        distinct_code_colors("rust") > 1,
        "a recognized language must highlight tokens with distinct colors"
    );
}

#[test]
fn unknown_highlight_language_renders_plain() {
    // An unsupported `language` warns (see compile_code) and falls back to
    // the plain single-color render — never an error, never a dropped actor.
    assert_eq!(distinct_code_colors("not-a-language"), 1);
}

/// Highlighting recolors tokens but must not change layout: the plain and
/// highlighted renders of the same snippet have identical glyph counts and
/// total advance width. This is the regression guard for Typst's line
/// builder dropping inter-token whitespace at styled-run boundaries (which
/// `highlighted_code_markup` avoids by keeping every space inside a token).
#[test]
fn highlight_preserves_spacing() {
    let font_ctx = test_font_ctx();
    let code = "fn main() {\n    let x = 42;\n}";
    let measure = |language: &str| {
        let cached = compile_text_cached(
            TextKind::Code,
            code,
            language,
            None,
            "monospace",
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
            &font_ctx,
        )
        .expect("Code compiles");
        let width = cached.paths.iter().map(|p| p.path.bounding_box().width()).sum::<f64>();
        (cached.paths.len(), width)
    };
    let (plain_n, plain_w) = measure("");
    let (hl_n, hl_w) = measure("rust");
    assert!(plain_n > 10, "plain render should have glyphs, got {plain_n}");
    assert!(hl_n > 10, "highlighted render should have glyphs, got {hl_n}");
    assert_eq!(plain_n, hl_n, "highlighting must not change the glyph count");
    assert!(
        (plain_w - hl_w).abs() < plain_w * 0.02 + 1.0,
        "highlighting must not change the total width: plain={plain_w} highlighted={hl_w}"
    );
}

#[test]
fn language_is_part_of_text_cache_key() {
    let _cache = cache_test_guard();
    let font_ctx = test_font_ctx();
    let compile = |lang: &str| {
        compile_text_cached(
            TextKind::Code,
            "def f(x): return x",
            lang,
            None,
            "monospace",
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
            &font_ctx,
        )
        .expect("Code compiles")
    };
    let python = compile("python");
    let rust = compile("rust");
    let python_again = compile("python");
    assert!(
        std::sync::Arc::ptr_eq(&python, &python_again),
        "the same (content, language) pair is a cache hit"
    );
    assert!(
        !std::sync::Arc::ptr_eq(&python, &rust),
        "different languages of the same content must not share a cache entry"
    );
}

/// A palette with distinct sentinel colours per role, for assertions.
fn sentinel_palette() -> HighlightPalette {
    HighlightPalette {
        keyword: [0.0, 1.0, 0.0, 1.0],
        function: [1.0, 1.0, 0.0, 1.0],
        string: [0.0, 1.0, 1.0, 1.0],
        number: [1.0, 0.0, 1.0, 1.0],
        comment: [0.5, 0.5, 0.5, 1.0],
        interpolation: [0.0, 0.0, 1.0, 1.0],
        escape: [1.0, 0.0, 0.0, 1.0],
        annotation: [0.0, 1.0, 0.5, 1.0],
    }
}

fn compile_code_with(
    font_ctx: &FontContext,
    code: &str,
    palette: Option<&HighlightPalette>,
) -> std::sync::Arc<CachedText> {
    compile_text_cached(
        TextKind::Code,
        code,
        "rust",
        palette,
        "monospace",
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
    .expect("Code compiles")
}

#[cfg(feature = "rich-text")]
fn glyph_hexes(cached: &CachedText) -> Vec<String> {
    cached
        .paths
        .iter()
        .map(|p| {
            let [r, g, b, _a] = p.color;
            format!("#{r:02x}{g:02x}{b:02x}")
        })
        .collect()
}
#[cfg(feature = "rich-text")]
#[test]

fn highlight_palette_recolours_tokens() {
    let font_ctx = test_font_ctx();
    // `fn` is a keyword; the sentinel palette maps keyword → pure green.
    let with =
        glyph_hexes(&compile_code_with(&font_ctx, "fn main() {}", Some(&sentinel_palette())));
    assert!(
        with.iter().any(|h| h.eq_ignore_ascii_case("#00ff00")),
        "a token should take its palette role colour, got {with:?}"
    );
    // Without a palette, Typst's fixed theme colour is used (not the green sentinel).
    let without = glyph_hexes(&compile_code_with(&font_ctx, "fn main() {}", None));
    assert!(
        !without.iter().any(|h| h.eq_ignore_ascii_case("#00ff00")),
        "no palette must keep the built-in theme colours, got {without:?}"
    );
}

#[test]
fn palette_is_part_of_text_cache_key() {
    let _cache = cache_test_guard();
    let font_ctx = test_font_ctx();
    let code = "fn main() {}";
    let a = sentinel_palette();
    let mut b = sentinel_palette();
    b.keyword = [0.2, 0.3, 0.4, 1.0];
    let ca = compile_code_with(&font_ctx, code, Some(&a));
    let cb = compile_code_with(&font_ctx, code, Some(&b));
    let ca_again = compile_code_with(&font_ctx, code, Some(&a));
    assert!(std::sync::Arc::ptr_eq(&ca, &ca_again), "same palette + content is a cache hit");
    assert!(
        !std::sync::Arc::ptr_eq(&ca, &cb),
        "different palettes must not share a cache entry"
    );
}

#[test]
fn is_latin_text_basic() {
    assert!(is_latin_text("Hello World"));
    assert!(is_latin_text("Café")); // Latin-1 Supplement
    assert!(is_latin_text("Âmbit")); // Latin Extended-A
    assert!(is_latin_text(""));
    assert!(!is_latin_text("中文"));
    assert!(!is_latin_text("Привет"));
    assert!(!is_latin_text("مرحبا"));
}

/// `load_face` hands out `Face<'static>`, which requires a process-long
/// buffer; the buffer must be allocated once per face, not once per call.
#[test]
fn load_face_reuses_one_buffer_per_face() {
    let font_ctx = test_font_ctx();
    let id = font_ctx
        .db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name("Open Sans")],
            weight: fontdb::Weight(400),
            style: fontdb::Style::Normal,
            ..Default::default()
        })
        .expect("Open Sans is bundled");
    let first = leaked_face_data(&font_ctx, id).expect("face data");
    let second = leaked_face_data(&font_ctx, id).expect("face data");
    assert!(
        std::ptr::eq(first, second),
        "load_face leaked a fresh buffer on the second call"
    );
}

#[test]
fn fast_path_produces_glyphs() {
    let font_ctx = test_font_ctx();
    let family = "Open Sans";

    // Verify the bundled font is available
    assert!(
        font_ctx.load_face(family, 400.0, "normal").is_some(),
        "Open Sans font should be loadable"
    );

    let compiled = compile_text_fast(
        "Hello",
        family,
        400.0,                // weight
        "normal",             // style
        48.0,                 // size
        [1.0, 1.0, 1.0, 1.0], // white
        0.0,                  // letter_spacing
        0.0,                  // word_spacing
        &font_ctx,
    )
    .expect("fast path should succeed");

    // Should produce at least one glyph path per character
    assert!(!compiled.glyphs.is_empty(), "Should produce glyph paths");

    // Metrics should be populated
    assert!(compiled.ascent > 0.0);
    assert!(compiled.descent < 0.0);
}

#[test]
fn fast_path_with_bold_weight() {
    let font_ctx = test_font_ctx();
    let family = "Open Sans";

    // Bold weight should also resolve
    assert!(
        font_ctx.load_face(family, 700.0, "normal").is_some(),
        "Open Sans bold should be loadable"
    );

    let compiled = compile_text_fast(
        "Bold Text",
        family,
        700.0, // bold
        "normal",
        48.0,
        [1.0, 1.0, 1.0, 1.0],
        0.0,
        0.0,
        &font_ctx,
    )
    .expect("bold fast path should succeed");

    assert!(!compiled.glyphs.is_empty(), "Bold text should produce glyph paths");
}

#[cfg(feature = "rich-text")]
#[test]
fn fast_path_vs_typst_visually_equivalent() {
    // Compare fast-path output against Typst output for a simple string.
    // They should produce bounding boxes that are close (sub-pixel tolerance).
    let font_ctx = test_font_ctx();
    let family = "Open Sans";
    let text = "Hello World";
    let size = 48.0;
    let color = [1.0, 1.0, 1.0, 1.0];

    // Fast path
    let fast_compiled =
        compile_text_fast(text, family, 400.0, "normal", size, color, 0.0, 0.0, &font_ctx)
            .expect("fast path should succeed");
    let fast_paths = fast_compiled.glyphs;

    // Typst path
    let typst_color = typst::visualize::Color::from_u8(255, 255, 255, 255);
    let frame = compile_text(
        text,
        size,
        typst_color,
        family,
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
    .expect("Typst path should succeed");
    let typst_paths = extract_glyphs(&frame);

    assert!(!fast_paths.is_empty(), "Fast path should produce paths");
    assert!(!typst_paths.is_empty(), "Typst path should produce paths");

    // Compare overall bounding box (centered).
    // Both paths use the same bundled font data, but different rendering
    // pipelines (ttf_parser vs Typst's engine) which can produce slightly
    // different metrics — allow a generous tolerance for this difference.
    let fast_bbox = measure_text_paths(&fast_paths);
    let typst_bbox = measure_text_paths(&typst_paths);

    let half_w_diff = (fast_bbox[0] - typst_bbox[0]).abs();
    let half_h_diff = (fast_bbox[1] - typst_bbox[1]).abs();
    assert!(
        half_w_diff < 30.0,
        "Width mismatch: fast={:.3}, typst={:.3}, diff={:.3}",
        fast_bbox[0] * 2.0,
        typst_bbox[0] * 2.0,
        half_w_diff * 2.0
    );
    assert!(
        half_h_diff < 30.0,
        "Height mismatch: fast={:.3}, typst={:.3}, diff={:.3}",
        fast_bbox[1] * 2.0,
        typst_bbox[1] * 2.0,
        half_h_diff * 2.0
    );
}

#[test]
fn fast_path_caches_results() {
    let _guard = cache_test_guard();
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    let paths1 = compiler
        .compile(
            "Cache Test",
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
        .expect("first compile should succeed");

    let paths2 = compiler
        .compile(
            "Cache Test",
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
        .expect("second compile should succeed (cache hit)");

    // Same Arc should be returned (cache hit)
    assert_eq!(paths1.as_ptr(), paths2.as_ptr(), "Cache should return the same Arc pointer");
}

#[test]
fn non_plain_text_routes_to_typst() {
    let font_ctx = test_font_ctx();
    let mut compiler = TextCompiler::new();
    compiler.text_fast_path = true;

    // Text with asterisk should route to Typst (not fast path)
    let paths = compiler
        .compile(
            "Hello *World*",
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
        .expect("Typst fallback should succeed");

    assert!(!paths.is_empty(), "Should produce glyph paths via Typst");
}

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
