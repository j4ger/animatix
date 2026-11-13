//! Plain-text fast-path classification and glyph-coverage helpers.

use crate::TextKind;
use crate::font_context::FontContext;

/// Returns `true` iff the string contains no Typst markup-special characters
/// and no newlines (single-line only).
pub fn is_plain_text(content: &str) -> bool {
    !content.contains('\n')
        && !content.chars().any(|c| {
            matches!(
                c,
                '*' | '_'
                    | '$'
                    | '\\'
                    | '#'
                    | '<'
                    | '>'
                    | '~'
                    | '`'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '/'
                    | '@'
            )
        })
}

/// Returns `true` iff the character is a CJK ideograph, syllabary, or punctuation mark.
pub fn is_cjk_char(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2EFF   // CJK Radicals Supplement
        | 0x3000..=0x303F // CJK Symbols and Punctuation
        | 0x3040..=0x309F // Hiragana
        | 0x30A0..=0x30FF // Katakana
        | 0x3100..=0x312F // Bopomofo
        | 0x3200..=0x32FF // Enclosed CJK Letters and Months
        | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
        | 0x20000..=0x2FA1F // CJK Unified Ideographs Ext B..F & Compat Supp
    )
}

/// Returns `true` iff the string contains only characters in Latin script ranges
/// (Basic Latin, Latin-1 Supplement, Latin Extended-A) plus common whitespace.
/// This classification is informational; ordinary Text always uses the lightweight shaper.
pub fn is_latin_text(content: &str) -> bool {
    content.chars().all(|c| {
        let cp = c as u32;
        // Basic Latin, Latin-1 Supplement, Latin Extended-A
        cp <= 0x017F
            // Spacing modifier letters / IPA Extensions
            || (0x02B0..=0x02FF).contains(&cp)
            // General punctuation block (spaces, dashes, quotes)
            || (0x2000..=0x206F).contains(&cp)
            // Also accept the replacement character \uFFFD (common in font fallback)
            || cp == 0xFFFD
    })
}

/// Characters for which the selected text path has no visible glyph.
/// Ordinary Text uses the same prepared primary and fallback faces as its
/// lightweight shaper; rich kinds inspect the registered rich-font database.
/// Whitespace and shaping controls never produce missing-glyph diagnostics.
pub fn missing_text_glyphs(
    kind: TextKind,
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    allow_fast_path: bool,
    font_ctx: &FontContext,
) -> Vec<char> {
    let takes_fast_path = kind == TextKind::Text;
    let _ = allow_fast_path;

    let chars: Vec<char> = content
        .chars()
        .filter(|c| !c.is_whitespace() && !crate::fast_layout::default_ignorable(*c))
        .collect();
    let mut missing: Vec<char> = Vec::new();

    if takes_fast_path {
        let Ok(sources) =
            crate::fast_layout::resolve_faces(content, family, weight, style, font_ctx, "coverage")
        else {
            return Vec::new();
        };
        let faces: Vec<_> = sources
            .iter()
            .filter_map(|source| ttf_parser::Face::parse(&source.bytes, source.face_index).ok())
            .collect();

        for c in chars {
            if !missing.contains(&c) && faces.iter().all(|face| face.glyph_index(c).is_none()) {
                missing.push(c);
            }
        }
    } else {
        // Typst-path text: its fallback chain can draw from any registered
        // face, so the honest coverage predicate is the whole database.
        #[cfg(feature = "rich-text")]
        {
            let mut covered = vec![false; chars.len()];
            for face_info in font_ctx.db.faces() {
                font_ctx.db.with_face_data(face_info.id, |data, face_index| {
                    let Ok(face) = ttf_parser::Face::parse(data, face_index) else {
                        return;
                    };
                    for (index, c) in chars.iter().enumerate() {
                        if !covered[index] && face.glyph_index(*c).is_some() {
                            covered[index] = true;
                        }
                    }
                });
            }
            for (index, c) in chars.iter().enumerate() {
                if !covered[index] && !missing.contains(c) {
                    missing.push(*c);
                }
            }
        }
    }

    missing
}
