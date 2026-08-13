//! Read-only syntax-highlighted source viewer for review runs.
//!
//! Kept deliberately small: a line-number gutter plus one highlighted layout
//! per source line. The review demo does not need the full cell editor, but it
//! does need line-anchored comments, so this viewer owns line hit-testing.

use animatix_analyzer::Diagnostic as AnalyzerDiagnostic;
use animatix_analyzer::DiagnosticSeverity as AnalyzerSeverity;
use animatix_syntax::diagnostics::{
    Diagnostic as SyntaxDiagnostic, DiagnosticSeverity as SyntaxSeverity,
};
use egui::text::LayoutJob;
use egui::{RichText, TextStyle};

use crate::highlighting::highlight_source;

#[derive(Clone)]
struct HighlightedLine {
    job: LayoutJob,
}

/// Renders a source file as selectable lines with line numbers.
#[derive(Default)]
pub(crate) struct SourceViewer {
    highlighted_lines: Vec<HighlightedLine>,
    cached_source: Option<String>,
    cached_selected_line: Option<usize>,
}

impl SourceViewer {
    /// Render the source. Returns the 0-based line clicked this frame.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        source: &str,
        diagnostics: &[SyntaxDiagnostic],
        selected_line: Option<usize>,
        scroll_to_line: bool,
    ) -> Option<usize> {
        let line_count = source.lines().count().max(1);
        let digit_width = line_count.to_string().len();
        let style = ui.style().clone();
        let row_height = ui.text_style_height(&TextStyle::Monospace).max(18.0);

        if self.cached_source.as_deref() != Some(source)
            || self.cached_selected_line != selected_line
        {
            self.highlighted_lines.clear();
            self.cached_source = Some(source.to_owned());
            self.cached_selected_line = selected_line;
        } else if self.highlighted_lines.len() != line_count {
            self.highlighted_lines.clear();
        }

        let mut clicked = None;
        egui::ScrollArea::both().id_salt("review_source_viewer").show_rows(
            ui,
            row_height,
            line_count,
            |ui, range| {
                if scroll_to_line {
                    if let Some(line) = selected_line {
                        ui.scroll_to_rect(
                            egui::Rect::from_min_size(
                                ui.min_rect().min + egui::vec2(0.0, line as f32 * row_height),
                                egui::vec2(1.0, row_height),
                            ),
                            Some(egui::Align::Center),
                        );
                    }
                }

                for line_idx in range {
                    if self.highlighted_lines.len() <= line_idx {
                        self.highlighted_lines.push(make_highlighted_line(
                            source,
                            &style,
                            diagnostics,
                            line_idx,
                            selected_line,
                        ));
                    }
                    let row = &self.highlighted_lines[line_idx];

                    ui.horizontal(|ui| {
                        let line_number = format!("{:width$}", line_idx + 1, width = digit_width);
                        let number_color = if selected_line == Some(line_idx) {
                            ui.visuals().selection.stroke.color
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        let button = egui::Button::new(
                            RichText::new(line_number).monospace().color(number_color),
                        )
                        .frame(false)
                        .min_size(egui::vec2(18.0 + digit_width as f32 * 7.0, row_height))
                        .selected(selected_line == Some(line_idx));
                        if ui.add(button).clicked() {
                            clicked = Some(line_idx);
                        }

                        let label = egui::Label::new(row.job.clone())
                            .selectable(false)
                            .wrap_mode(egui::TextWrapMode::Extend);
                        if ui.add(label).clicked() {
                            clicked = Some(line_idx);
                        }
                    });
                }
            },
        );

        clicked
    }
}

fn make_highlighted_line(
    source: &str,
    style: &egui::Style,
    diagnostics: &[SyntaxDiagnostic],
    line_idx: usize,
    selected_line: Option<usize>,
) -> HighlightedLine {
    let (start, end) = line_byte_range(source, line_idx);
    let line_text = if start < end { &source[start..end] } else { "" };
    let line_diags = line_diagnostics(diagnostics, line_idx);
    let job = highlight_source(
        line_text,
        style,
        &line_diags,
        if selected_line == Some(line_idx) {
            Some(0)
        } else {
            None
        },
        &[],
    );
    HighlightedLine { job }
}

fn line_diagnostics(diagnostics: &[SyntaxDiagnostic], line_idx: usize) -> Vec<AnalyzerDiagnostic> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let location = &diagnostic.location;
            let line = location.line.unwrap_or(1).saturating_sub(1);
            let col = location.column.unwrap_or(1).saturating_sub(1);
            let end_line =
                location.end_line.unwrap_or(location.line.unwrap_or(1)).saturating_sub(1);
            let end_col =
                location.end_col.unwrap_or(location.column.unwrap_or(1)).saturating_sub(1);
            if line != line_idx && end_line != line_idx {
                return None;
            }
            Some(AnalyzerDiagnostic {
                severity: match diagnostic.severity {
                    SyntaxSeverity::Error => AnalyzerSeverity::Error,
                    SyntaxSeverity::Warning => AnalyzerSeverity::Warning,
                    SyntaxSeverity::Info => AnalyzerSeverity::Info,
                    SyntaxSeverity::Hint => AnalyzerSeverity::Hint,
                },
                line,
                col,
                end_line,
                end_col,
                message: diagnostic.message.clone(),
                code: None,
            })
        })
        .collect()
}

fn line_byte_range(source: &str, line: usize) -> (usize, usize) {
    let mut current_line = 0;
    let mut byte_offset = 0;

    for ch in source.chars() {
        if current_line == line {
            let rest = &source[byte_offset..];
            let line_end = rest.find('\n').map(|i| byte_offset + i).unwrap_or(source.len());
            return (byte_offset, line_end);
        }
        byte_offset += ch.len_utf8();
        if ch == '\n' {
            current_line += 1;
        }
    }

    (source.len(), source.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_byte_range_spans_source_lines() {
        let source = "one\ntwo\nthree";
        assert_eq!(line_byte_range(source, 0), (0, 3));
        assert_eq!(line_byte_range(source, 1), (4, 7));
        assert_eq!(line_byte_range(source, 2), (8, 13));
        assert_eq!(line_byte_range(source, 3), (13, 13));
    }
}
