use egui::{Color32, Frame, Margin, Pos2, RichText, ScrollArea, Stroke, Vec2};

use crate::app::components::{anim, text_tooltip};
use crate::app::design_tokens::motion;
use crate::app::design_tokens::spatial::{
    RADIUS_M, RADIUS_S, ROW_S, STROKE_WIDTH, Spatial, editor, spatial,
};
use crate::app::design_tokens::typography::TextRole;
use crate::app::design_tokens::util::lerp_color;
use crate::cell_editor::{Cell, CellDiagnostic, CellEditorState};
use crate::highlighting::highlight_source;

/// Build analyzer diagnostics for a specific cell (body-relative coordinates).
fn cell_analyzer_diagnostics(
    cell_index: usize,
    state: &CellEditorState,
) -> Vec<animatix_analyzer::Diagnostic> {
    state
        .diagnostics
        .iter()
        .filter(|d| d.cell_index == cell_index)
        .map(|d| animatix_analyzer::Diagnostic {
            severity: match d.severity {
                animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                    animatix_analyzer::DiagnosticSeverity::Error
                },
                animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                    animatix_analyzer::DiagnosticSeverity::Warning
                },
                animatix_syntax::diagnostics::DiagnosticSeverity::Info => {
                    animatix_analyzer::DiagnosticSeverity::Info
                },
                animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                    animatix_analyzer::DiagnosticSeverity::Hint
                },
            },
            line: d.rel_line,
            col: d.rel_col,
            end_line: d.rel_end_line,
            end_col: d.rel_end_col,
            message: d.message.clone(),
            code: None,
        })
        .collect()
}

/// Choose a left-border color based on diagnostic severity for this cell.
fn diagnostic_border_color(
    index: usize,
    state: &CellEditorState,
    theme: eparts::Theme,
) -> Option<Color32> {
    if state.error_cells.contains(&index) {
        Some(theme.palette.status.error)
    } else if state.warning_cells.contains(&index) {
        Some(theme.palette.status.warning)
    } else {
        None
    }
}

/// Overlay find-match backgrounds onto an already-built syntax `LayoutJob`.
///
/// `ranges` are `(start_byte, end_byte, is_current)` offsets into the job text.
/// Sections are split at range boundaries so only matched slices change;
/// `is_current` ranges win when ranges overlap. This is applied to the cloned
/// job every frame *after* it is fetched from `cached_highlight_jobs`, so
/// changing the query or options never requires invalidating that cache.
fn apply_find_highlights(
    job: &mut egui::text::LayoutJob,
    ranges: &[(usize, usize, bool)],
    match_bg: Color32,
    current_bg: Color32,
) {
    if ranges.is_empty() {
        return;
    }

    let text_len = job.text.len();
    let mut normalized: Vec<(usize, usize, bool)> = ranges
        .iter()
        .filter_map(|&(start, end, is_current)| {
            let start = start.min(text_len);
            let end = end.min(text_len);
            (start < end).then_some((start, end, is_current))
        })
        .collect();
    if normalized.is_empty() {
        return;
    }
    normalized.sort_by_key(|&(start, _, _)| start);

    let old_sections = std::mem::take(&mut job.sections);
    let mut sections = Vec::with_capacity(old_sections.len());
    for section in old_sections {
        let sec_start = section.byte_range.start;
        let sec_end = section.byte_range.end;
        if sec_start >= sec_end {
            continue;
        }

        // Split this section at every find-range boundary that falls inside it.
        let mut points = vec![sec_start, sec_end];
        for &(start, end, _) in &normalized {
            if start > sec_start && start < sec_end {
                points.push(start);
            }
            if end > sec_start && end < sec_end {
                points.push(end);
            }
        }
        points.sort_unstable();
        points.dedup();

        for window in points.windows(2) {
            let (a, b) = (window[0], window[1]);
            if a >= b {
                continue;
            }
            let mut format = section.format.clone();
            let mut bg: Option<Color32> = None;
            for &(start, end, is_current) in &normalized {
                if a >= start && b <= end {
                    if is_current {
                        bg = Some(current_bg);
                        break;
                    }
                    bg.get_or_insert(match_bg);
                }
            }
            if let Some(color) = bg {
                format.background = color;
            }
            sections.push(egui::text::LayoutSection {
                leading_space: section.leading_space,
                byte_range: a..b,
                format,
            });
        }
    }
    job.sections = sections;
}

// ── Public API ───────────────────────────────────────────────────────────

/// Render the cell editor UI.
/// Some native IME integrations deliver only `ImeEvent::Commit` without first
/// sending `ImeEvent::Enabled`. egui's TextEdit uses the Enabled event to record
/// the composition cursor range; without it a Commit at a non-zero cursor is
/// silently ignored. Insert the missing Enabled event before the Commit so
/// existing-text cells accept input the same way newly created empty cells do.
fn normalize_ime_input(ctx: &egui::Context) {
    ctx.input_mut(|i| {
        if i.events.iter().any(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Enabled))) {
            return;
        }
        if let Some(pos) = i
            .events
            .iter()
            .position(|e| matches!(e, egui::Event::Ime(egui::ImeEvent::Commit(_))))
        {
            i.events.insert(pos, egui::Event::Ime(egui::ImeEvent::Enabled));
        }
    });
}

pub fn render_cell_editor(
    ui: &mut egui::Ui,
    cells: &mut [Cell],
    state: &mut CellEditorState,
    on_source_changed: &mut dyn FnMut(String),
    on_scrub_to_time: &mut dyn FnMut(f64),
) {
    normalize_ime_input(ui.ctx());
    let style = ui.style().clone();
    let mut source_changed = false;

    ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        let mut current_source_line = 0usize;
        for (index, cell) in cells.iter_mut().enumerate() {
            let cell_lines = cell.to_source().lines().count();
            let start_line = current_source_line;
            let end_line = start_line + cell_lines;
            current_source_line = end_line;

            if index > 0 {
                divider(ui, index - 1, state);
            }

            let highlighted =
                state.highlighted_cell == Some(index) || state.focused_cell == Some(index);

            let cell_response = match cell {
                Cell::Code { .. } => render_code_cell(
                    ui,
                    index,
                    cell,
                    &style,
                    highlighted,
                    state,
                    &mut source_changed,
                    start_line,
                    end_line,
                ),
                Cell::Keyframe { .. } => render_keyframe_cell(
                    ui,
                    index,
                    cell,
                    &style,
                    highlighted,
                    state,
                    on_scrub_to_time,
                    &mut source_changed,
                    start_line,
                    end_line,
                ),
            };

            if cell_response.clicked() {
                state.focused_cell = Some(index);
                state.highlighted_cell = None;
            }

            if state.scroll_to_cell == Some(index) {
                ui.scroll_to_rect(cell_response.rect, Some(egui::Align::Center));
                state.scroll_to_cell = None;
            }
        }

        // ── End of document append buttons ──
        let sp = spatial(ui);
        ui.add_space(sp.editor.append_spacing);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = sp.base.space_4;
            if ui
                .add(eparts::Button::secondary("+ Keyframe").with_size(eparts::Size::Sm))
                .clicked()
            {
                state.pending_append_at_end = Some(crate::cell_editor::CellType::Keyframe);
            }
            if ui
                .add(eparts::Button::secondary("+ Code").with_size(eparts::Size::Sm))
                .clicked()
            {
                state.pending_append_at_end = Some(crate::cell_editor::CellType::Code);
            }
        });
        ui.add_space(sp.editor.bottom_margin);
    });

    if source_changed {
        on_source_changed(crate::cell_editor::cells_to_source(cells));
    }
}

// ── Header icon button ───────────────────────────────────────────────────

/// Clean 20×20 icon button for cell headers.
///
/// No idle background; a subtle hover bg fades in and the icon brightens.
mod body;
mod chrome;
mod frames;
#[cfg(test)]
mod tests;

use body::*;
use chrome::*;
