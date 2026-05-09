//! Tree-sitter based syntax highlighting for the Animatix DSL.

use animatix_analyzer::Diagnostic;
use egui::text::LayoutJob;
use egui::{Color32, FontId, FontFamily, TextFormat};
use std::sync::LazyLock;
use tree_sitter::{Parser, Language};
use tree_sitter_animatix::{language, HIGHLIGHTS_QUERY};

static LANGUAGE: LazyLock<Language> = LazyLock::new(language);

/// Highlight names used in the queries/highlights.scm file.
/// These map to indices in the highlight configuration.
const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",
    "type",
    "type.builtin",
    "string",
    "number",
    "boolean",
    "comment",
    "operator",
    "punctuation",
    "punctuation.bracket",
    "variable",
    "property",
    "parameter",
    "function",
];

/// Color scheme for syntax highlighting.
struct HighlightColors {
    keyword: Color32,
    type_name: Color32,
    string: Color32,
    number: Color32,
    boolean: Color32,
    comment: Color32,
    operator: Color32,
    punctuation: Color32,
    variable: Color32,
    property: Color32,
    parameter: Color32,
    function: Color32,
    default: Color32,
}

impl HighlightColors {
    fn from_style(style: &egui::Style) -> Self {
        let visuals = &style.visuals;
        let is_dark = visuals.dark_mode;

        if is_dark {
            // Dark theme (Gruvbox-inspired)
            Self {
                keyword: Color32::from_rgb(251, 73, 106),      // red
                type_name: Color32::from_rgb(250, 189, 47),     // yellow
                string: Color32::from_rgb(184, 187, 38),        // green
                number: Color32::from_rgb(215, 153, 33),        // orange
                boolean: Color32::from_rgb(215, 153, 33),       // orange
                comment: Color32::from_rgb(146, 131, 116),      // gray
                operator: Color32::from_rgb(254, 128, 25),      // bright orange
                punctuation: Color32::from_rgb(189, 174, 147),  // light gray
                variable: Color32::from_rgb(142, 192, 124),     // green
                property: Color32::from_rgb(131, 165, 152),     // teal
                parameter: Color32::from_rgb(211, 134, 155),    // pink
                function: Color32::from_rgb(254, 128, 25),      // bright orange
                default: Color32::from_rgb(235, 219, 178),      // light
            }
        } else {
            // Light theme
            Self {
                keyword: Color32::from_rgb(204, 36, 29),       // red
                type_name: Color32::from_rgb(181, 137, 0),     // yellow
                string: Color32::from_rgb(121, 133, 0),        // green
                number: Color32::from_rgb(181, 101, 0),        // orange
                boolean: Color32::from_rgb(181, 101, 0),       // orange
                comment: Color32::from_rgb(146, 131, 116),     // gray
                operator: Color32::from_rgb(181, 101, 0),      // orange
                punctuation: Color32::from_rgb(100, 100, 100), // dark gray
                variable: Color32::from_rgb(69, 133, 15),      // green
                property: Color32::from_rgb(69, 133, 136),     // teal
                parameter: Color32::from_rgb(177, 98, 134),    // pink
                function: Color32::from_rgb(181, 101, 0),      // orange
                default: Color32::from_rgb(60, 60, 60),        // dark
            }
        }
    }

    fn color_for_highlight(&self, name: &str) -> Color32 {
        match name {
            "keyword" => self.keyword,
            "type" | "type.builtin" => self.type_name,
            "string" => self.string,
            "number" => self.number,
            "boolean" => self.boolean,
            "comment" => self.comment,
            "operator" => self.operator,
            "punctuation" | "punctuation.bracket" => self.punctuation,
            "variable" => self.variable,
            "property" => self.property,
            "parameter" => self.parameter,
            "function" => self.function,
            _ => self.default,
        }
    }
}

/// Highlight source code using tree-sitter and return an egui LayoutJob.
///
/// Additional visual layers:
/// - `highlighted_line`: entire line gets a subtle blue background (timeline sync)
pub fn highlight_source(
    source: &str,
    style: &egui::Style,
    diagnostics: &[Diagnostic],
    highlighted_line: Option<usize>,
) -> LayoutJob {
    let colors = HighlightColors::from_style(style);
    let font_id = FontId::new(14.0, FontFamily::Monospace);

    let mut parser = Parser::new();
    parser.set_language(&*LANGUAGE).expect("Failed to set Animatix language");

    // Verify the source can be parsed
    if parser.parse(source, None).is_none() {
        return plain_text_job(source, &font_id, colors.default);
    }

    // Use tree-sitter highlight
    let mut highlighter = tree_sitter_highlight::Highlighter::new();

    let mut config = tree_sitter_highlight::HighlightConfiguration::new(
        tree_sitter_animatix::language(),
        "animatix",
        HIGHLIGHTS_QUERY,
        "",
        "",
    )
    .expect("Failed to create highlight configuration");

    config.configure(HIGHLIGHT_NAMES);

    let highlights = match highlighter.highlight(&config, source.as_bytes(), None, |_| None) {
        Ok(highlights) => highlights,
        Err(_) => {
            return plain_text_job(source, &font_id, colors.default);
        }
    };

    // Collect highlight spans as (start, end, color)
    let mut highlight_spans: Vec<(usize, usize, Color32)> = Vec::new();
    let mut current_highlight: Option<&str> = None;
    let mut last_end = 0;

    for event in highlights {
        match event {
            Ok(tree_sitter_highlight::HighlightEvent::Source { start, end }) => {
                let color = current_highlight
                    .map(|h| colors.color_for_highlight(h))
                    .unwrap_or(colors.default);
                highlight_spans.push((start, end, color));
                last_end = end;
            }
            Ok(tree_sitter_highlight::HighlightEvent::HighlightStart(highlight)) => {
                current_highlight = Some(HIGHLIGHT_NAMES[highlight.0]);
            }
            Ok(tree_sitter_highlight::HighlightEvent::HighlightEnd) => {
                current_highlight = None;
            }
            Err(_) => break,
        }
    }

    // Append any remaining text as a span
    if last_end < source.len() {
        highlight_spans.push((last_end, source.len(), colors.default));
    }

    // Build decoration ranges
    let mut deco_ranges: Vec<(usize, usize, Color32)> = Vec::new();

    // Highlighted line (timeline sync) — blue background
    if let Some(line) = highlighted_line {
        let (start, end) = line_byte_range(source, line);
        if start < end {
            deco_ranges.push((
                start,
                end,
                Color32::from_rgba_premultiplied(84, 110, 255, 45),
            ));
        }
    }

    let special_highlights: Vec<(usize, usize, Color32)> = Vec::new();

    // Apply all background layers
    let job = apply_background_layers(
        source,
        &font_id,
        &highlight_spans,
        diagnostics,
        &deco_ranges,
        &special_highlights,
    );

    job
}

/// Return the byte range of `line` (0-indexed) in `source`.
fn line_byte_range(source: &str, line: usize) -> (usize, usize) {
    let mut current_line = 0;
    let mut byte_offset = 0;

    for ch in source.chars() {
        if current_line == line {
            // Find end of this line
            let rest = &source[byte_offset..];
            let line_end = rest.find('\n').map(|i| byte_offset + i).unwrap_or(source.len());
            return (byte_offset, line_end);
        }
        byte_offset += ch.len_utf8();
        if ch == '\n' {
            current_line += 1;
        }
    }

    // Line is at or past end of source
    (source.len(), source.len())
}

/// Apply diagnostic and decoration background colors to highlight spans.
///
/// `special_highlights` override the text color for specific ranges (e.g.
/// amber text for keyframe timestamps).
fn apply_background_layers(
    source: &str,
    font_id: &FontId,
    highlight_spans: &[(usize, usize, Color32)],
    diagnostics: &[Diagnostic],
    deco_ranges: &[(usize, usize, Color32)],
    special_highlights: &[(usize, usize, Color32)],
) -> LayoutJob {
    // Convert diagnostics to byte offsets with background colors
    let diag_ranges: Vec<(usize, usize, Color32)> = diagnostics
        .iter()
        .filter_map(|d| {
            let start = line_col_to_byte(source, d.line, d.col);
            let end = line_col_to_byte(source, d.end_line, d.end_col);
            if start < end {
                let bg = match d.severity {
                    animatix_analyzer::DiagnosticSeverity::Error => {
                        Color32::from_rgba_premultiplied(255, 0, 0, 30)
                    }
                    animatix_analyzer::DiagnosticSeverity::Warning => {
                        Color32::from_rgba_premultiplied(255, 255, 0, 30)
                    }
                    animatix_analyzer::DiagnosticSeverity::Info => {
                        Color32::from_rgba_premultiplied(0, 100, 255, 30)
                    }
                    animatix_analyzer::DiagnosticSeverity::Hint => {
                        Color32::from_rgba_premultiplied(0, 200, 100, 30)
                    }
                };
                Some((start, end, bg))
            } else {
                None
            }
        })
        .collect();

    // Collect all boundary points
    let mut boundaries: Vec<usize> = Vec::new();
    boundaries.push(0);
    for &(start, end, _) in highlight_spans {
        boundaries.push(start);
        boundaries.push(end);
    }
    for &(start, end, _) in &diag_ranges {
        boundaries.push(start);
        boundaries.push(end);
    }
    for &(start, end, _) in deco_ranges {
        boundaries.push(start);
        boundaries.push(end);
    }
    for &(start, end, _) in special_highlights {
        boundaries.push(start);
        boundaries.push(end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut job = LayoutJob::default();

    // Process each segment between boundaries
    for window in boundaries.windows(2) {
        let seg_start = window[0];
        let seg_end = window[1];
        if seg_start >= seg_end {
            continue;
        }

        // Find the highlight color for this segment
        let mut highlight_color = Color32::from_rgb(235, 219, 178); // default light
        for &(h_start, h_end, h_color) in highlight_spans {
            if seg_start >= h_start && seg_end <= h_end {
                highlight_color = h_color;
                break;
            }
        }

        // Special highlights override syntax color (e.g. amber timestamp text)
        for &(sh_start, sh_end, sh_color) in special_highlights {
            if seg_start >= sh_start && seg_end <= sh_end {
                highlight_color = sh_color;
                break;
            }
        }

        // Check if segment is covered by a diagnostic
        let mut bg_color: Option<Color32> = None;
        for &(d_start, d_end, d_color) in &diag_ranges {
            if seg_start >= d_start && seg_end <= d_end {
                bg_color = Some(d_color);
                break;
            }
        }

        // Decoration layers override diagnostic backgrounds if present
        for &(deco_start, deco_end, deco_color) in deco_ranges {
            if seg_start >= deco_start && seg_end <= deco_end {
                bg_color = Some(deco_color);
                break;
            }
        }

        let text = &source[seg_start..seg_end];
        let background = bg_color.unwrap_or(Color32::TRANSPARENT);

        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color: highlight_color,
                background,
                ..Default::default()
            },
        );
    }

    job
}

/// Convert line and column (0-indexed) to a byte offset in the source.
fn line_col_to_byte(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 0;
    let mut current_col = 0;
    let mut byte_offset = 0;

    for ch in source.chars() {
        if current_line == line {
            if current_col >= col {
                return byte_offset;
            }
            current_col += 1;
        } else if ch == '\n' {
            current_line += 1;
            current_col = 0;
        }
        byte_offset += ch.len_utf8();
    }

    // If we reached end of source and the position is at or past the requested position
    source.len()
}

fn plain_text_job(text: &str, font_id: &FontId, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font_id.clone(),
            color,
            ..Default::default()
        },
    );
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_simple_input() {
        let source = r#"# 0s
title: Text {
    content: "Hello",
    position: (400, 300),
}"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None);

        // Should produce a non-empty layout job
        assert!(!job.text.is_empty());
        assert_eq!(job.text, source);
    }

    #[test]
    fn highlight_with_comments() {
        let source = r#"// This is a comment
title: Text {
    content: "Hello",
}"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None);

        assert!(!job.text.is_empty());
    }

    #[test]
    fn highlight_with_current_line() {
        let source = r#"line one
line two
line three
"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], Some(1));

        assert!(!job.text.is_empty());
    }
}
