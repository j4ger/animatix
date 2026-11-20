use std::ops::Range;

use super::NarrationParagraph;

const PREFIX_V1: &str = "__narration_v1_";
const PREFIX_V2: &str = "__narration_v2_";
const IMPLICIT_PREFIX: &str = "__narration_implicit_";
const CONTEXT_CHARS: usize = 6;

/// The scene's conventional generated narration actor label.
pub const TRACK: &str = "narration";

/// The selected boundary of a spoken paragraph or phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorEdge {
    /// Start of the selected speech.
    Start,
    /// End of the selected speech.
    End,
}

/// An authored reference to a spoken paragraph or phrase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NarrationAnchor {
    /// The complete authored paragraph text.
    pub paragraph: String,
    /// One-based occurrence of this paragraph's text in the scene.
    pub paragraph_occurrence: usize,
    /// The selected phrase, or the whole paragraph when omitted.
    pub phrase: Option<String>,
    /// One-based occurrence of the phrase in the paragraph, including overlaps.
    pub phrase_occurrence: usize,
    /// Which boundary supplies the timestamp.
    pub edge: AnchorEdge,
}

/// A compact, versioned selector carried by a narration marker.
///
/// A key contains a stable fingerprint rather than the paragraph text. If the
/// text changes after normalization, select the phrase again to update the key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AnchorKey {
    fingerprint: u64,
    len: usize,
    occurrence: usize,
    phrase: Option<Range<usize>>,
    end: bool,
    repair: Option<RepairHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RepairHint {
    phrase: u64,
    before: u16,
    after: u16,
}

impl RepairHint {
    fn decode(encoded: &str) -> Option<Self> {
        if encoded.len() != 24 || !encoded.is_ascii() {
            return None;
        }
        Some(Self {
            phrase: u64::from_str_radix(&encoded[..16], 16).ok()?,
            before: u16::from_str_radix(&encoded[16..20], 16).ok()?,
            after: u16::from_str_radix(&encoded[20..], 16).ok()?,
        })
    }
}

impl AnchorKey {
    fn whole(paragraph: &str, occurrence: usize, edge: AnchorEdge) -> Option<Self> {
        let normalized = normalize(paragraph);
        let len = normalized.chars().count();
        (len > 0 && occurrence > 0).then(|| Self {
            fingerprint: fingerprint(&normalized),
            len,
            occurrence,
            phrase: None,
            end: edge == AnchorEdge::End,
            repair: None,
        })
    }

    /// Whether this key identifies this source paragraph within its scene.
    pub fn matches_paragraph(&self, paragraph: &NarrationParagraph) -> bool {
        self.occurrence == paragraph.occurrence && self.matches_text(&normalize(&paragraph.text))
    }

    /// Whether the supplied normalized text has this paragraph identity.
    pub fn matches_text(&self, normalized: &str) -> bool {
        normalized.chars().count() == self.len && fingerprint(normalized) == self.fingerprint
    }

    /// Whether a second key refers to the same paragraph and occurrence.
    pub fn same_paragraph(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
            && self.len == other.len
            && self.occurrence == other.occurrence
    }

    /// One-based occurrence of this paragraph in a scene or its transcript.
    pub fn paragraph_occurrence(&self) -> usize {
        self.occurrence
    }

    /// Stable paragraph ID used in a managed voice take's transcript.
    pub fn paragraph_id(&self) -> String {
        paragraph_id(self.fingerprint, self.occurrence)
    }

    /// Number of normalized characters in this paragraph.
    pub fn paragraph_len(&self) -> usize {
        self.len
    }

    /// Character range in the normalized paragraph, when selecting a phrase.
    pub fn normalized_phrase(&self) -> Option<Range<usize>> {
        self.phrase.clone()
    }

    /// Byte range of the selected phrase in the original paragraph text.
    pub fn phrase_range(&self, paragraph: &str) -> Option<Range<usize>> {
        let range = self.phrase.clone()?;
        let (normalized, positions) = normalize_with_positions(paragraph);
        self.matches_text(&normalized).then(|| byte_range(&positions, range))
    }

    /// Suggest source byte ranges for the selected phrase after a paragraph edit.
    ///
    /// Matches are ordered by surrounding context, with ties in source order.
    /// Applying a suggestion needs a new binding. Legacy and whole-paragraph
    /// selectors have no phrase repair hint.
    pub fn repair_ranges(&self, paragraph: &str) -> Vec<Range<usize>> {
        let (Some(phrase), Some(hint)) = (&self.phrase, self.repair) else {
            return Vec::new();
        };
        let length = phrase.len();
        let (normalized, positions) = normalize_with_positions(paragraph);
        if length > positions.len() {
            return Vec::new();
        }
        let offsets = byte_offsets(&normalized);
        let mut matches = Vec::new();
        for (start, window) in offsets.windows(length + 1).enumerate() {
            let target = &normalized[window[0]..window[length]];
            if fingerprint(target) == hint.phrase {
                let range = start..start + length;
                let (before, after) = adjacent_context(&normalized, &offsets, &range);
                let score = u8::from(before == hint.before) + u8::from(after == hint.after);
                matches.push((score, byte_range(&positions, range)));
            }
        }
        matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        matches.into_iter().map(|(_, range)| range).collect()
    }

    /// The chosen timestamp boundary.
    pub fn edge(&self) -> AnchorEdge {
        if self.end {
            AnchorEdge::End
        } else {
            AnchorEdge::Start
        }
    }

    fn identifier(&self) -> String {
        let edge = if self.end { 'e' } else { 's' };
        let prefix = if self.repair.is_some() {
            PREFIX_V2
        } else {
            PREFIX_V1
        };
        let mut name =
            format!("{prefix}{:016x}_{}_{}_{edge}", self.fingerprint, self.len, self.occurrence,);
        if let Some(range) = &self.phrase {
            name.push_str(&format!("_{}_{}", range.start, range.end));
        }
        if let Some(hint) = self.repair {
            name.push_str(&format!("_{:016x}{:04x}{:04x}", hint.phrase, hint.before, hint.after));
        }
        name
    }
}

/// Create an identifier for use after `#narration@`.
///
/// Returns `None` if the paragraph or occurrence is invalid, or the selected
/// phrase does not occur that many times in the paragraph.
pub fn encode_anchor(anchor: &NarrationAnchor) -> Option<String> {
    let mut key = AnchorKey::whole(&anchor.paragraph, anchor.paragraph_occurrence, anchor.edge)?;
    if let Some(phrase) = &anchor.phrase {
        let normalized = normalize(&anchor.paragraph);
        let phrase = normalize(phrase);
        let index = anchor.phrase_occurrence.checked_sub(1)?;
        let range = normalized_matches(&normalized, &phrase).nth(index)?;
        let (before, after) = adjacent_context(&normalized, &byte_offsets(&normalized), &range);
        key.repair = Some(RepairHint {
            phrase: fingerprint(&phrase),
            before,
            after,
        });
        key.phrase = Some(range);
    }
    Some(key.identifier())
}

/// Recognize and decode a narration marker identifier.
pub fn decode_anchor(marker: &str) -> Option<AnchorKey> {
    let marker = match marker.strip_prefix(IMPLICIT_PREFIX) {
        Some(implicit) => implicit.split_once('\0').map_or(implicit, |(key, _)| key),
        None => marker,
    };
    let (encoded, has_repair) = if let Some(encoded) = marker.strip_prefix(PREFIX_V2) {
        (encoded, true)
    } else {
        (marker.strip_prefix(PREFIX_V1)?, false)
    };
    let mut fields = encoded.split('_');
    let fingerprint = u64::from_str_radix(fields.next()?, 16).ok()?;
    let len: usize = fields.next()?.parse().ok()?;
    let occurrence: usize = fields.next()?.parse().ok()?;
    let end = match fields.next()? {
        "s" => false,
        "e" => true,
        _ => return None,
    };
    let phrase = match fields.next() {
        None => None,
        Some(start) => {
            let start = start.parse().ok()?;
            let end = fields.next()?.parse().ok()?;
            if start >= end || end > len {
                return None;
            }
            Some(start..end)
        },
    };
    let repair = if has_repair {
        phrase.as_ref()?;
        Some(RepairHint::decode(fields.next()?)?)
    } else {
        None
    };
    (len > 0 && occurrence > 0 && fields.next().is_none()).then_some(AnchorKey {
        fingerprint,
        len,
        occurrence,
        phrase,
        end,
        repair,
    })
}

/// Whether a time marker is synthesized by a standalone paragraph.
pub fn is_implicit_anchor(marker: &str) -> bool {
    marker.starts_with(IMPLICIT_PREFIX) && decode_anchor(marker).is_some()
}

pub(crate) fn implicit_anchor(paragraph: &NarrationParagraph) -> Option<String> {
    let key = AnchorKey::whole(&paragraph.text, paragraph.occurrence, AnchorEdge::Start)?;
    // Internally carry the original text for AST-only serialization. Authored
    // marker names are identifiers and cannot contain this separator.
    Some(format!("{IMPLICIT_PREFIX}{}\0{}", key.identifier(), paragraph.text))
}

pub(crate) fn implicit_anchor_text(marker: &str) -> Option<&str> {
    marker.strip_prefix(IMPLICIT_PREFIX)?.split_once('\0').map(|(_, text)| text)
}

pub(crate) fn strip_implicit_anchor_text(marker: &mut String) {
    if marker.starts_with(IMPLICIT_PREFIX)
        && let Some(separator) = marker.find('\0')
    {
        marker.truncate(separator);
    }
}

/// Find normalized phrase matches as authored UTF-8 ranges, including overlaps.
pub fn phrase_ranges(paragraph: &str, phrase: &str) -> Vec<Range<usize>> {
    let (normalized, positions) = normalize_with_positions(paragraph);
    let phrase = normalize(phrase);
    normalized_matches(&normalized, &phrase)
        .map(|range| byte_range(&positions, range))
        .collect()
}

fn normalized_matches<'a>(
    text: &'a str,
    phrase: &'a str,
) -> impl Iterator<Item = Range<usize>> + 'a {
    let length = phrase.chars().count();
    text.char_indices().enumerate().filter_map(move |(start, (byte, _))| {
        (length > 0 && text[byte..].starts_with(phrase)).then_some(start..start + length)
    })
}

fn byte_range(positions: &[Range<usize>], range: Range<usize>) -> Range<usize> {
    positions[range.start].start..positions[range.end - 1].end
}

fn byte_offsets(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn adjacent_context(text: &str, offsets: &[usize], range: &Range<usize>) -> (u16, u16) {
    let before = range.start.saturating_sub(CONTEXT_CHARS);
    let after = range.end.saturating_add(CONTEXT_CHARS).min(offsets.len() - 1);
    (
        fingerprint(&text[offsets[before]..offsets[range.start]]) as u16,
        fingerprint(&text[offsets[range.end]..offsets[after]]) as u16,
    )
}

/// Fold case and full-width ASCII and ignore punctuation and whitespace.
///
/// Concatenating normalized transcript tokens yields the same text across
/// different word segmentation, including for Han characters.
pub fn normalize(text: &str) -> String {
    text.chars().flat_map(normalized_chars).collect()
}

fn normalize_with_positions(text: &str) -> (String, Vec<Range<usize>>) {
    let mut normalized = String::new();
    let mut positions = Vec::new();
    for (start, ch) in text.char_indices() {
        for normalized_ch in normalized_chars(ch) {
            normalized.push(normalized_ch);
            positions.push(start..start + ch.len_utf8());
        }
    }
    (normalized, positions)
}

fn normalized_chars(ch: char) -> impl Iterator<Item = char> {
    let ch = match ch {
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(ch as u32 - 0xfee0).unwrap_or(ch),
        _ => ch,
    };
    ch.to_lowercase().filter(|ch| ch.is_alphanumeric())
}

pub(super) fn fingerprint(normalized: &str) -> u64 {
    // Stable FNV-1a over UTF-8, part of the versioned source identifier.
    normalized.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

pub(super) fn paragraph_id(fingerprint: u64, occurrence: usize) -> String {
    format!("p_{fingerprint:016x}_{occurrence}")
}
