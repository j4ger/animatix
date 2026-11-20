//! Resolve authored narration anchors against the scene's speech transcript.

use animatix_syntax::narration::{AnchorEdge, AnchorKey, normalize};
use animatix_text::transcript::{TranscriptSegment, TranscriptWord};

pub use animatix_text::transcript::{SpeechTranscript, parse_transcript_auto};

/// Resolve an anchor to clip-relative seconds.
///
/// Whole paragraphs can use segment boundaries in a subtitle-only transcript.
/// A phrase uses its first or last timed word, including when the transcript
/// groups several Han characters into a single timed word.
pub fn resolve_anchor(transcript: &SpeechTranscript, key: &AnchorKey) -> Option<f64> {
    // Missing takes are intentional in a managed partial chapter. Falling
    // back to text here could assign the take to an unvoiced duplicate.
    if let Some(takes) = &transcript.narration_takes {
        let range = takes.get(&key.paragraph_id())?.clone();
        let segments = transcript.segments.get(range)?;
        let text: String = segments.iter().map(|segment| normalize(&segment.text)).collect();
        return key.matches_text(&text).then(|| resolve_segments(segments, key)).flatten();
    }
    let occurrence = key.paragraph_occurrence() - 1;
    if let Some(segment) = transcript
        .segments
        .iter()
        .filter(|segment| key.matches_text(&normalize(&segment.text)))
        .nth(occurrence)
    {
        return resolve_segments(std::slice::from_ref(segment), key);
    }

    let units = transcript.segments.iter().flat_map(|segment| &segment.words);
    let (text, timings) = timed_text(units);
    let chars = text.char_indices().map(|(index, _)| index).collect::<Vec<_>>();
    if chars.len() < key.paragraph_len() {
        return None;
    }
    let range = (0..=chars.len() - key.paragraph_len())
        .filter_map(|start| {
            let end = start + key.paragraph_len();
            let end_byte = chars.get(end).copied().unwrap_or(text.len());
            let candidate = &text[chars[start]..end_byte];
            if (timings[start].word_start || candidate.starts_with(is_han))
                && (timings[end - 1].word_end || candidate.ends_with(is_han))
                && key.matches_text(candidate)
            {
                Some(start..end)
            } else {
                None
            }
        })
        .scan(0, |next, range| {
            let nonoverlapping = range.start >= *next;
            if nonoverlapping {
                *next = range.end;
            }
            Some(nonoverlapping.then_some(range))
        })
        .flatten()
        .nth(occurrence)?;
    let length = range.end - range.start;
    let selection = key.normalized_phrase().unwrap_or(0..length);
    edge_time(
        timings[range.start + selection.start].start,
        timings[range.start + selection.end - 1].end,
        key.edge(),
    )
}

fn resolve_segments(segments: &[TranscriptSegment], key: &AnchorKey) -> Option<f64> {
    let phrase = key.normalized_phrase().unwrap_or(0..key.paragraph_len());
    if phrase.start == 0 && phrase.end == key.paragraph_len() {
        let start = segments.first()?.start_s;
        let end = segments.iter().map(|segment| segment.end_s).max_by(f64::total_cmp)?;
        return edge_time(start, end, key.edge());
    }
    let (text, timings) = timed_text(segments.iter().flat_map(|segment| &segment.words));
    if !key.matches_text(&text) {
        return None;
    }
    edge_time(timings[phrase.start].start, timings[phrase.end - 1].end, key.edge())
}

fn edge_time(start: f64, end: f64, edge: AnchorEdge) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < start {
        return None;
    }
    Some(match edge {
        AnchorEdge::Start => start,
        AnchorEdge::End => end,
    })
}

struct Timing {
    start: f64,
    end: f64,
    word_start: bool,
    word_end: bool,
}

fn timed_text<'a>(words: impl IntoIterator<Item = &'a TranscriptWord>) -> (String, Vec<Timing>) {
    let mut text = String::new();
    let mut timings = Vec::new();
    for word in words {
        let normalized = normalize(&word.word);
        let len = normalized.chars().count();
        for index in 0..len {
            timings.push(Timing {
                start: word.start_s,
                end: word.end_s,
                word_start: index == 0,
                word_end: index + 1 == len,
            });
        }
        text.push_str(&normalized);
    }
    (text, timings)
}

fn is_han(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{2fa1f}' | '\u{30000}'..='\u{323af}')
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::narration::{NarrationAnchor, decode_anchor, encode_anchor};

    fn key(occurrence: usize, phrase: Option<&str>, edge: AnchorEdge) -> AnchorKey {
        let encoded = encode_anchor(&NarrationAnchor {
            paragraph: "这就是猫。".into(),
            paragraph_occurrence: occurrence,
            phrase: phrase.map(str::to_owned),
            phrase_occurrence: 1,
            edge,
        })
        .expect("valid anchor");
        decode_anchor(&encoded).expect("decoded anchor")
    }

    #[test]
    fn managed_transcript_uses_tagged_duplicate_and_supports_split_segments() {
        let paragraph = key(2, None, AnchorEdge::End);
        let transcript = parse_transcript_auto(&format!(
            r#"{{"narration_takes":{{"{}":{{"start":0,"end":2}}}},"segments":[
            {{"start":1,"end":2,"text":"这就","words":[{{"word":"这就","start":1,"end":2}}]}},
            {{"start":2.1,"end":3,"text":"是猫。","words":[{{"word":"是猫","start":2.1,"end":3}}]}}
        ]}}"#,
            paragraph.paragraph_id()
        ))
        .expect("managed speech transcript");

        assert_eq!(resolve_anchor(&transcript, &paragraph), Some(3.0));
        assert_eq!(resolve_anchor(&transcript, &key(2, Some("猫"), AnchorEdge::Start)), Some(2.1));
        assert_eq!(resolve_anchor(&transcript, &key(1, None, AnchorEdge::Start)), None);
    }
}
