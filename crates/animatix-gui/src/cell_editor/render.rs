use egui::{Color32, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};

use crate::app::components::anim;
use crate::app::design_tokens::motion;
use crate::app::design_tokens::spatial::ROW_S;
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
        Some(theme.status.error)
    } else if state.warning_cells.contains(&index) {
        Some(theme.status.warning)
    } else {
        None
    }
}

// ── Public API ───────────────────────────────────────────────────────────

/// Render the cell editor UI.
pub fn render_cell_editor(
    ui: &mut egui::Ui,
    cells: &mut [Cell],
    state: &mut CellEditorState,
    on_source_changed: &mut dyn FnMut(String),
    on_scrub_to_time: &mut dyn FnMut(f64),
) {
    let style = ui.style().clone();
    let mut source_changed = false;

    ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;

        for (index, cell) in cells.iter_mut().enumerate() {
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
    });

    if source_changed {
        on_source_changed(crate::cell_editor::cells_to_source(cells));
    }
}

// ── Header icon button ───────────────────────────────────────────────────

/// Clean 20×20 icon button for cell headers.
///
/// No idle background; a subtle hover bg fades in and the icon brightens.
fn header_btn(ui: &mut egui::Ui, icon: &'static str, tooltip: &'static str) -> bool {
    let size = Vec2::splat(ROW_S);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let theme = eparts::theme(ui);
    let t = anim::animate_toward(
        ui.ctx(),
        response.id,
        if response.hovered() || response.is_pointer_button_down_on() {
            1.0
        } else {
            0.0
        },
        motion::HOVER,
    );

    let bg = if response.is_pointer_button_down_on() {
        theme.surface.active
    } else {
        lerp_color(Color32::TRANSPARENT, theme.surface.hover, t)
    };

    let icon_color = if response.is_pointer_button_down_on() {
        theme.text.primary
    } else {
        lerp_color(theme.text.muted, theme.text.primary, t)
    };

    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 4.0, bg);
    }

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        TextRole::BodyS.font_id(),
        icon_color,
    );

    let clicked = response.clicked();
    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip).clicked();
    }
    clicked
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// If a pending cursor is scheduled for `index`, write it into the egui
/// `TextEditState` so the next render of that cell's body places the caret
/// at the requested char index.
///
/// **Does not** consume `pending_cursor_cell`; the caller must request focus
/// after the `TextEdit` is added and then clear the flag.
fn apply_pending_cursor(ui: &mut egui::Ui, index: usize, state: &mut CellEditorState) {
    if state.pending_cursor_cell == Some(index) {
        if let Some(char_idx) = state.pending_cursor_char.take() {
            let text_edit_id = ui.id().with(("cell_body", index));
            let mut te_state =
                egui::text_edit::TextEditState::load(ui.ctx(), text_edit_id).unwrap_or_default();
            use egui::text::{CCursor, CCursorRange};
            te_state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(char_idx))));
            te_state.store(ui.ctx(), text_edit_id);
        }
    }
}

// ── Code cell ────────────────────────────────────────────────────────────

fn render_code_cell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    state: &mut CellEditorState,
    source_changed: &mut bool,
) -> egui::Response {
    let theme = eparts::theme(ui);
    let expanded = cell.is_expanded(index, &state.collapsed_cells);
    let bg = if highlighted {
        theme.surface.surface
    } else {
        theme.surface.panel
    };
    let border_color = if state.focused_cell == Some(index) {
        Some(theme.accent.primary)
    } else {
        diagnostic_border_color(index, state, theme)
    };
    let cell_diags = cell_analyzer_diagnostics(index, state);

    let diagnostic_margin = border_color.map(|_| Margin {
        left: 2,
        right: 0,
        top: 0,
        bottom: 0,
    });

    Frame::new()
        .fill(border_color.unwrap_or(bg))
        .inner_margin(diagnostic_margin.unwrap_or(Margin::symmetric(0, 0)))
        .show(ui, |ui| {
            Frame::new().fill(bg).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
                ui.vertical(|ui| {
                    // Header row
                    let _header_response = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                        let toggle = if expanded {
                            egui_phosphor::regular::CARET_DOWN
                        } else {
                            egui_phosphor::regular::CARET_RIGHT
                        };
                        if header_btn(ui, toggle, if expanded { "Collapse" } else { "Expand" }) {
                            cell.set_expanded(!expanded);
                        }

                        // Code type icon + label
                        ui.label(
                            RichText::new(egui_phosphor::regular::CODE)
                                .size(TextRole::BodyS.size())
                                .color(theme.text.muted),
                        );
                        ui.label(
                            RichText::new(format!("Code {index}"))
                                .size(TextRole::Micro.size())
                                .color(theme.text.muted),
                        );

                        // Right-aligned actions
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                            if header_btn(ui, egui_phosphor::regular::TRASH, "Delete code block") {
                                state.pending_delete_cell = Some(index);
                            }
                            if header_btn(ui, egui_phosphor::regular::ARROW_DOWN, "Move down") {
                                state.pending_move_down = Some(index);
                            }
                            if header_btn(ui, egui_phosphor::regular::ARROW_UP, "Move up") {
                                state.pending_move_up = Some(index);
                            }
                        });
                    });

                    if expanded {
                        ui.add_space(4.0);

                        apply_pending_cursor(ui, index, state);

                        let layouter_style = style.clone();
                        let cell_diags_ref = cell_diags.clone();
                        let cell_semantic: Vec<_> = state
                            .semantic_highlights
                            .iter()
                            .filter(|sh| sh.cell_index == index)
                            .cloned()
                            .collect();
                        // Cached highlight: skip highlight_source when cell body unchanged
                        let body_text = cell.body().to_string();
                        let cached_job = state
                            .cached_highlight_jobs
                            .get(&index)
                            .filter(|(cached_body, _)| cached_body == &body_text)
                            .map(|(_, job)| job.clone());
                        let base_job = if let Some(job) = cached_job {
                            job
                        } else {
                            let new_job = highlight_source(
                                &body_text,
                                &layouter_style,
                                &cell_diags_ref,
                                None,
                                &cell_semantic,
                            );
                            state
                                .cached_highlight_jobs
                                .insert(index, (body_text.clone(), new_job.clone()));
                            new_job
                        };
                        let mut layouter =
                            move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                                let buf_text = buf.as_str();
                                let mut job = if buf_text == base_job.text.as_str() {
                                    base_job.clone()
                                } else {
                                    highlight_source(
                                        buf_text,
                                        &layouter_style,
                                        &cell_diags_ref,
                                        None,
                                        &cell_semantic,
                                    )
                                };
                                job.wrap.max_width = wrap_width;
                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                            };

                        let text_edit_id = ui.id().with(("cell_body", index));
                        let response = ui.add(
                            egui::TextEdit::multiline(cell.body_mut())
                                .id(text_edit_id)
                                .code_editor()
                                .frame(Frame::NONE)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter),
                        );
                        if response.changed() {
                            *source_changed = true;
                        }
                        if state.pending_cursor_cell == Some(index) {
                            response.request_focus();
                            state.pending_cursor_cell = None;
                        }
                        track_focus(index, &response, state);

                        // Draw wavy diagnostic underlines
                        let cell_underlines: Vec<CellDiagnostic> = state
                            .diagnostics
                            .iter()
                            .filter(|d| d.cell_index == index)
                            .cloned()
                            .collect();
                        if !cell_underlines.is_empty() {
                            draw_wavy_underlines(ui, &cell_underlines, response.rect, theme);
                        }
                    }
                });
            });
        })
        .response
}

// ── Keyframe cell ────────────────────────────────────────────────────────

fn render_keyframe_cell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    state: &mut CellEditorState,
    on_scrub_to_time: &mut dyn FnMut(f64),
    source_changed: &mut bool,
) -> egui::Response {
    let theme = eparts::theme(ui);
    let time_s = cell.time_s().unwrap_or(0.0);
    let expanded = cell.is_expanded(index, &state.collapsed_cells);
    let bg = if highlighted {
        theme.surface.surface
    } else {
        theme.surface.panel
    };
    let border_color = if state.focused_cell == Some(index) {
        Some(theme.accent.primary)
    } else {
        diagnostic_border_color(index, state, theme)
    };
    let cell_diags = cell_analyzer_diagnostics(index, state);

    let diagnostic_margin = border_color.map(|_| Margin {
        left: 2,
        right: 0,
        top: 0,
        bottom: 0,
    });

    Frame::new()
        .fill(border_color.unwrap_or(bg))
        .inner_margin(diagnostic_margin.unwrap_or(Margin::symmetric(0, 0)))
        .show(ui, |ui| {
            Frame::new().fill(bg).show(ui, |ui| {
                ui.vertical(|ui| {
                    // ── Header bar ──────────────────────────────
                    Frame::new()
                        .fill(theme.surface.base)
                        .inner_margin(Margin::symmetric(10, 5))
                        .show(ui, |ui| {
                            ui.set_min_height(26.0);
                            let _header_response = ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                                let toggle = if expanded {
                                    egui_phosphor::regular::CARET_DOWN
                                } else {
                                    egui_phosphor::regular::CARET_RIGHT
                                };
                                if header_btn(
                                    ui,
                                    toggle,
                                    if expanded { "Collapse" } else { "Expand" },
                                ) {
                                    if expanded {
                                        state.collapsed_cells.insert(index);
                                    } else {
                                        state.collapsed_cells.remove(&index);
                                    }
                                }

                                // Keyframe type icon
                                ui.label(
                                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                                        .size(TextRole::BodyS.size())
                                        .color(theme.text.muted),
                                );

                                // Editable timestamp
                                render_timestamp_editor(ui, cell, source_changed, state, index);

                                // Play
                                if header_btn(
                                    ui,
                                    egui_phosphor::regular::PLAY,
                                    "Play from this keyframe",
                                ) {
                                    on_scrub_to_time(time_s);
                                }

                                // Right-aligned actions
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                                        if header_btn(
                                            ui,
                                            egui_phosphor::regular::TRASH,
                                            "Delete keyframe",
                                        ) {
                                            state.pending_delete_cell = Some(index);
                                        }
                                        if header_btn(
                                            ui,
                                            egui_phosphor::regular::ARROW_DOWN,
                                            "Move down",
                                        ) {
                                            state.pending_move_down = Some(index);
                                        }
                                        if header_btn(
                                            ui,
                                            egui_phosphor::regular::ARROW_UP,
                                            "Move up",
                                        ) {
                                            state.pending_move_up = Some(index);
                                        }
                                    },
                                );
                            });
                        });

                    // ── Body editor ─────────────────────────────
                    if expanded {
                        Frame::new().fill(bg).inner_margin(Margin::symmetric(10, 8)).show(
                            ui,
                            |ui| {
                                let layouter_style = style.clone();
                                let cell_diags_ref = cell_diags.clone();
                                let cell_semantic: Vec<_> = state
                                    .semantic_highlights
                                    .iter()
                                    .filter(|sh| sh.cell_index == index)
                                    .cloned()
                                    .collect();
                                // Cached highlight: skip highlight_source when cell body unchanged
                                let body_text = cell.body().to_string();
                                let cached_job = state
                                    .cached_highlight_jobs
                                    .get(&index)
                                    .filter(|(cached_body, _)| cached_body == &body_text)
                                    .map(|(_, job)| job.clone());
                                let base_job = if let Some(job) = cached_job {
                                    job
                                } else {
                                    let new_job = highlight_source(
                                        &body_text,
                                        &layouter_style,
                                        &cell_diags_ref,
                                        None,
                                        &cell_semantic,
                                    );
                                    state
                                        .cached_highlight_jobs
                                        .insert(index, (body_text.clone(), new_job.clone()));
                                    new_job
                                };
                                let mut layouter =
                                    move |ui: &egui::Ui,
                                          buf: &dyn egui::TextBuffer,
                                          wrap_width: f32| {
                                        let buf_text = buf.as_str();
                                        let mut job = if buf_text == base_job.text.as_str() {
                                            base_job.clone()
                                        } else {
                                            highlight_source(
                                                buf_text,
                                                &layouter_style,
                                                &cell_diags_ref,
                                                None,
                                                &cell_semantic,
                                            )
                                        };
                                        job.wrap.max_width = wrap_width;
                                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                                    };

                                apply_pending_cursor(ui, index, state);

                                let text_edit_id = ui.id().with(("cell_body", index));
                                let response = ui.add(
                                    egui::TextEdit::multiline(cell.body_mut())
                                        .id(text_edit_id)
                                        .code_editor()
                                        .frame(Frame::NONE)
                                        .desired_width(f32::INFINITY)
                                        .layouter(&mut layouter),
                                );
                                if response.changed() {
                                    *source_changed = true;
                                }
                                if state.pending_cursor_cell == Some(index) {
                                    response.request_focus();
                                    state.pending_cursor_cell = None;
                                }
                                track_focus(index, &response, state);

                                // Draw wavy diagnostic underlines
                                let cell_underlines: Vec<CellDiagnostic> = state
                                    .diagnostics
                                    .iter()
                                    .filter(|d| d.cell_index == index)
                                    .cloned()
                                    .collect();
                                if !cell_underlines.is_empty() {
                                    draw_wavy_underlines(
                                        ui,
                                        &cell_underlines,
                                        response.rect,
                                        theme,
                                    );
                                }
                            },
                        );
                    }
                });
            });
        })
        .response
}

// ── Timestamp inline editor ──────────────────────────────────────────────

fn render_timestamp_editor(
    ui: &mut egui::Ui,
    cell: &mut Cell,
    source_changed: &mut bool,
    state: &mut CellEditorState,
    cell_index: usize,
) {
    let (raw_ts, is_rel) = match cell {
        Cell::Keyframe {
            timestamp,
            is_relative,
            ..
        } => (timestamp.clone(), *is_relative),
        _ => return,
    };

    let theme = eparts::theme(ui);
    let is_editing = state.editing_timestamp_cell == Some(cell_index);

    if is_editing {
        let mut edited = raw_ts.clone();
        let ts_response = ui.add(
            egui::TextEdit::singleline(&mut edited)
                .font(TextRole::Mono.font_id())
                .desired_width(100.0)
                .frame(Frame::NONE)
                .text_color(theme.accent.primary),
        );

        if ts_response.changed() {
            let new_is_relative = if edited.starts_with('+') {
                true
            } else {
                is_rel
            };

            if let Cell::Keyframe {
                timestamp,
                is_relative,
                ..
            } = cell
            {
                *timestamp = edited.trim().to_string();
                *is_relative = new_is_relative;
                *source_changed = true;
            }
        }

        if ts_response.lost_focus() {
            state.editing_timestamp_cell = None;
        }

        track_focus(cell_index, &ts_response, state);
    } else {
        let display = cell.display_timestamp().unwrap_or_else(|| raw_ts.clone());
        let label_response = ui.add(
            egui::Label::new(
                RichText::new(display)
                    .font(TextRole::Mono.font_id())
                    .color(theme.accent.primary),
            )
            .sense(egui::Sense::click()),
        );
        if label_response.clicked() {
            state.editing_timestamp_cell = Some(cell_index);
        }
        if label_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    }
}

// ── Focus tracking ───────────────────────────────────────────────────────

fn track_focus(index: usize, response: &egui::Response, state: &mut CellEditorState) {
    if response.gained_focus() {
        state.focused_cell = Some(index);
        state.highlighted_cell = None;
    }
}

/// Draw wavy squiggly underlines for diagnostics below the text.
///
/// Uses the egui painter to draw a series of small zigzag segments under each
/// diagnostic span.  Error diagnostics get red waves, warnings get amber waves.
fn draw_wavy_underlines(
    ui: &egui::Ui,
    diags: &[CellDiagnostic],
    text_rect: egui::Rect,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    let font_id = TextRole::Mono.font_id();

    // Get monospace metrics for position calculation
    let char_width = ui.fonts_mut(|f| f.glyph_width(&font_id, 'm'));
    let line_height = ui.fonts_mut(|f| f.row_height(&font_id));

    for d in diags {
        let color = match d.severity {
            animatix_syntax::diagnostics::DiagnosticSeverity::Error => theme.status.error,
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning
            | animatix_syntax::diagnostics::DiagnosticSeverity::Info
            | animatix_syntax::diagnostics::DiagnosticSeverity::Hint => theme.status.warning,
        };

        // Y position: baseline below the diagnostic line
        let y = text_rect.top() + (d.rel_line as f32 + 1.0) * line_height - 2.0;

        // X positions
        let x_start = text_rect.left() + d.rel_col as f32 * char_width;
        let x_end = text_rect.left() + d.rel_end_col as f32 * char_width;

        if x_end <= x_start
            || y < text_rect.top() - line_height
            || y > text_rect.bottom() + line_height
        {
            continue;
        }

        // Draw wavy line: series of small zigzag segments
        let width = x_end - x_start;
        let wave_len = 4.0; // pixels per half-wave
        let wave_amp = 1.5; // amplitude in pixels
        let num_segments = (width / wave_len).ceil() as usize;
        let actual_seg_width = width / num_segments.max(1) as f32;

        let mut points = Vec::with_capacity(num_segments + 1);
        for i in 0..=num_segments {
            let x = x_start + i as f32 * actual_seg_width;
            let y_off = if i % 2 == 0 { 0.0 } else { wave_amp };
            points.push(egui::Pos2::new(x, y + y_off));
        }

        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
    }
}

// ── Divider between cells ────────────────────────────────────────────────

fn divider(ui: &mut egui::Ui, after_index: usize, state: &mut CellEditorState) {
    let theme = eparts::theme(ui);
    let available = ui.available_width();
    let height = 36.0; // taller for breathing room
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    let center = rect.center();
    let y = center.y;
    let left = rect.left() + 48.0;
    let right = rect.right() - 48.0;

    // Smooth hover transition for the divider line
    let t = anim::animate_toward(
        ui.ctx(),
        ui.id().with(("divider_line", after_index)),
        if response.hovered() { 1.0 } else { 0.0 },
        motion::HOVER,
    );

    // ── Divider line (always visible, brightens on hover) ──
    if left < right {
        let line_color = if t > 0.0 {
            theme.border.strong
        } else {
            theme.border.default
        };
        let line_a = egui::lerp(120.0..=220.0, t) as u8;
        ui.painter().line_segment(
            [egui::pos2(left, y), egui::pos2(right, y)],
            Stroke::new(
                egui::lerp(1.0..=1.5, t),
                Color32::from_rgba_premultiplied(
                    line_color.r(),
                    line_color.g(),
                    line_color.b(),
                    line_a,
                ),
            ),
        );
    }

    // ── Center "add" button — ALWAYS visible ──
    let btn_size = Vec2::splat(24.0);
    let btn_rect = egui::Rect::from_center_size(center, btn_size);

    // Animate the button independently so it has smooth transitions even though
    // it's always on screen.
    let btn_t = anim::animate_toward(
        ui.ctx(),
        ui.id().with(("divider_btn", after_index)),
        if response.hovered() || response.is_pointer_button_down_on() {
            1.0
        } else {
            0.0
        },
        motion::HOVER,
    );
    let pressed = response.is_pointer_button_down_on();

    // Background (always visible — no alpha tricks)
    let bg_idle = theme.surface.widget;
    let bg_hover = theme.surface.hover;
    let bg = if pressed {
        theme.status.warning
    } else {
        lerp_color(bg_idle, bg_hover, btn_t)
    };

    // Border (subtle idle, stronger hover)
    let border = if pressed {
        theme.status.warning
    } else if btn_t > 0.0 {
        theme.border.strong
    } else {
        theme.border.default
    };

    // Icon color
    let icon = if pressed {
        theme.text.on_accent
    } else if btn_t > 0.0 {
        theme.text.primary
    } else {
        theme.text.secondary
    };

    ui.painter().rect_filled(btn_rect, 6.0, bg);
    ui.painter()
        .rect_stroke(btn_rect, 6.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);

    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        TextRole::Body.font_id(),
        icon,
    );

    if response.clicked() {
        state.pending_insert_after = Some(after_index);
    }

    use crate::app::components::context_menu::{MenuEntry, render_menu};

    response.context_menu(|ui| {
        let entries = vec![
            MenuEntry::item_with_icon(egui_phosphor::regular::FILM_STRIP, "Insert keyframe"),
            MenuEntry::item_with_icon(egui_phosphor::regular::CODE, "Insert code block"),
        ];
        if let Some(idx) = render_menu(ui, &entries) {
            match idx {
                0 => state.pending_insert_after = Some(after_index),
                1 => state.pending_insert_code_after = Some(after_index),
                _ => {},
            }
            ui.close();
        }
    });
}
