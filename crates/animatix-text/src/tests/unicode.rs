use super::*;

#[test]
fn combining_cluster_keeps_one_logical_character_index() {
    let ctx = test_font_ctx();
    let compile = |text: &str| {
        compile_text_fast(text, "Open Sans", 537.0, "normal", 28.0, [1.0; 4], 0.0, 0.0, &ctx)
            .unwrap()
    };
    let composed = compile("éx");
    let decomposed = compile("e\u{301}x");
    assert_eq!(composed.glyphs.len(), decomposed.glyphs.len());
    assert_eq!(decomposed.glyphs.last().unwrap().char_idx, 2);
    assert_eq!(decomposed.glyphs.first().unwrap().char_idx, 0);
    let composed_width = measure_text_paths(&composed.glyphs)[0];
    let decomposed_width = measure_text_paths(&decomposed.glyphs)[0];
    assert!((composed_width - decomposed_width).abs() < 0.01);
}

#[test]
fn chinese_wraps_at_unicode_breaks_with_source_indices() {
    let ctx = test_font_ctx();
    let compiled = compile_text_fast_wrapped(
        "中文换行测试",
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &ctx,
        50.0,
        None,
        "left",
        "visible",
    )
    .unwrap();
    assert_eq!(compiled.glyphs.len(), 6);
    assert_eq!(
        compiled.glyphs.iter().map(|glyph| glyph.char_idx).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
    assert_eq!(
        compiled.glyphs.iter().map(|glyph| glyph.word_idx).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
    assert!(compiled.glyphs.iter().any(|glyph| glyph.line_idx > 0));
    assert!(
        compiled
            .hardware_layout
            .unwrap()
            .runs
            .iter()
            .all(|run| run.glyphs.iter().all(|glyph| glyph.id != 0))
    );
}

#[test]
fn font_bytes_and_face_ids_do_not_cross_overlay_contexts() {
    let empty = || FontContext::with_database(std::sync::Arc::new(fontdb::Database::new()));
    let original = BUNDLED_FONTS[0].data.to_vec();
    let mut altered = original.clone();
    // Valid font tables stay identical; trailing bytes distinguish content
    // without requiring an additional licensed font fixture.
    altered.extend_from_slice(b"overlay-content-identity");
    let mut first = empty();
    let mut same = empty();
    let mut second = empty();
    first.load_font_bytes(original.clone());
    same.load_font_bytes(original);
    second.load_font_bytes(altered);
    assert_eq!(first.font_set_id(), same.font_set_id());
    assert_ne!(first.font_set_id(), second.font_set_id());
    let a = first.load_face("Open Sans", 400.0, "normal").unwrap();
    let b = second.load_face("Open Sans", 400.0, "normal").unwrap();
    assert_eq!(a.id, b.id, "independent databases reuse face ids");
    assert_ne!(a.bytes.len(), b.bytes.len());
    assert!(!std::sync::Arc::ptr_eq(&a.bytes, &b.bytes));
    let a_again = first.load_face("Open Sans", 400.0, "normal").unwrap();
    assert!(std::sync::Arc::ptr_eq(&a.bytes, &a_again.bytes));
}

#[test]
fn empty_context_never_uses_compiled_bundle_accidentally() {
    let ctx = FontContext::empty();
    assert!(ctx.load_face_data("Open Sans", 400.0, "normal").is_none());
    assert!(!ctx.families().iter().any(|name| name == "Open Sans"));
    assert!(ctx.fallback_face_for_char('A').is_none());
    assert!(ctx.fallback_face_for_char('中').is_none());
    assert!(
        super::super::compile_text_fast(
            "A",
            "Open Sans",
            400.0,
            "normal",
            24.0,
            [1.0; 4],
            0.0,
            0.0,
            &ctx
        )
        .is_err()
    );
}

#[test]
fn registered_fallback_precedes_bundled_fallback_for_same_family() {
    let mut ctx = FontContext::empty();
    ctx.load_font_bytes(BUNDLED_FONTS[0].data.to_vec());
    ctx.load_font_bytes(BUNDLED_FONTS[1].data.to_vec());
    let faces =
        super::super::fast_layout::resolve_faces("中", "Open Sans", 400.0, "normal", &ctx, "test")
            .unwrap();
    assert!(faces[1].id.is_some(), "first matching face must be the registered overlay");
    assert_eq!(faces[1].bytes.as_ref(), BUNDLED_FONTS[1].data);
}

#[test]
fn identical_font_sets_with_different_database_ids_share_coverage_safely() {
    let make = |with_hole: bool| {
        let mut db = fontdb::Database::new();
        let removed = if with_hole {
            db.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(BUNDLED_FONTS[2].data)))
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        db.load_font_data(BUNDLED_FONTS[0].data.to_vec());
        db.load_font_data(BUNDLED_FONTS[1].data.to_vec());
        for id in removed {
            db.remove_face(id);
        }
        FontContext::with_database(std::sync::Arc::new(db))
    };
    let first = make(false);
    let second = make(true);
    assert_eq!(first.font_set_id(), second.font_set_id());
    let a = first.fallback_face_for_char('你').unwrap();
    let b = second.fallback_face_for_char('你').unwrap();
    assert_ne!(a.id, b.id, "the fixture must exercise different local ids");
    for data in [a, b] {
        let face = ttf_parser::Face::parse(&data.bytes, data.face_index).unwrap();
        assert!(face.glyph_index('你').is_some());
    }
}

#[test]
fn cluster_fallback_requires_one_face_covering_every_character() {
    let mut db = fontdb::Database::new();
    db.load_font_data(BUNDLED_FONTS[0].data.to_vec());
    db.load_font_data(BUNDLED_FONTS[1].data.to_vec());
    let ctx = FontContext::with_database(std::sync::Arc::new(db));
    // Open Sans covers `e`, and the bundled Noto subset covers `中`, but no
    // face in this fixture covers both. Cluster fallback must not claim a
    // face by combining two scalar-level fallback results.
    let latin = ctx.fallback_face_for_char('e').unwrap();
    let han = ctx.fallback_face_for_char('中').unwrap();
    assert_ne!(latin.id, han.id);
    assert!(ctx.fallback_face_for_cluster(&['e', '中']).is_none());
}
