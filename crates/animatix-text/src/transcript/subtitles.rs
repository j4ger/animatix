use std::fmt::Write as _;

/// One logical subtitle interval in seconds, with intentional line breaks.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleCue {
    pub start_s: f64,
    pub end_s: f64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleFormat {
    Srt,
    Vtt,
}

impl SubtitleFormat {
    /// Encode plain text cues as UTF-8 SubRip or WebVTT.
    pub fn encode(self, cues: &[SubtitleCue]) -> String {
        let mut output = if self == Self::Vtt {
            "WEBVTT\n\n".to_owned()
        } else {
            String::new()
        };
        let separator = if self == Self::Vtt { '.' } else { ',' };
        let mut number = 0;
        for cue in cues {
            if !cue.start_s.is_finite() || !cue.end_s.is_finite() || cue.end_s <= cue.start_s {
                continue;
            }
            let start = (cue.start_s.max(0.0) * 1000.0).round() as u64;
            let end = (cue.end_s.max(0.0) * 1000.0).round() as u64;
            if end <= start || cue.text.trim().is_empty() {
                continue;
            }
            number += 1;
            if self == Self::Srt {
                let _ = writeln!(output, "{number}"); // Writing into a String cannot fail.
            }
            let _ = writeln!(
                output,
                "{} --> {}",
                timestamp(start, separator),
                timestamp(end, separator)
            );
            let text = cue.text.replace("\r\n", "\n").replace('\r', "\n");
            for line in text.trim().split('\n') {
                // An empty physical line terminates a cue. Keep the visual gap.
                if line.trim().is_empty() {
                    output.push('\u{00a0}');
                } else {
                    output.push_str(
                        &line.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"),
                    );
                }
                output.push('\n');
            }
            output.push('\n');
        }
        output
    }
}

fn timestamp(time_ms: u64, separator: char) -> String {
    let seconds = time_ms / 1000;
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        time_ms % 1000
    )
}
