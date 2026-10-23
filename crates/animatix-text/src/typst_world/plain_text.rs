//! Plain-text fast-path classification and glyph-coverage helpers.

use crate::TextKind;
use crate::font_context::{BUNDLED_FONTS, FontContext, bundled_face, resolve_font_family};

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
/// Non-Latin text (CJK, Arabic, Cyrillic, etc.) falls back to the Typst path.
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

/// Characters in `content` that the render path has no glyph for and will
/// draw as tofu. Which face set governs follows [`crate::compile_text_cached`]'s
/// selection, cfg included: fast-path text (plain, Latin, `allow_fast_path`)
/// is checked against the family's face plus the bundled fallback set, and
/// everything else goes through the Typst engine, whose fallback chain draws
/// from the whole registered database — a character no registered face covers
/// is tofu there too.
///
/// Whitespace is never reported. Empty when the fast-path family cannot be
/// loaded at all, because [`crate::fast_layout::compile_text_fast`] then fails with a proper
/// error of its own.
pub fn missing_text_glyphs(
    kind: TextKind,
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    allow_fast_path: bool,
    font_ctx: &FontContext,
) -> Vec<char> {
    #[cfg(feature = "rich-text")]
    let takes_fast_path = allow_fast_path
        && kind == TextKind::Text
        && is_plain_text(content)
        && is_latin_text(content);
    #[cfg(not(feature = "rich-text"))]
    let takes_fast_path = true;
    let _ = (kind, allow_fast_path);

    let chars: Vec<char> = content.chars().filter(|c| !c.is_whitespace()).collect();
    let mut missing: Vec<char> = Vec::new();

    if takes_fast_path {
        // Same face resolution as `compile_text_fast`: the family's bundled
        // face if bundled, else the registered database, then every other
        // bundled face as the per-char fallback set.
        let resolved_family = resolve_font_family(family, font_ctx);
        let mut faces: Vec<ttf_parser::Face<'static>> = Vec::new();
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            if let Ok(face) = ttf_parser::Face::parse(bf.data, 0) {
                faces.push(face);
            }
        }
        if faces.is_empty() {
            match font_ctx.load_face(&resolved_family, weight, style) {
                Some(face) => faces.push(face),
                // `compile_text_fast` fails with a clear error of its own in
                // this case; the probe stays quiet rather than double-reporting.
                None => return Vec::new(),
            }
        }
        faces.extend(
            BUNDLED_FONTS
                .iter()
                .filter(|bf| bf.family != resolved_family)
                .filter_map(|bf| ttf_parser::Face::parse(bf.data, 0).ok()),
        );

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
