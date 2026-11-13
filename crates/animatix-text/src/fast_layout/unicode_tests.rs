use super::compile_text_fast;
use super::layout::{ParagraphBidi, parse_faces, resolve_faces, shape_fragment, tokenize_words};
use crate::{BUNDLED_FONTS, CompiledText, FontContext, TextPath};
use std::collections::{BTreeMap, HashSet};

fn bundled_context() -> FontContext {
    let mut context = FontContext::empty();
    // Keep the fast-layout regression fixtures deterministic with and without
    // the crate's optional bundled-font feature.
    context.load_font_bytes(include_bytes!("../../assets/fonts/OpenSans-Regular.ttf").to_vec());
    for font in BUNDLED_FONTS {
        context.load_font_bytes(font.data.to_vec());
    }
    context.load_font_bytes(
        include_bytes!(
            "../../../../tools/animatix-web-build/fixtures/text-parity/fonts/NotoNaskhArabicUI.ttf"
        )
        .to_vec(),
    );
    context
}

fn actual_visual_char_indices<'a>(glyphs: impl IntoIterator<Item = &'a TextPath>) -> Vec<usize> {
    let mut positioned: Vec<_> = glyphs
        .into_iter()
        .map(|glyph| ((glyph.bounds.x0 + glyph.bounds.x1) * 0.5, glyph.char_idx as usize))
        .collect();
    positioned.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut seen = HashSet::new();
    positioned
        .into_iter()
        .filter_map(|(_, char_idx)| seen.insert(char_idx).then_some(char_idx))
        .collect()
}

/// Character-order oracle: ask Unicode bidi for paragraph/line visual runs,
/// then apply L2 within odd runs. This intentionally works at character level,
/// independently of the fast path's token/fragment mapping.
fn unicode_visual_char_indices(text: &str, range: std::ops::Range<usize>) -> Vec<usize> {
    let bidi = unicode_bidi::BidiInfo::new(text, None);
    let Some(paragraph) = bidi.paragraphs.first() else {
        return Vec::new();
    };
    let (levels, runs) = bidi.visual_runs(paragraph, range);
    let mut output = Vec::new();
    for run in runs {
        let char_base = text[..run.start].chars().count();
        let mut chars: Vec<_> = text[run.clone()]
            .char_indices()
            .filter_map(|(byte, ch)| {
                (!ch.is_whitespace() && !super::layout::default_ignorable(ch))
                    .then_some(char_base + text[run.start..run.start + byte].chars().count())
            })
            .collect();
        if levels.get(run.start).is_some_and(|level| level.is_rtl()) {
            chars.reverse();
        }
        output.extend(chars);
    }
    output
}

fn assert_matches_unicode_order(text: &str, font_family: &str) -> CompiledText {
    let context = bundled_context();
    let compiled =
        compile_text_fast(text, font_family, 400.0, "normal", 32.0, [1.0; 4], 0.0, 0.0, &context)
            .unwrap();
    let actual = actual_visual_char_indices(&compiled.glyphs);
    let actual_chars: HashSet<_> = actual.iter().copied().collect();
    let expected: Vec<_> = unicode_visual_char_indices(text, 0..text.len())
        .into_iter()
        .filter(|char_idx| actual_chars.contains(char_idx))
        .collect();
    assert_eq!(actual, expected, "visual char order for {text:?}");
    compiled
}

#[test]
fn unicode_breaks_preserve_graphemes_and_source_offsets() {
    let text = "e\u{301} 中文👨\u{200d}👩\u{200d}👧\u{200d}👦";
    let tokens = tokenize_words(text);
    assert_eq!(tokens[0].text, "e\u{301}");
    assert_eq!(tokens[0].char_start, 0);
    assert_eq!(tokens[1].text, "中");
    assert_eq!(tokens[1].char_start, 3);
    assert_eq!(tokens[2].text, "文");
    assert_eq!(tokens[2].char_start, 4);
    assert!(tokens.iter().any(|token| token.text == "👨\u{200d}👩\u{200d}👧\u{200d}👦"));
    for token in tokens {
        assert_eq!(&text[token.byte_start..token.byte_end], token.text);
        assert_eq!(text[..token.byte_start].chars().count(), token.char_start);
    }
}

#[test]
fn nonbreaking_spaces_stay_inside_a_single_token() {
    let tokens = tokenize_words("a\u{a0}b c\u{202f}d");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].text, "a\u{a0}b");
    assert_eq!(tokens[1].text, "c\u{202f}d");
}

#[test]
fn bidi_reorders_words_but_keeps_logical_indices() {
    let text = "שלום עולם";
    let tokens = tokenize_words(text);
    let bidi = ParagraphBidi::new(text);
    let ranges: Vec<_> = tokens.iter().map(|token| (token.byte_start, token.byte_end)).collect();
    assert_eq!(bidi.visual_order_for_ranges(&ranges, 0..text.len()).unwrap(), vec![1, 0]);
    let mixed = "hello שלום עולם";
    let tokens = tokenize_words(mixed);
    let ranges: Vec<_> = tokens.iter().map(|token| (token.byte_start, token.byte_end)).collect();
    assert_eq!(
        ParagraphBidi::new(mixed)
            .visual_order_for_ranges(&ranges, 0..mixed.len())
            .unwrap(),
        vec![0, 2, 1]
    );
}

#[test]
fn explicit_rle_pdf_reorders_paired_punctuation_and_numbers() {
    let text = "LTR 12 \u{202b}34 (56)\u{202c} end";
    let context = bundled_context();
    let compiled =
        compile_text_fast(text, "Open Sans", 400.0, "normal", 32.0, [1.0; 4], 0.0, 0.0, &context)
            .unwrap();
    let glyphs = &compiled.glyphs;
    let actual = actual_visual_char_indices(glyphs);
    assert_eq!(actual, vec![0, 1, 2, 4, 5, 14, 12, 13, 11, 8, 9, 17, 18, 19]);
    assert_eq!(actual, unicode_visual_char_indices(text, 0..text.len()));
    assert_source_word_indices(text, glyphs);
    assert_parentheses_are_mirrored(&context);
}

#[test]
fn lri_rli_fsi_and_nested_embeddings_match_unicode_visual_order() {
    for text in [
        "left \u{2066}abc 123\u{2069} end",
        "left \u{2067}12 34\u{2069} end",
        "left \u{2068}אב 12\u{2069} end",
        "a \u{202b}12 \u{202a}34\u{202c} 56\u{202c} z",
        "A\t\u{202b}12\t(34)\u{202c}\tZ",
    ] {
        let compiled = assert_matches_unicode_order(text, "Open Sans");
        assert_source_word_indices(text, &compiled.glyphs);
    }
}

#[test]
fn arabic_words_and_latin_digits_match_unicode_visual_order() {
    let text = "مرحبا بالعالم 123";
    let compiled = assert_matches_unicode_order(text, "Noto Naskh Arabic UI");
    let actual = actual_visual_char_indices(&compiled.glyphs);
    let expected = unicode_visual_char_indices(text, 0..text.len());
    assert_eq!(actual, expected, "the fixture has no ligature clusters");
    let expected_chars: HashSet<_> = expected.iter().copied().collect();
    let actual_chars: HashSet<_> =
        compiled.glyphs.iter().map(|glyph| glyph.char_idx as usize).collect();
    assert_eq!(actual_chars, expected_chars, "all Arabic and digit clusters draw");
    assert_hardware_glyphs_present(&compiled, expected.len());
    assert_source_word_indices(text, &compiled.glyphs);
}

#[test]
fn wrapped_visual_runs_keep_logical_word_indices_and_line_order() {
    let text = "start \u{202b}alpha beta (12) gamma\u{202c} end";
    let context = bundled_context();
    let compiled = super::compile_text_fast_wrapped(
        text,
        "Open Sans",
        400.0,
        "normal",
        24.0,
        1.2,
        [1.0; 4],
        0.0,
        0.0,
        &context,
        150.0,
        None,
        "left",
        "visible",
    )
    .unwrap();
    let tokens = tokenize_words(text);
    let mut lines: BTreeMap<u16, Vec<&TextPath>> = BTreeMap::new();
    for glyph in &compiled.glyphs {
        lines.entry(glyph.line_idx).or_default().push(glyph);
        let token = &tokens[glyph.word_idx as usize];
        let char_start = token.char_start;
        let char_end = char_start + token.text.chars().count();
        assert!((char_start..char_end).contains(&(glyph.char_idx as usize)));
    }
    assert!(lines.len() > 1, "fixture must wrap at the chosen width");
    let mut token_lines = BTreeMap::new();
    for (line_idx, glyphs) in &lines {
        let word_ids: HashSet<_> = glyphs.iter().map(|glyph| glyph.word_idx as usize).collect();
        for word_id in &word_ids {
            assert_eq!(token_lines.insert(*word_id, *line_idx), None, "word split across lines");
        }
        let first = *word_ids.iter().min().unwrap();
        let last = *word_ids.iter().max().unwrap();
        let line_range = tokens[first].byte_start..tokens[last].byte_end;
        let actual = actual_visual_char_indices(glyphs.iter().copied());
        let actual_chars: HashSet<_> = actual.iter().copied().collect();
        let expected: Vec<_> = unicode_visual_char_indices(text, line_range)
            .into_iter()
            .filter(|char_idx| actual_chars.contains(char_idx))
            .collect();
        assert_eq!(actual, expected, "wrapped line {line_idx} visual order");
    }
}

#[test]
fn cjk_following_a_spaced_word_does_not_gain_a_second_gap() {
    let tokens = tokenize_words("one two中文");
    assert_eq!(
        tokens.iter().map(|token| token.text).collect::<Vec<_>>(),
        vec!["one", "two", "中", "文"]
    );
    assert!(tokens[1].space_before);
    assert!(!tokens[2].space_before);
    assert!(!tokens[3].space_before);
}

#[test]
fn positioned_cjk_after_english_word_has_no_implicit_gap() {
    let context = bundled_context();
    let compile = |text: &str| {
        compile_text_fast(text, "Open Sans", 400.0, "normal", 24.0, [1.0; 4], 0.0, 0.0, &context)
            .unwrap()
    };
    let with_prefix = compile("one two中文");
    let pair_only = compile("two中文");
    let center = |glyphs: &[TextPath], char_idx| {
        let glyph = glyphs
            .iter()
            .find(|glyph| glyph.char_idx as usize == char_idx)
            .expect("expected source character glyph");
        (glyph.bounds.x0 + glyph.bounds.x1) * 0.5
    };
    let prefixed_spacing = center(&with_prefix.glyphs, 7) - center(&with_prefix.glyphs, 6);
    let pair_spacing = center(&pair_only.glyphs, 3) - center(&pair_only.glyphs, 2);
    assert!(
        (prefixed_spacing - pair_spacing).abs() < 0.01,
        "CJK following an unspaced word boundary must not gain whitespace"
    );
}

#[test]
fn plain_ltr_zero_width_joiner_is_skipped_without_tracking_advance() {
    let context = bundled_context();
    let compile = |text: &str| {
        compile_text_fast(text, "Open Sans", 400.0, "normal", 24.0, [1.0; 4], 2.0, 0.0, &context)
            .unwrap()
    };
    let plain = compile("AB");
    let center = |glyphs: &[TextPath], char_idx| {
        let glyph = glyphs.iter().find(|glyph| glyph.char_idx as usize == char_idx).unwrap();
        (glyph.bounds.x0 + glyph.bounds.x1) * 0.5
    };
    let plain_advance = center(&plain.glyphs, 1) - center(&plain.glyphs, 0);
    for text in ["A\u{200b}B", "A\u{2060}B"] {
        let joined = compile(text);
        assert_eq!(actual_visual_char_indices(&joined.glyphs), vec![0, 2]);
        let joined_advance = center(&joined.glyphs, 2) - center(&joined.glyphs, 0);
        assert!((joined_advance - plain_advance).abs() < 0.01);
    }
    assert!(ParagraphBidi::new("\u{200b}").text_fragments(0..3).is_empty());
    assert!(ParagraphBidi::new("\u{2060}").text_fragments(0..3).is_empty());
}

#[test]
fn tracking_applies_between_grapheme_clusters_not_base_and_mark_glyphs() {
    let context = bundled_context();
    let compile = |letter_spacing| {
        compile_text_fast(
            "x\u{301}b",
            "Open Sans",
            400.0,
            "normal",
            32.0,
            [1.0; 4],
            letter_spacing,
            0.0,
            &context,
        )
        .unwrap()
    };
    let untracked = compile(0.0);
    let tracked = compile(8.0);
    let untracked_cluster = combining_cluster_glyphs(&untracked);
    let tracked_cluster = combining_cluster_glyphs(&tracked);
    let center_x = |glyph: &TextPath| (glyph.bounds.x0 + glyph.bounds.x1) * 0.5;
    let untracked_mark_offset = center_x(untracked_cluster[0]) - center_x(untracked_cluster[1]);
    let tracked_mark_offset = center_x(tracked_cluster[0]) - center_x(tracked_cluster[1]);
    assert!((tracked_mark_offset - untracked_mark_offset).abs() < 0.01);

    let untracked_next = untracked
        .glyphs
        .iter()
        .find(|glyph| glyph.char_idx == 2)
        .expect("next logical cluster glyph");
    let tracked_next = tracked
        .glyphs
        .iter()
        .find(|glyph| glyph.char_idx == 2)
        .expect("next logical cluster glyph");
    let untracked_advance = center_x(untracked_next) - center_x(untracked_cluster[1]);
    let tracked_advance = center_x(tracked_next) - center_x(tracked_cluster[1]);
    assert!((tracked_advance - untracked_advance - 8.0).abs() < 0.1);
}

fn assert_source_word_indices(text: &str, glyphs: &[TextPath]) {
    let tokens = tokenize_words(text);
    for glyph in glyphs {
        let char_idx = glyph.char_idx as usize;
        let token_idx = tokens
            .iter()
            .position(|token| {
                (token.char_start..token.char_start + token.text.chars().count())
                    .contains(&char_idx)
            })
            .expect("glyph source index must map to a logical word");
        assert_eq!(glyph.word_idx as usize, token_idx);
    }
}

fn assert_hardware_glyphs_present(compiled: &CompiledText, minimum: usize) {
    let glyphs: Vec<_> = compiled
        .hardware_layout
        .as_ref()
        .expect("fast layout emits hardware glyphs")
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .collect();
    assert!(glyphs.len() >= minimum);
    assert!(glyphs.iter().all(|glyph| glyph.id != 0));
}

fn assert_parentheses_are_mirrored(context: &FontContext) {
    let sources = resolve_faces("34 (56)", "Open Sans", 400.0, "normal", context, "test").unwrap();
    let faces = parse_faces(&sources, 400.0, "normal");
    let (_, shaped) =
        shape_fragment("34 (56)", 8, &faces, 32.0, 0.0, rustybuzz::Direction::RightToLeft);
    let font = context.load_face_data("Open Sans", 400.0, "normal").unwrap();
    let face = ttf_parser::Face::parse(&font.bytes, font.face_index).unwrap();
    let open = face.glyph_index('(').unwrap().0 as u32;
    let close = face.glyph_index(')').unwrap().0 as u32;
    let source_glyph =
        |char_idx| shaped.iter().find(|glyph| glyph.char_idx as usize == char_idx).unwrap();
    assert_eq!(source_glyph(14).glyph_id as u32, open, "right parenthesis is mirrored");
    assert_eq!(source_glyph(11).glyph_id as u32, close, "left parenthesis is mirrored");
}

fn combining_cluster_glyphs(compiled: &CompiledText) -> Vec<&TextPath> {
    let mut glyphs: Vec<_> = compiled.glyphs.iter().filter(|glyph| glyph.char_idx == 0).collect();
    glyphs.sort_by(|left, right| left.bounds.height().total_cmp(&right.bounds.height()));
    assert_eq!(glyphs.len(), 2, "decomposed x-acute is one two-glyph cluster");
    glyphs
}
