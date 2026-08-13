//! Tree-sitter based syntax highlighting for the Animatix DSL.

use std::sync::LazyLock;

use animatix_analyzer::Diagnostic;
use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};
use tracing::warn;
use tree_sitter::{Language, Parser};
use tree_sitter_animatix::{HIGHLIGHTS_QUERY, language};

use crate::app::design_tokens::typography::TextRole;
use crate::cell_editor::SemanticHighlight;

static LANGUAGE: LazyLock<Language> = LazyLock::new(language);

static HIGHLIGHT_CONFIG: LazyLock<Option<tree_sitter_highlight::HighlightConfiguration>> =
    LazyLock::new(|| {
        let mut config = tree_sitter_highlight::HighlightConfiguration::new(
            tree_sitter_animatix::language(),
            "animatix",
            HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .ok()?;
        config.configure(HIGHLIGHT_NAMES);
        Some(config)
    });

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
    "label",
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
    label: Color32,
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
                type_name: Color32::from_rgb(250, 189, 47),    // yellow
                string: Color32::from_rgb(184, 187, 38),       // green
                number: Color32::from_rgb(215, 153, 33),       // orange
                boolean: Color32::from_rgb(215, 153, 33),      // orange
                comment: Color32::from_rgb(146, 131, 116),     // gray
                operator: Color32::from_rgb(168, 153, 132),    // muted gray
                punctuation: Color32::from_rgb(189, 174, 147), // light gray
                variable: Color32::from_rgb(177, 98, 134),     // purple
                property: Color32::from_rgb(131, 165, 152),    // teal
                parameter: Color32::from_rgb(211, 134, 155),   // pink
                function: Color32::from_rgb(254, 128, 25),     // bright orange
                label: Color32::from_rgb(102, 153, 204),       // soft blue
                default: Color32::from_rgb(235, 219, 178),     // light
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
                operator: Color32::from_rgb(140, 140, 140),    // gray
                punctuation: Color32::from_rgb(100, 100, 100), // dark gray
                variable: Color32::from_rgb(127, 0, 85),       // purple
                property: Color32::from_rgb(69, 133, 136),     // teal
                parameter: Color32::from_rgb(177, 98, 134),    // pink
                function: Color32::from_rgb(181, 101, 0),      // orange
                label: Color32::from_rgb(51, 102, 153),        // dark blue
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
            "label" => self.label,
            _ => self.default,
        }
    }
}

/// Highlight source code using tree-sitter and return an egui LayoutJob.
///
/// Additional visual layers:
/// - `highlighted_line`: entire line gets a subtle blue background (timeline sync)
/// - `semantic_highlights`: actor names, scene names, component names get distinct colors
pub fn highlight_source(
    source: &str,
    style: &egui::Style,
    diagnostics: &[Diagnostic],
    highlighted_line: Option<usize>,
    semantic_highlights: &[SemanticHighlight],
) -> LayoutJob {
    let colors = HighlightColors::from_style(style);
    let font_id = TextRole::Mono.font_id();

    let mut parser = Parser::new();
    if parser.set_language(&LANGUAGE).is_err() {
        warn!("highlight_source: failed to set language, falling back to plain text");
        return plain_text_job(source, &font_id, colors.default);
    }

    // Verify the source can be parsed
    if parser.parse(source, None).is_none() {
        warn!("highlight_source: parse returned None, falling back to plain text");
        return plain_text_job(source, &font_id, colors.default);
    }

    // Use tree-sitter highlight
    let mut highlighter = tree_sitter_highlight::Highlighter::new();

    let config = match HIGHLIGHT_CONFIG.as_ref() {
        Some(c) => c,
        None => {
            warn!(
                "highlight_source: HIGHLIGHT_CONFIG initialization failed, falling back to plain text"
            );
            return plain_text_job(source, &font_id, colors.default);
        },
    };

    let highlights = match highlighter.highlight(config, source.as_bytes(), None, |_| None) {
        Ok(highlights) => highlights,
        Err(e) => {
            warn!(
                "highlight_source: highlighter.highlight failed: {:?}, falling back to plain text",
                e
            );
            return plain_text_job(source, &font_id, colors.default);
        },
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
            },
            Ok(tree_sitter_highlight::HighlightEvent::HighlightStart(highlight)) => {
                current_highlight = Some(HIGHLIGHT_NAMES[highlight.0]);
            },
            Ok(tree_sitter_highlight::HighlightEvent::HighlightEnd) => {
                current_highlight = None;
            },
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
            deco_ranges.push((start, end, Color32::from_rgba_premultiplied(84, 110, 255, 45)));
        }
    }

    // Convert semantic highlights to special highlight ranges with colors
    let mut special_highlights: Vec<(usize, usize, Color32)> = Vec::new();
    for sh in semantic_highlights {
        let start = line_col_to_byte(source, sh.rel_line, sh.rel_col);
        let end = line_col_to_byte(source, sh.rel_end_line, sh.rel_end_col);
        if start < end {
            let color = match sh.kind {
                crate::cell_editor::SemanticTokenKind::ActorName => {
                    Color32::from_rgb(102, 153, 204) // blue, matching @label
                },
                crate::cell_editor::SemanticTokenKind::ComponentName => {
                    Color32::from_rgb(211, 134, 155) // pink
                },
                crate::cell_editor::SemanticTokenKind::SceneName => {
                    Color32::from_rgb(131, 165, 152) // teal
                },
                crate::cell_editor::SemanticTokenKind::PropertyName => {
                    Color32::from_rgb(142, 192, 124) // green
                },
            };
            special_highlights.push((start, end, color));
        }
    }

    // Apply all background layers
    apply_background_layers(
        source,
        &font_id,
        &highlight_spans,
        diagnostics,
        &deco_ranges,
        &special_highlights,
    )
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
    // Diagnostic byte ranges for boundary splitting.
    let diag_ranges: Vec<(usize, usize)> = diagnostics
        .iter()
        .filter_map(|d| {
            let start = line_col_to_byte(source, d.line, d.col);
            let end = line_col_to_byte(source, d.end_line, d.end_col);
            if start < end {
                Some((start, end))
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
    for &(start, end) in &diag_ranges {
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
        for d in diagnostics {
            let d_start = line_col_to_byte(source, d.line, d.col);
            let d_end = line_col_to_byte(source, d.end_line, d.end_col);
            if seg_start >= d_start && seg_end <= d_end {
                bg_color = Some(match d.severity {
                    animatix_analyzer::DiagnosticSeverity::Error => {
                        Color32::from_rgba_premultiplied(255, 60, 60, 55)
                    },
                    animatix_analyzer::DiagnosticSeverity::Warning => {
                        Color32::from_rgba_premultiplied(255, 200, 80, 75)
                    },
                    animatix_analyzer::DiagnosticSeverity::Info => {
                        Color32::from_rgba_premultiplied(80, 140, 255, 40)
                    },
                    animatix_analyzer::DiagnosticSeverity::Hint => {
                        Color32::from_rgba_premultiplied(80, 220, 140, 40)
                    },
                });
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
        let job = highlight_source(source, &style, &[], None, &[]);

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
        let job = highlight_source(source, &style, &[], None, &[]);

        assert!(!job.text.is_empty());
    }

    #[test]
    fn highlight_with_current_line() {
        let source = r#"line one
line two
line three
"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], Some(1), &[]);

        assert!(!job.text.is_empty());
    }

    #[test]
    fn highlight_produces_actual_colors() {
        // This test verifies the highlight pipeline doesn't silently fall back
        // to plain text. We check that at least some non-default colors are used.
        let source = r#"// comment
let x = 42
title: Text, text: "hello"
#1s
fade-in title [1s]
"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        assert!(!job.text.is_empty());
        assert_eq!(job.text, source);

        // Check that we have at least one non-default color in the layout job.
        // The default color in dark mode is Color32::from_rgb(235, 219, 178).
        let default_light = Color32::from_rgb(235, 219, 178);
        let has_highlight = job.sections.iter().any(|s| s.format.color != default_light);
        assert!(
            has_highlight,
            "Expected some highlighted sections, but all sections used the default color. \
             This usually means the highlight pipeline fell back to plain_text_job."
        );
    }

    #[test]
    fn highlight_configuration_loads() {
        // Verify the tree-sitter highlight configuration can be created from the query.
        let mut config = tree_sitter_highlight::HighlightConfiguration::new(
            tree_sitter_animatix::language(),
            "animatix",
            tree_sitter_animatix::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .expect("HighlightConfiguration::new should succeed with the bundled query");

        config.configure(HIGHLIGHT_NAMES);
    }

    #[test]
    fn highlight_events_are_produced() {
        // Verify that tree-sitter-highlight actually produces highlight events.
        let source = r#"let x = 42
fade-in title [1s]
"#;

        let mut highlighter = tree_sitter_highlight::Highlighter::new();
        let mut config = tree_sitter_highlight::HighlightConfiguration::new(
            tree_sitter_animatix::language(),
            "animatix",
            tree_sitter_animatix::HIGHLIGHTS_QUERY,
            "",
            "",
        )
        .unwrap();
        config.configure(HIGHLIGHT_NAMES);

        let highlights = highlighter.highlight(&config, source.as_bytes(), None, |_| None).unwrap();

        let mut source_events = 0;
        let mut highlight_starts = 0;
        let mut highlight_ends = 0;

        for event in highlights {
            match event {
                Ok(tree_sitter_highlight::HighlightEvent::Source { .. }) => source_events += 1,
                Ok(tree_sitter_highlight::HighlightEvent::HighlightStart(_)) => {
                    highlight_starts += 1
                },
                Ok(tree_sitter_highlight::HighlightEvent::HighlightEnd) => highlight_ends += 1,
                Err(e) => panic!("Highlight error: {:?}", e),
            }
        }

        assert!(highlight_starts > 0, "Expected at least some HighlightStart events, got 0");
        assert_eq!(
            highlight_starts, highlight_ends,
            "Mismatched HighlightStart/HighlightEnd count"
        );
        assert!(source_events > 0, "Expected at least some Source events");
    }

    #[test]
    fn highlight_source_sections_have_varied_colors() {
        // This test verifies that the LayoutJob produced by highlight_source
        // contains sections with multiple distinct colors.
        let source = r#"let x = 42
title: Text, text: "hello"
#1s
fade-in title [1s]
"#;

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let mut color_counts: std::collections::HashMap<Color32, usize> =
            std::collections::HashMap::new();
        for section in &job.sections {
            *color_counts.entry(section.format.color).or_insert(0) += 1;
        }

        assert!(
            color_counts.len() > 1,
            "Expected multiple distinct colors in LayoutJob sections, got: {:?}",
            color_counts.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn actor_label_and_type_have_distinct_colors() {
        // Verify actor labels and types are colored differently.
        // Regression test: both were accidentally yellow when the query
        // or color mapping was misconfigured.
        let source = "backdrop: Rect, size: (1280, 720)";

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let backdrop_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "backdrop")
            .map(|s| s.format.color);
        let rect_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "Rect")
            .map(|s| s.format.color);

        assert!(
            backdrop_color != rect_color,
            "actor label 'backdrop' and type 'Rect' should have different colors"
        );
    }

    #[test]
    fn path_expression_base_and_name_have_distinct_colors() {
        // Verify property access paths have different colors for base and property.
        // Regression test: "orb.size" was all the same color.
        let source = "orb.size = (120, 120)";

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let orb_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "orb")
            .map(|s| s.format.color);
        let size_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "size")
            .map(|s| s.format.color);

        assert!(
            orb_color != size_color,
            "path base 'orb' and property 'size' should have different colors"
        );
    }

    #[test]
    fn actor_label_is_blue_with_semantic_highlight() {
        // Verify actor labels stay blue even when semantic highlights are applied.
        // Regression test: ActorName semantic highlight (amber) was overriding
        // the tree-sitter @label (blue), making labels look the same as types.
        let source = "backdrop: Rect";

        let style = egui::Style::default();
        let semantic = &[crate::cell_editor::SemanticHighlight {
            cell_index: 0,
            rel_line: 0,
            rel_col: 0,
            rel_end_line: 0,
            rel_end_col: 8,
            kind: crate::cell_editor::SemanticTokenKind::ActorName,
        }];
        let job = highlight_source(source, &style, &[], None, semantic);

        let backdrop_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "backdrop")
            .map(|s| s.format.color);
        let rect_color = job
            .sections
            .iter()
            .find(|s| &job.text[s.byte_range.clone()] == "Rect")
            .map(|s| s.format.color);

        assert!(
            backdrop_color != rect_color,
            "actor label 'backdrop' should remain a different color from type 'Rect' \
             even with semantic highlights applied"
        );

        // Also verify the label color matches the expected blue
        let expected_blue = Color32::from_rgb(102, 153, 204);
        assert_eq!(
            backdrop_color,
            Some(expected_blue),
            "actor label 'backdrop' should be blue (#6699CC)"
        );
    }

    fn text_color(job: &egui::text::LayoutJob, needle: &str) -> Option<Color32> {
        job.sections
            .iter()
            .find(|section| &job.text[section.byte_range.clone()] == needle)
            .map(|section| section.format.color)
    }

    #[test]
    fn indexed_action_target_matches_named_target_label_color() {
        let source = "fade-in card[0], named [1s]";

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let card_color = text_color(&job, "card");
        let named_color = text_color(&job, "named");
        assert_eq!(
            card_color, named_color,
            "indexed target base 'card' should use the same color as named target 'named'"
        );
    }

    #[test]
    fn indexed_assignment_target_base_and_property_have_distinct_colors() {
        let source = "card[0].scale = 1.0";

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let expected_label = Color32::from_rgb(102, 153, 204);
        let expected_property = Color32::from_rgb(131, 165, 152);
        assert_eq!(
            text_color(&job, "card"),
            Some(expected_label),
            "indexed target base 'card' should be highlighted as a label"
        );
        assert_eq!(
            text_color(&job, "scale"),
            Some(expected_property),
            "indexed target property 'scale' should be highlighted as a property"
        );
    }

    #[test]
    fn index_expression_outside_target_list_keeps_non_label_color() {
        let source = "let x = items[0]";

        let style = egui::Style::default();
        let job = highlight_source(source, &style, &[], None, &[]);

        let expected_label = Color32::from_rgb(102, 153, 204);
        assert_ne!(
            text_color(&job, "items"),
            Some(expected_label),
            "indexed expression base 'items' should not become an actor label outside a target list"
        );
    }
}
