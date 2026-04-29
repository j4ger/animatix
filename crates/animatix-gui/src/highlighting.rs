//! Tree-sitter based syntax highlighting for the Animatix DSL.

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
pub fn highlight_source(source: &str, style: &egui::Style) -> LayoutJob {
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

    let mut job = LayoutJob::default();
    let mut current_highlight: Option<&str> = None;
    let mut last_end = 0;

    for event in highlights {
        match event {
            Ok(tree_sitter_highlight::HighlightEvent::Source { start, end }) => {
                let text = &source[start..end];
                let color = current_highlight
                    .map(|h| colors.color_for_highlight(h))
                    .unwrap_or(colors.default);

                job.append(
                    text,
                    0.0,
                    TextFormat {
                        font_id: font_id.clone(),
                        color,
                        ..Default::default()
                    },
                );
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

    // Append any remaining text
    if last_end < source.len() {
        job.append(
            &source[last_end..],
            0.0,
            TextFormat {
                font_id,
                color: colors.default,
                ..Default::default()
            },
        );
    }

    job
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
        let job = highlight_source(source, &style);

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
        let job = highlight_source(source, &style);

        assert!(!job.text.is_empty());
    }
}
