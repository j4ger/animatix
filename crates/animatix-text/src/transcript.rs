//! Speech transcript, subtitles, and audio synchronization parsing.
//!
//! Supports:
//! - Universal Speech JSON (`.speech.json` / `.transcript.json`)
//! - Raw Whisper JSON format (`segments` with `words`)
//! - WebVTT (`.vtt`)
//! - SubRip (`.srt`)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error encountered while parsing a transcript.
#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("Failed to parse JSON transcript: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Failed to parse timestamp in subtitle: {0}")]
    InvalidTimestamp(String),
    #[error("Invalid or empty subtitle structure")]
    InvalidStructure,
}

/// A word-level timestamp within a speech transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWord {
    /// The spoken word string (trimmed).
    pub word: String,
    /// Start timestamp in seconds.
    pub start_s: f64,
    /// End timestamp in seconds.
    pub end_s: f64,
    /// Optional confidence score (0.0 to 1.0).
    #[serde(default)]
    pub confidence: f32,
}

/// A segment or line cue within a speech transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// 0-indexed segment ID.
    pub id: u32,
    /// Segment start time in seconds.
    pub start_s: f64,
    /// Segment end time in seconds.
    pub end_s: f64,
    /// Complete text of the segment.
    pub text: String,
    /// Optional word-level breakdown.
    #[serde(default)]
    pub words: Vec<TranscriptWord>,
    /// Optional speaker tag (e.g. "Narrator").
    #[serde(default)]
    pub speaker: Option<String>,
}

/// A complete speech transcript with cues, word timings, and markers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SpeechTranscript {
    /// Total duration of audio in seconds (if specified).
    #[serde(default)]
    pub duration: f64,
    /// Named marker timestamps in seconds (e.g. "intro" -> 0.52).
    #[serde(default)]
    pub markers: HashMap<String, f64>,
    /// Subtitle / speech segments in chronological order.
    #[serde(default)]
    pub segments: Vec<TranscriptSegment>,
}

impl SpeechTranscript {
    /// Look up a named marker timestamp in seconds.
    pub fn find_marker(&self, marker: &str) -> Option<f64> {
        self.markers.get(marker).copied()
    }

    /// Look up a spoken word by text and 1-indexed occurrence count.
    /// Returns `Some((start_s, end_s))`.
    pub fn find_word(&self, word: &str, occurrence: usize) -> Option<(f64, f64)> {
        let clean_target = clean_word_token(word);
        if clean_target.is_empty() {
            return None;
        }

        let target_occurrence = occurrence.max(1);
        let mut count = 0;

        for seg in &self.segments {
            for w in &seg.words {
                let clean_w = clean_word_token(&w.word);
                if clean_w.eq_ignore_ascii_case(&clean_target) {
                    count += 1;
                    if count == target_occurrence {
                        return Some((w.start_s, w.end_s));
                    }
                }
            }
        }
        None
    }

    /// Get the active segment for a given time in seconds.
    pub fn active_segment_at(&self, time_s: f64) -> Option<&TranscriptSegment> {
        self.segments.iter().find(|seg| time_s >= seg.start_s && time_s <= seg.end_s)
    }

    /// Get the index of the currently active word in `segment` at `time_s`.
    pub fn active_word_index_at(&self, segment: &TranscriptSegment, time_s: f64) -> Option<usize> {
        segment.words.iter().position(|w| time_s >= w.start_s && time_s <= w.end_s)
    }
}

/// Strip punctuation and whitespace from a word token for fuzzy matching.
pub fn clean_word_token(s: &str) -> String {
    s.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_string()
}

/// Automatically detect format and parse transcript content.
pub fn parse_transcript_auto(content: &str) -> Result<SpeechTranscript, TranscriptError> {
    let trimmed = content.trim_start();
    if trimmed.starts_with("WEBVTT") {
        parse_vtt(content)
    } else if trimmed.starts_with('{') {
        parse_transcript_json(content)
    } else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) && trimmed.contains("-->")
    {
        parse_srt(content)
    } else {
        // Fallback to JSON parser
        parse_transcript_json(content)
    }
}

/// Internal JSON schema structures to accept both Whisper and Universal formats.
#[derive(Deserialize)]
struct RawTranscriptJson {
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    markers: Option<HashMap<String, f64>>,
    #[serde(default)]
    segments: Option<Vec<RawSegmentJson>>,
    #[serde(default)]
    cues: Option<Vec<RawSegmentJson>>,
}

#[derive(Deserialize)]
struct RawSegmentJson {
    #[serde(default)]
    id: Option<u32>,
    #[serde(default, alias = "start_s", alias = "start_time")]
    start: Option<f64>,
    #[serde(default, alias = "end_s", alias = "end_time")]
    end: Option<f64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    words: Option<Vec<RawWordJson>>,
}

#[derive(Deserialize)]
struct RawWordJson {
    #[serde(alias = "w", alias = "text")]
    word: String,
    #[serde(alias = "start_s", alias = "start_time")]
    start: f64,
    #[serde(alias = "end_s", alias = "end_time")]
    end: f64,
    #[serde(default)]
    probability: Option<f32>,
    #[serde(default)]
    confidence: Option<f32>,
}

/// Parse Whisper JSON or Universal Speech JSON.
pub fn parse_transcript_json(json_str: &str) -> Result<SpeechTranscript, TranscriptError> {
    let raw: RawTranscriptJson = serde_json::from_str(json_str)?;

    let mut transcript = SpeechTranscript {
        duration: raw.duration.unwrap_or(0.0),
        markers: raw.markers.unwrap_or_default(),
        segments: Vec::new(),
    };

    let segments_source = raw.segments.or(raw.cues).unwrap_or_default();
    for (i, seg) in segments_source.into_iter().enumerate() {
        let start_s = seg.start.unwrap_or(0.0);
        let end_s = seg.end.unwrap_or(start_s);
        let text = seg.text.unwrap_or_default().trim().to_string();

        let words = seg
            .words
            .unwrap_or_default()
            .into_iter()
            .map(|w| TranscriptWord {
                word: w.word.trim().to_string(),
                start_s: w.start,
                end_s: w.end,
                confidence: w.confidence.or(w.probability).unwrap_or(1.0),
            })
            .collect();

        transcript.segments.push(TranscriptSegment {
            id: seg.id.unwrap_or(i as u32),
            start_s,
            end_s,
            text,
            words,
            speaker: seg.speaker,
        });
    }

    if transcript.duration == 0.0 {
        if let Some(last) = transcript.segments.last() {
            transcript.duration = last.end_s;
        }
    }

    Ok(transcript)
}

/// Parse WebVTT format content.
pub fn parse_vtt(vtt_str: &str) -> Result<SpeechTranscript, TranscriptError> {
    parse_timed_subtitles(vtt_str, '.')
}

/// Parse SubRip (SRT) format content.
pub fn parse_srt(srt_str: &str) -> Result<SpeechTranscript, TranscriptError> {
    parse_timed_subtitles(srt_str, ',')
}

fn parse_timed_subtitles(
    content: &str,
    decimal_sep: char,
) -> Result<SpeechTranscript, TranscriptError> {
    let mut segments = Vec::new();
    let mut id_counter = 0;

    let blocks = content.replace("\r\n", "\n").replace('\r', "\n");
    let chunks = blocks.split("\n\n");

    for chunk in chunks {
        let lines: Vec<&str> = chunk.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            continue;
        }

        // Skip WEBVTT header line if present
        if lines[0].starts_with("WEBVTT") {
            continue;
        }

        // Find the timing line with "-->"
        let timing_line_idx = match lines.iter().position(|l| l.contains("-->")) {
            Some(idx) => idx,
            None => continue,
        };

        let timing_line = lines[timing_line_idx];
        let parts: Vec<&str> = timing_line.split("-->").map(str::trim).collect();
        if parts.len() < 2 {
            continue;
        }

        let start_str = parts[0].split_whitespace().next().unwrap_or("");
        let end_str = parts[1].split_whitespace().next().unwrap_or("");

        let start_s = parse_sub_timestamp(start_str, decimal_sep)?;
        let end_s = parse_sub_timestamp(end_str, decimal_sep)?;

        let text_lines = &lines[(timing_line_idx + 1)..];
        let text = text_lines.join(" ");

        let words: Vec<TranscriptWord> = text
            .split_whitespace()
            .map(|w| TranscriptWord {
                word: w.to_string(),
                start_s,
                end_s,
                confidence: 1.0,
            })
            .collect();

        segments.push(TranscriptSegment {
            id: id_counter,
            start_s,
            end_s,
            text,
            words,
            speaker: None,
        });
        id_counter += 1;
    }

    let duration = segments.last().map(|s| s.end_s).unwrap_or(0.0);
    Ok(SpeechTranscript {
        duration,
        markers: HashMap::new(),
        segments,
    })
}

fn parse_sub_timestamp(ts: &str, decimal_sep: char) -> Result<f64, TranscriptError> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() < 2 {
        return Err(TranscriptError::InvalidTimestamp(ts.to_string()));
    }

    let (hours, mins, secs_str) = if parts.len() == 3 {
        let h: f64 = parts[0]
            .parse()
            .map_err(|_| TranscriptError::InvalidTimestamp(ts.to_string()))?;
        let m: f64 = parts[1]
            .parse()
            .map_err(|_| TranscriptError::InvalidTimestamp(ts.to_string()))?;
        (h, m, parts[2])
    } else {
        let m: f64 = parts[0]
            .parse()
            .map_err(|_| TranscriptError::InvalidTimestamp(ts.to_string()))?;
        (0.0, m, parts[1])
    };

    let normalized_secs = secs_str.replace(decimal_sep, ".");
    let s: f64 = normalized_secs
        .parse()
        .map_err(|_| TranscriptError::InvalidTimestamp(ts.to_string()))?;

    Ok(hours * 3600.0 + mins * 60.0 + s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_universal_json() {
        let json = r#"{
            "audio": "test.mp3",
            "duration": 10.5,
            "markers": {
                "intro": 0.5,
                "step1": 3.2
            },
            "segments": [
                {
                    "id": 0,
                    "start": 0.5,
                    "end": 2.5,
                    "text": "Welcome to Animatix.",
                    "words": [
                        {"word": "Welcome", "start": 0.5, "end": 1.0},
                        {"word": "to", "start": 1.0, "end": 1.3},
                        {"word": "Animatix.", "start": 1.3, "end": 2.5}
                    ]
                }
            ]
        }"#;

        let transcript = parse_transcript_auto(json).unwrap();
        assert_eq!(transcript.duration, 10.5);
        assert_eq!(transcript.find_marker("intro"), Some(0.5));
        assert_eq!(transcript.find_marker("step1"), Some(3.2));
        assert_eq!(transcript.find_word("Welcome", 1), Some((0.5, 1.0)));
        assert_eq!(transcript.find_word("animatix", 1), Some((1.3, 2.5)));
    }

    #[test]
    fn test_parse_whisper_json() {
        let json = r#"{
            "text": "Hello world",
            "segments": [
                {
                    "id": 0,
                    "start": 1.2,
                    "end": 3.8,
                    "text": "Hello world",
                    "words": [
                        {"word": "Hello", "start": 1.2, "end": 2.0, "probability": 0.99},
                        {"word": "world", "start": 2.1, "end": 3.8, "probability": 0.95}
                    ]
                }
            ]
        }"#;

        let transcript = parse_transcript_auto(json).unwrap();
        assert_eq!(transcript.segments.len(), 1);
        assert_eq!(transcript.segments[0].words.len(), 2);
        assert_eq!(transcript.find_word("hello", 1), Some((1.2, 2.0)));
    }

    #[test]
    fn test_parse_vtt() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.500\nHello from WebVTT\n\n00:00:05.000 --> 00:00:08.000\nSecond line\n";
        let transcript = parse_transcript_auto(vtt).unwrap();
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[0].start_s, 1.0);
        assert_eq!(transcript.segments[0].end_s, 4.5);
        assert_eq!(transcript.segments[0].text, "Hello from WebVTT");
    }

    #[test]
    fn test_parse_srt() {
        let srt = "1\n00:00:01,500 --> 00:00:03,000\nSubRip Subtitle\n\n2\n00:00:04,000 --> 00:00:06,000\nNext line\n";
        let transcript = parse_transcript_auto(srt).unwrap();
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[0].start_s, 1.5);
        assert_eq!(transcript.segments[0].end_s, 3.0);
        assert_eq!(transcript.segments[0].text, "SubRip Subtitle");
    }

    #[test]
    fn test_word_occurrence() {
        let json = r#"{
            "segments": [
                {
                    "id": 0,
                    "start": 0.0,
                    "end": 5.0,
                    "text": "test and test again",
                    "words": [
                        {"word": "test", "start": 0.0, "end": 1.0},
                        {"word": "and", "start": 1.1, "end": 1.5},
                        {"word": "test", "start": 1.6, "end": 2.5},
                        {"word": "again", "start": 2.6, "end": 3.5}
                    ]
                }
            ]
        }"#;

        let transcript = parse_transcript_auto(json).unwrap();
        assert_eq!(transcript.find_word("test", 1), Some((0.0, 1.0)));
        assert_eq!(transcript.find_word("test", 2), Some((1.6, 2.5)));
        assert_eq!(transcript.find_word("test", 3), None);
    }
}
