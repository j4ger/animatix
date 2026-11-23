use std::path::Path;

use animatix::renderer::text::transcript::{SpeechTranscript, parse_transcript_auto};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentEdit {
    id: u32,
    text: Option<String>,
    start_s: Option<f64>,
    end_s: Option<f64>,
    #[serde(default)]
    words: Vec<WordEdit>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WordEdit {
    index: usize,
    word: Option<String>,
    start_s: Option<f64>,
    end_s: Option<f64>,
}

pub fn run(input: &Path, edits: Option<&Path>, output: &Path) -> Result<(), String> {
    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))
    };
    let mut transcript = parse_transcript_auto(&read(input)?).map_err(|error| error.to_string())?;
    if let Some(path) = edits {
        let edits = serde_json::from_str::<Vec<SegmentEdit>>(&read(path)?)
            .map_err(|error| format!("invalid transcript edits: {error}"))?;
        apply_edits(&mut transcript, edits)?;
    }
    let mut json = serde_json::to_string_pretty(&transcript).map_err(|error| error.to_string())?;
    json.push('\n');
    std::fs::write(output, json)
        .map_err(|error| format!("could not write {}: {error}", output.display()))
}

fn apply_edits(transcript: &mut SpeechTranscript, edits: Vec<SegmentEdit>) -> Result<(), String> {
    for edit in edits {
        let mut matching = transcript.segments.iter_mut().filter(|segment| segment.id == edit.id);
        let segment = matching.next().ok_or_else(|| format!("unknown segment ID {}", edit.id))?;
        if matching.next().is_some() {
            return Err(format!("segment ID {} is ambiguous", edit.id));
        }
        segment.start_s = edit.start_s.unwrap_or(segment.start_s);
        segment.end_s = edit.end_s.unwrap_or(segment.end_s);
        if segment.start_s < 0.0 || segment.end_s <= segment.start_s {
            return Err(format!("segment {} needs a positive interval", edit.id));
        }
        if edit.text.is_none() && edit.words.iter().any(|word| word.word.is_some()) {
            return Err(format!(
                "supply the corrected segment text when changing words in segment {}",
                edit.id
            ));
        }
        if let Some(text) = edit.text {
            segment.text = text;
        }
        for edit in edit.words {
            let index = edit.index;
            let word = segment
                .words
                .get_mut(index)
                .ok_or_else(|| format!("segment {} has no word at index {index}", segment.id))?;
            if let Some(text) = edit.word {
                word.word = text;
            }
            word.start_s = edit.start_s.unwrap_or(word.start_s);
            word.end_s = edit.end_s.unwrap_or(word.end_s);
            if word.start_s < segment.start_s
                || word.end_s > segment.end_s
                || word.end_s <= word.start_s
            {
                return Err(format!(
                    "word {index} in segment {} needs a positive interval within the segment",
                    segment.id
                ));
            }
        }
        if !segment.words.windows(2).all(|words| words[0].start_s <= words[1].start_s) {
            return Err(format!("corrected words in segment {} must be in time order", segment.id));
        }
        transcript.duration = transcript.duration.max(segment.end_s);
    }
    Ok(())
}
