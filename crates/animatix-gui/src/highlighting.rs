//! Tokenizer-based syntax highlighting for the Animatix DSL.

use std::collections::HashMap;

use animatix_syntax::highlight::lexical_role;
use animatix_syntax::token::LineIndex;
use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};

use crate::app::design_tokens::typography::TextRole;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HighlightSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HighlightDiagnostic {
    pub start: usize,
    pub end: usize,
    pub severity: HighlightSeverity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FindHighlight {
    pub start: usize,
    pub end: usize,
    pub current: bool,
}

#[derive(Clone)]
pub(crate) struct HighlightCache {
    pub source: String,
    pub dark_mode: bool,
    pub font_id: FontId,
    pub highlighted_line: Option<usize>,
    pub diagnostics: Vec<HighlightDiagnostic>,
    pub find_highlights: Vec<FindHighlight>,
    pub job: LayoutJob,
    pub line_count: usize,
    pub max_line_chars: usize,
}

impl HighlightCache {
    pub fn new(
        source: String,
        dark_mode: bool,
        font_id: FontId,
        highlighted_line: Option<usize>,
        diagnostics: Vec<HighlightDiagnostic>,
        find_highlights: Vec<FindHighlight>,
        job: LayoutJob,
    ) -> Self {
        let line_count = source.split('\n').count().max(1);
        let max_line_chars = source.split('\n').map(|line| line.chars().count()).max().unwrap_or(0);
        Self {
            source,
            dark_mode,
            font_id,
            highlighted_line,
            diagnostics,
            find_highlights,
            job,
            line_count,
            max_line_chars,
        }
    }
}

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
        if style.visuals.dark_mode {
            Self {
                keyword: Color32::from_rgb(251, 73, 106),
                type_name: Color32::from_rgb(250, 189, 47),
                string: Color32::from_rgb(184, 187, 38),
                number: Color32::from_rgb(215, 153, 33),
                boolean: Color32::from_rgb(215, 153, 33),
                comment: Color32::from_rgb(146, 131, 116),
                operator: Color32::from_rgb(168, 153, 132),
                punctuation: Color32::from_rgb(189, 174, 147),
                variable: Color32::from_rgb(177, 98, 134),
                property: Color32::from_rgb(131, 165, 152),
                parameter: Color32::from_rgb(211, 134, 155),
                function: Color32::from_rgb(254, 128, 25),
                label: Color32::from_rgb(102, 153, 204),
                default: Color32::from_rgb(235, 219, 178),
            }
        } else {
            Self {
                keyword: Color32::from_rgb(204, 36, 29),
                type_name: Color32::from_rgb(181, 137, 0),
                string: Color32::from_rgb(121, 133, 0),
                number: Color32::from_rgb(181, 101, 0),
                boolean: Color32::from_rgb(181, 101, 0),
                comment: Color32::from_rgb(146, 131, 116),
                operator: Color32::from_rgb(140, 140, 140),
                punctuation: Color32::from_rgb(100, 100, 100),
                variable: Color32::from_rgb(127, 0, 85),
                property: Color32::from_rgb(69, 133, 136),
                parameter: Color32::from_rgb(177, 98, 134),
                function: Color32::from_rgb(181, 101, 0),
                label: Color32::from_rgb(51, 102, 153),
                default: Color32::from_rgb(60, 60, 60),
            }
        }
    }

    fn color_for_role(&self, role: &str) -> Color32 {
        match role {
            "keyword" | "wildcard" => self.keyword,
            "type" | "type.builtin" => self.type_name,
            "string" => self.string,
            "number" => self.number,
            "boolean" => self.boolean,
            "comment" => self.comment,
            "operator" => self.operator,
            "punctuation" | "punctuation.bracket" => self.punctuation,
            "variable" | "importalias" => self.variable,
            "property" => self.property,
            "parameter" => self.parameter,
            "function" | "action" => self.function,
            "label" => self.label,
            _ => self.default,
        }
    }
}

/// Build the source editor's base syntax and decoration layout.
pub(crate) fn highlight_source(
    source: &str,
    style: &egui::Style,
    diagnostics: &[HighlightDiagnostic],
    highlighted_line: Option<usize>,
    find_highlights: &[FindHighlight],
) -> LayoutJob {
    let colors = HighlightColors::from_style(style);
    let font_id = TextRole::Mono.font_id();
    let line_index = LineIndex::new(source);
    // This parse returns the tokens and semantic occurrences together; avoid a
    // second lexer pass just to recover token spans.
    let parsed = animatix_syntax::parser::parse_source_complete(source);
    let occurrences: HashMap<usize, &animatix_syntax::occurrence::Occurrence> = parsed
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.span.start, occurrence))
        .collect();

    let mut text_spans = Vec::with_capacity(parsed.tokens.len() * 2 + 1);
    let mut last_end = 0;
    for token in &parsed.tokens {
        if token.span.start > last_end {
            text_spans.push((last_end, token.span.start, colors.default));
        }
        let role = match &token.kind {
            animatix_syntax::token::TokenKind::Ident(_) => occurrences
                .get(&token.span.start)
                .map(|occurrence| occurrence.kind.token_role())
                .unwrap_or("variable"),
            _ => lexical_role(token),
        };
        text_spans.push((token.span.start, token.span.end, colors.color_for_role(role)));
        last_end = token.span.end;
    }
    if last_end < source.len() {
        text_spans.push((last_end, source.len(), colors.default));
    }

    let current_line = highlighted_line
        .and_then(|line| line_byte_range(source, &line_index, line))
        .map(|(start, end)| (start, end, Color32::from_rgba_premultiplied(84, 110, 255, 45)));
    let mut job =
        apply_layers(source, &font_id, &text_spans, diagnostics, current_line, find_highlights);
    let row_height = font_id.size;
    for section in &mut job.sections {
        section.format.line_height = Some(row_height);
    }
    job
}

fn line_byte_range(source: &str, index: &LineIndex<'_>, line: usize) -> Option<(usize, usize)> {
    let start = index.line_col_to_byte(line, 0);
    if start >= source.len() {
        return None;
    }
    let end = source[start..]
        .find('\n')
        .map(|newline| start + newline)
        .unwrap_or(source.len());
    Some((start, end))
}

fn apply_layers(
    source: &str,
    font_id: &FontId,
    text_spans: &[(usize, usize, Color32)],
    diagnostics: &[HighlightDiagnostic],
    current_line: Option<(usize, usize, Color32)>,
    find_highlights: &[FindHighlight],
) -> LayoutJob {
    let mut boundaries = vec![0, source.len()];
    for &(start, end, _) in text_spans {
        boundaries.extend([start, end]);
    }
    for diagnostic in diagnostics {
        if let Some((start, end)) = clamp_range(source, diagnostic.start, diagnostic.end) {
            boundaries.extend([start, end]);
        }
    }
    if let Some((start, end, _)) = current_line {
        boundaries.extend([start, end]);
    }
    for matched in find_highlights {
        if let Some((start, end)) = clamp_range(source, matched.start, matched.end) {
            boundaries.extend([start, end]);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut job = LayoutJob::default();
    let mut span_index = 0;
    let mut match_index = 0;
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
            continue;
        }
        // Tokens and regex matches are ordered, disjoint source ranges. Walk
        // them once instead of rescanning the whole document for every span.
        while span_index < text_spans.len() && text_spans[span_index].1 <= start {
            span_index += 1;
        }
        let color = text_spans
            .get(span_index)
            .filter(|(span_start, span_end, _)| start >= *span_start && end <= *span_end)
            .map(|(_, _, color)| *color)
            .unwrap_or(Color32::from_rgb(235, 219, 178));
        let mut background = diagnostics
            .iter()
            .find(|diagnostic| start >= diagnostic.start && end <= diagnostic.end)
            .map(|diagnostic| diagnostic_background(diagnostic.severity));
        if let Some((line_start, line_end, line_color)) = current_line
            && start >= line_start
            && end <= line_end
        {
            background = Some(line_color);
        }
        while match_index < find_highlights.len() && find_highlights[match_index].end <= start {
            match_index += 1;
        }
        if let Some(matched) = find_highlights.get(match_index)
            && start >= matched.start
            && end <= matched.end
        {
            background = Some(if matched.current {
                Color32::from_rgba_premultiplied(80, 140, 255, 100)
            } else {
                Color32::from_rgba_premultiplied(220, 180, 40, 80)
            });
        }
        job.append(
            &source[start..end],
            0.0,
            TextFormat {
                font_id: font_id.clone(),
                color,
                background: background.unwrap_or(Color32::TRANSPARENT),
                ..Default::default()
            },
        );
    }
    job
}

fn clamp_range(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let mut start = start.min(source.len());
    let mut end = end.min(source.len());
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    while end > start && !source.is_char_boundary(end) {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

fn diagnostic_background(severity: HighlightSeverity) -> Color32 {
    match severity {
        HighlightSeverity::Error => Color32::from_rgba_premultiplied(255, 60, 60, 55),
        HighlightSeverity::Warning => Color32::from_rgba_premultiplied(255, 200, 80, 75),
        HighlightSeverity::Info => Color32::from_rgba_premultiplied(80, 140, 255, 40),
        HighlightSeverity::Hint => Color32::from_rgba_premultiplied(80, 220, 140, 40),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section_for_text<'a>(
        job: &'a LayoutJob,
        needle: &str,
    ) -> Option<&'a egui::text::LayoutSection> {
        job.sections.iter().find(|section| {
            &job.text[section.byte_range.start.0..section.byte_range.end.0] == needle
        })
    }

    fn text_color(job: &LayoutJob, needle: &str) -> Option<Color32> {
        section_for_text(job, needle).map(|section| section.format.color)
    }

    fn background_color(job: &LayoutJob, needle: &str) -> Option<Color32> {
        section_for_text(job, needle).map(|section| section.format.background)
    }

    #[test]
    fn highlight_simple_input() {
        let source = r#"# 0s
title: Text {
    content: "Hello",
    position: (400, 300),
}"#;
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(job.text, source);
    }

    #[test]
    fn highlight_with_comments() {
        let source = "// This is a comment\ntitle: Text { content: \"Hello\" }";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(job.text, source);
        assert_ne!(text_color(&job, "// This is a comment"), text_color(&job, "title"));
    }

    #[test]
    fn highlight_with_current_line() {
        let source = "line one\nline two\nline three\n";
        let job = highlight_source(source, &egui::Style::default(), &[], Some(1), &[]);
        assert_eq!(job.text, source);
        assert_eq!(
            background_color(&job, "two"),
            Some(Color32::from_rgba_premultiplied(84, 110, 255, 45))
        );
        assert_eq!(background_color(&job, "one"), Some(Color32::TRANSPARENT));
    }

    #[test]
    fn highlight_produces_actual_colors() {
        let source = "// comment\nlet x = 42\ntitle: Text, text: \"hello\"\n#1s\nfade_in(target: title, duration: 1s)\n";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(job.text, source);
        let default_dark = Color32::from_rgb(235, 219, 178);
        assert!(job.sections.iter().any(|section| section.format.color != default_dark));
    }

    #[test]
    fn highlight_source_sections_have_varied_colors() {
        let source =
            "let x = 42\ntitle: Text, text: \"hello\"\n#1s\nfade_in(target: title, duration: 1s)\n";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        let colors: std::collections::HashSet<_> =
            job.sections.iter().map(|section| section.format.color).collect();
        assert!(colors.len() > 1, "expected token roles to use varied colors: {colors:?}");
    }

    #[test]
    fn actor_label_and_type_have_distinct_colors() {
        let source = "backdrop: Rect, size: (1280, 720)";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_ne!(text_color(&job, "backdrop"), text_color(&job, "Rect"));
        assert_eq!(text_color(&job, "backdrop"), Some(Color32::from_rgb(102, 153, 204)));
    }

    #[test]
    fn path_expression_base_and_name_have_distinct_colors() {
        let source = "orb.size = (120, 120)";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_ne!(text_color(&job, "orb"), text_color(&job, "size"));
    }

    #[test]
    fn format_interpolation_keeps_function_string_and_variable_roles() {
        let source = "let x = format(\"y = {:.2}\", value)";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_ne!(text_color(&job, "format"), text_color(&job, "\"y = {:.2}\""));
        assert_ne!(text_color(&job, "value"), text_color(&job, "\"y = {:.2}\""));
    }

    #[test]
    fn actor_label_is_blue_from_parser_occurrence() {
        // Parser occurrences preserve label coloring now that the separate
        // notebook-cell semantic overlay has been removed.
        let source = "backdrop: Rect";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(text_color(&job, "backdrop"), Some(Color32::from_rgb(102, 153, 204)));
        assert_ne!(text_color(&job, "backdrop"), text_color(&job, "Rect"));
    }

    #[test]
    fn indexed_callable_target_matches_named_target_label_color() {
        let source = "fade_in(targets: {card[0], named}, duration: 1s)";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(text_color(&job, "card"), text_color(&job, "named"));
    }

    #[test]
    fn indexed_assignment_target_base_and_property_have_distinct_colors() {
        let source = "card[0].scale = 1.0";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(text_color(&job, "card"), Some(Color32::from_rgb(102, 153, 204)));
        assert_ne!(text_color(&job, "card"), text_color(&job, "scale"));
    }

    #[test]
    fn index_expression_outside_target_list_keeps_non_label_color() {
        let source = "let x = items[0]";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_ne!(text_color(&job, "items"), Some(Color32::from_rgb(102, 153, 204)));
    }

    #[test]
    fn unicode_and_crlf_source_ranges_remain_utf8_byte_aligned() {
        let source = "let face = \"😀\"; backdrop: Rect\r\n#1s\r\n";
        let job = highlight_source(source, &egui::Style::default(), &[], None, &[]);
        assert_eq!(job.text, source, "highlighting must preserve CRLF and astral text bytes");

        let backdrop_byte = source.find("backdrop").expect("actor label");
        let backdrop = section_for_text(&job, "backdrop").expect("label section");
        assert_eq!(backdrop.byte_range.start.0, backdrop_byte);
        for section in &job.sections {
            assert!(source.is_char_boundary(section.byte_range.start.0));
            assert!(source.is_char_boundary(section.byte_range.end.0));
        }
    }

    #[test]
    fn diagnostic_line_and_find_backgrounds_keep_their_layer_priority() {
        let source = "error\nline\nhit\n";
        let error_start = source.find("error").expect("diagnostic text");
        let hit_start = source.find("hit").expect("find match");
        let diagnostics = [HighlightDiagnostic {
            start: error_start,
            end: error_start + "error".len(),
            severity: HighlightSeverity::Error,
        }];
        let finds = [FindHighlight {
            start: hit_start,
            end: hit_start + "hit".len(),
            current: true,
        }];
        let job = highlight_source(source, &egui::Style::default(), &diagnostics, Some(1), &finds);

        assert_eq!(
            background_color(&job, "error"),
            Some(Color32::from_rgba_premultiplied(255, 60, 60, 55))
        );
        assert_eq!(
            background_color(&job, "line"),
            Some(Color32::from_rgba_premultiplied(84, 110, 255, 45))
        );
        assert_eq!(
            background_color(&job, "hit"),
            Some(Color32::from_rgba_premultiplied(80, 140, 255, 100))
        );

        let overlaps = [HighlightDiagnostic {
            start: hit_start,
            end: hit_start + "hit".len(),
            severity: HighlightSeverity::Error,
        }];
        let overlapping_job =
            highlight_source(source, &egui::Style::default(), &overlaps, Some(2), &finds);
        assert_eq!(
            background_color(&overlapping_job, "hit"),
            Some(Color32::from_rgba_premultiplied(80, 140, 255, 100)),
            "current find highlight should sit above the line and diagnostic backgrounds"
        );
    }
}
