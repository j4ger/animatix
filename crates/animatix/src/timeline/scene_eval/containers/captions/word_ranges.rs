use animatix_text::transcript::TranscriptWord;
use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Mutex, OnceLock};
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_segmentation::UnicodeSegmentation;

const WARNED_UNMATCHED_CAPTION_WORDS: usize = 128;

pub(super) struct CaptionWordStyle<'a> {
    pub caption_mode: &'a str,
    pub active_color: [f32; 4],
    pub inactive_color: [f32; 4],
    pub base_color: [f32; 4],
    pub pop_scale: f32,
}

/// Map each timed word to its original logical character span in the cue.
pub(super) fn map_timed_words_to_char_ranges(
    text: &str,
    words: &[TranscriptWord],
    cue_label: &str,
) -> Vec<Option<Range<usize>>> {
    let mut cursor = 0;
    words
        .iter()
        .map(|word| {
            let matched = find_word_range(text, cursor, &word.word);
            if let Some(range) = &matched {
                cursor = range.end;
            } else {
                warn_unmatched_word(text, &word.word, cue_label);
            }
            matched.map(|range| byte_range_to_char_range(text, range))
        })
        .collect()
}

/// Find a timed token monotonically, first verbatim and then without only its
/// edge whitespace/punctuation. The returned source range expands to include
/// attached punctuation and complete grapheme clusters.
fn find_word_range(text: &str, cursor: usize, word: &str) -> Option<Range<usize>> {
    if word.is_empty() || cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let exact = find_candidate(text, cursor, word);
    let matched = exact.or_else(|| {
        let core = word.trim_matches(|ch: char| ch.is_whitespace() || is_punctuation(ch));
        (!core.is_empty() && core != word)
            .then(|| find_candidate(text, cursor, core))
            .flatten()
    })?;
    let matched = snap_to_grapheme_boundaries(text, matched, cursor)?;
    Some(expand_attached_punctuation(text, matched, cursor))
}

fn find_candidate(text: &str, cursor: usize, needle: &str) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    for (relative_start, _) in text[cursor..].match_indices(needle) {
        let start = cursor + relative_start;
        let end = start + needle.len();
        if token_boundaries_match(text, start, end, needle) {
            return Some(start..end);
        }
    }
    for (relative_start, _) in text[cursor..].char_indices() {
        let start = cursor + relative_start;
        let end = start + needle.len();
        if end <= text.len()
            && text.is_char_boundary(end)
            && text[start..end].eq_ignore_ascii_case(needle)
            && token_boundaries_match(text, start, end, needle)
        {
            return Some(start..end);
        }
    }
    None
}

fn token_boundaries_match(text: &str, start: usize, end: usize, token: &str) -> bool {
    // CJK transcript words are often adjacent without whitespace; monotonic
    // exact matching is the only source boundary available for those tokens.
    if token.chars().any(is_cjk) {
        return true;
    }
    let previous_is_word = text[..start].chars().next_back().is_some_and(char::is_alphanumeric);
    let next_is_word = text[end..].chars().next().is_some_and(char::is_alphanumeric);
    !previous_is_word && !next_is_word
}

fn snap_to_grapheme_boundaries(
    text: &str,
    mut range: Range<usize>,
    minimum_start: usize,
) -> Option<Range<usize>> {
    for (start, grapheme) in text.grapheme_indices(true) {
        let end = start + grapheme.len();
        if start <= range.start && range.start < end {
            if start < minimum_start {
                return None;
            }
            range.start = start;
        }
        if start < range.end && range.end < end {
            range.end = end;
        }
    }
    Some(range)
}

fn expand_attached_punctuation(
    text: &str,
    mut range: Range<usize>,
    minimum_start: usize,
) -> Range<usize> {
    let has_leading_whitespace =
        text[range.clone()].chars().next().is_some_and(char::is_whitespace);
    let has_trailing_whitespace =
        text[range.clone()].chars().next_back().is_some_and(char::is_whitespace);
    // If a fallback match found only a grapheme's base scalar, keep its
    // following combining marks with it. These marks are never edge
    // punctuation and must not be trimmed from ASR tokens.
    while let Some(next) = text[range.end..].chars().next() {
        if !is_combining_mark(next) {
            break;
        }
        range.end += next.len_utf8();
    }
    if !has_leading_whitespace {
        while let Some((start, previous)) = text[..range.start].grapheme_indices(true).next_back() {
            let end = start + previous.len();
            if start < minimum_start
                || previous.chars().any(char::is_whitespace)
                || !is_punctuation_grapheme(previous)
            {
                break;
            }
            range.start = start;
            debug_assert_eq!(end, range.start + previous.len());
        }
    }
    if !has_trailing_whitespace {
        while let Some((start, next)) = text[range.end..].grapheme_indices(true).next() {
            if next.chars().any(char::is_whitespace) || !is_punctuation_grapheme(next) {
                break;
            }
            range.end += start + next.len();
        }
    }
    range
}

fn byte_range_to_char_range(text: &str, range: Range<usize>) -> Range<usize> {
    text[..range.start].chars().count()..text[..range.end].chars().count()
}

fn is_punctuation(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::OtherPunctuation
            | GeneralCategory::OpenPunctuation
    )
}

fn is_combining_mark(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

fn is_punctuation_grapheme(grapheme: &str) -> bool {
    let mut chars = grapheme.chars();
    chars.next().is_some_and(is_punctuation) && chars.all(is_combining_mark)
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2e80..=0x2fff
            | 0x3000..=0x30ff
            | 0x3100..=0x31ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xf900..=0xfaff
            | 0xff00..=0xffef
            | 0x20000..=0x2fa1f
    )
}

fn word_warnings() -> &'static Mutex<VecDeque<(String, String)>> {
    static WARNED: OnceLock<Mutex<VecDeque<(String, String)>>> = OnceLock::new();
    WARNED.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn warn_unmatched_word(text: &str, word: &str, cue_label: &str) {
    let key = (text.to_owned(), word.to_owned());
    let mut warned = word_warnings().lock().unwrap();
    if warned.contains(&key) {
        return;
    }
    if warned.len() == WARNED_UNMATCHED_CAPTION_WORDS {
        warned.pop_front();
    }
    warned.push_back(key);
    tracing::warn!(
        cue = cue_label,
        timed_word = word,
        text,
        "caption timed word does not match cue text; unmatched glyphs keep base color"
    );
}

pub(super) fn word_index_for_char(
    ranges: &[Option<Range<usize>>],
    char_idx: usize,
) -> Option<usize> {
    ranges
        .iter()
        .position(|range| range.as_ref().is_some_and(|range| range.contains(&char_idx)))
}

pub(super) fn style_for_glyph(
    ranges: &[Option<Range<usize>>],
    char_idx: usize,
    active_word_idx: Option<usize>,
    style: &CaptionWordStyle<'_>,
) -> ([f32; 4], f32) {
    let Some(word_idx) = word_index_for_char(ranges, char_idx) else {
        return (style.base_color, 1.0);
    };
    super::caption_glyph_style(
        style.caption_mode,
        active_word_idx == Some(word_idx),
        active_word_idx.is_some_and(|active| word_idx < active),
        style.active_color,
        style.inactive_color,
        style.base_color,
        style.pop_scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[&str]) -> Vec<TranscriptWord> {
        values
            .iter()
            .enumerate()
            .map(|(index, word)| TranscriptWord {
                word: (*word).to_string(),
                start_s: index as f64,
                end_s: index as f64 + 1.0,
                confidence: 1.0,
            })
            .collect()
    }

    #[test]
    fn cjk_ranges_cover_complete_transcript_words_and_logical_glyph_indices() {
        let ranges =
            map_timed_words_to_char_ranges("字幕与时间", &words(&["字幕与", "时间"]), "cue");
        assert_eq!(ranges, [Some(0..3), Some(3..5)]);
        assert_eq!(word_index_for_char(&ranges, 0), Some(0));
        assert_eq!(word_index_for_char(&ranges, 2), Some(0));
        assert_eq!(word_index_for_char(&ranges, 3), Some(1));
        assert_eq!(word_index_for_char(&ranges, 4), Some(1));
    }

    #[test]
    fn latin_multiword_ranges_keep_spaces_outside_timed_words() {
        let ranges =
            map_timed_words_to_char_ranges("hello world", &words(&["hello", "world"]), "cue");
        assert_eq!(ranges, [Some(0..5), Some(6..11)]);
        assert_eq!(word_index_for_char(&ranges, 5), None);
    }

    #[test]
    fn punctuation_and_decomposed_graphemes_stay_with_the_timed_word() {
        let decomposed = "(e\u{301}), next!";
        let ranges =
            map_timed_words_to_char_ranges(decomposed, &words(&["e\u{301}", "next"]), "cue");
        assert_eq!(ranges, [Some(0..5), Some(6..11)]);
        assert_eq!(word_index_for_char(&ranges, 1), Some(0));
        assert_eq!(word_index_for_char(&ranges, 2), Some(0));
        assert_eq!(word_index_for_char(&ranges, 5), None);
    }

    #[test]
    fn bidi_ranges_remain_in_logical_source_order() {
        let text = "שלום עולם";
        let ranges = map_timed_words_to_char_ranges(text, &words(&["שלום", "עולם"]), "cue");
        assert_eq!(ranges, [Some(0..4), Some(5..9)]);
        assert_eq!(word_index_for_char(&ranges, 0), Some(0));
        assert_eq!(word_index_for_char(&ranges, 5), Some(1));
    }

    #[test]
    fn punctuation_in_asr_word_and_source_is_not_dropped() {
        let text = "“Hello,” world!";
        let ranges = map_timed_words_to_char_ranges(text, &words(&["Hello,", "world"]), "cue");
        assert_eq!(ranges, [Some(0..8), Some(9..15)]);
        assert_eq!(word_index_for_char(&ranges, 0), Some(0));
        assert_eq!(word_index_for_char(&ranges, 7), Some(0));
        assert_eq!(word_index_for_char(&ranges, 8), None);
        assert_eq!(word_index_for_char(&ranges, 14), Some(1));
    }

    #[test]
    fn leading_timed_whitespace_does_not_steal_prior_punctuation() {
        let ranges = map_timed_words_to_char_ranges("a, b", &words(&["a", " b"]), "cue");
        assert_eq!(ranges, [Some(0..2), Some(2..4)]);
        assert_eq!(word_index_for_char(&ranges, 1), Some(0));
        assert_eq!(word_index_for_char(&ranges, 2), Some(1));
    }

    #[test]
    fn attached_punctuation_does_not_overlap_adjacent_timed_words() {
        let ranges = map_timed_words_to_char_ranges("a,b", &words(&["a", "b"]), "cue");
        assert_eq!(ranges, [Some(0..2), Some(2..3)]);
        assert_eq!(word_index_for_char(&ranges, 1), Some(0));
        assert_eq!(word_index_for_char(&ranges, 2), Some(1));
    }

    #[test]
    fn unmatched_glyphs_keep_base_color_instead_of_disappearing() {
        let ranges = map_timed_words_to_char_ranges("caption", &words(&["mismatch"]), "cue");
        assert_eq!(ranges, [None]);
        let style = CaptionWordStyle {
            caption_mode: "pop",
            active_color: [1.0, 0.0, 0.0, 1.0],
            inactive_color: [0.0, 0.0, 1.0, 1.0],
            base_color: [0.5, 0.5, 0.5, 1.0],
            pop_scale: 1.25,
        };
        assert_eq!(style_for_glyph(&ranges, 0, Some(0), &style), (style.base_color, 1.0));
    }

    fn fixture_fonts(with_cjk: bool) -> animatix_text::FontContext {
        let mut fonts = animatix_text::FontContext::empty();
        fonts.load_font_bytes(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../animatix-text/assets/fonts/OpenSans-Regular.ttf"
            ))
            .to_vec(),
        );
        if with_cjk {
            fonts.load_font_bytes(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../animatix-text/assets/fonts/NotoSansSC-Common.ttf"
                ))
                .to_vec(),
            );
        }
        fonts
    }

    fn compiled_paths(
        text: &str,
        fonts: &animatix_text::FontContext,
    ) -> Vec<animatix_text::TextPath> {
        let request = animatix_text::TextCompileRequest::new(
            animatix_text::TextKind::Text,
            text,
            animatix_text::TextStyle::new("Open Sans", 32.0),
        );
        animatix_text::compile_text_cached(&request, fonts)
            .expect("fixture text compiles")
            .paths
            .to_vec()
    }

    #[test]
    fn actual_cjk_glyph_indices_map_to_their_timed_transcript_words() {
        let text = "字幕与时间";
        let ranges = map_timed_words_to_char_ranges(text, &words(&["字幕", "与", "时间"]), "cue");
        let glyphs = compiled_paths(text, &fixture_fonts(true));
        assert_eq!(glyphs.len(), 5);
        assert!(glyphs.iter().all(|glyph| glyph.bounds.width() > 0.0));
        let style = CaptionWordStyle {
            caption_mode: "pop",
            active_color: [1.0, 0.0, 0.0, 1.0],
            inactive_color: [0.0, 0.0, 1.0, 1.0],
            base_color: [1.0; 4],
            pop_scale: 1.25,
        };
        for glyph in glyphs {
            let idx = usize::from(glyph.char_idx);
            let (color, scale) = style_for_glyph(&ranges, idx, Some(1), &style);
            match idx {
                0..=1 => assert_eq!(color, style.inactive_color),
                2 => {
                    assert_eq!(color, style.active_color);
                    assert_eq!(scale, 1.25);
                },
                3..=4 => assert_eq!(color, style.inactive_color),
                _ => panic!("unexpected logical char index {idx}"),
            }
        }
    }

    #[test]
    fn actual_bidi_and_combining_glyph_indices_stay_logical() {
        for (text, timed_words) in [
            ("abc \u{202b}123\u{202c} xyz", vec!["abc", "123", "xyz"]),
            ("e\u{301} next", vec!["e\u{301}", "next"]),
        ] {
            let ranges = map_timed_words_to_char_ranges(text, &words(&timed_words), "cue");
            let glyphs = compiled_paths(text, &fixture_fonts(false));
            assert!(!glyphs.is_empty());
            assert!(glyphs.iter().all(|glyph| glyph.bounds.width() > 0.0));
            let style = CaptionWordStyle {
                caption_mode: "pop",
                active_color: [1.0, 0.0, 0.0, 1.0],
                inactive_color: [0.0, 0.0, 1.0, 1.0],
                base_color: [1.0; 4],
                pop_scale: 1.25,
            };
            for glyph in glyphs {
                let idx = usize::from(glyph.char_idx);
                let word_idx = word_index_for_char(&ranges, idx)
                    .expect("rendered logical glyph maps to a timed word");
                let expected = match text {
                    "abc \u{202b}123\u{202c} xyz" => {
                        if idx < 3 {
                            0
                        } else if (5..8).contains(&idx) {
                            1
                        } else {
                            2
                        }
                    },
                    _ => usize::from(idx >= 3),
                };
                if text == "abc \u{202b}123\u{202c} xyz" && (3..5).contains(&idx) {
                    continue;
                }
                assert_eq!(word_idx, expected);
                let (color, scale) = style_for_glyph(&ranges, idx, Some(1), &style);
                assert_eq!(
                    color,
                    if expected == 1 {
                        style.active_color
                    } else {
                        style.inactive_color
                    }
                );
                assert_eq!(scale, if expected == 1 { 1.25 } else { 1.0 });
            }
        }
    }
}
