use egui::{Color32, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};

use crate::cell_editor::{Cell, CellEditorState};
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
                animatix::diagnostics::DiagnosticSeverity::Error => {
                    animatix_analyzer::DiagnosticSeverity::Error
                }
                animatix::diagnostics::DiagnosticSeverity::Warning => {
                    animatix_analyzer::DiagnosticSeverity::Warning
                }
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
fn diagnostic_border_color(index: usize, state: &CellEditorState) -> Option<Color32> {
    if state.error_cells.contains(&index) {
        Some(Color32::from_rgb(255, 100, 100)) // red
    } else if state.warning_cells.contains(&index) {
        Some(Color32::from_rgb(255, 196, 92)) // amber
    } else {
        None
    }
}

// ── Palette ──────────────────────────────────────────────────────────────

const AMBER: Color32 = Color32::from_rgb(255, 196, 92);

const KEYFRAME_BG: Color32 = Color32::from_rgb(28, 31, 38);
const KEYFRAME_BG_HIGHLIGHT: Color32 = Color32::from_rgb(44, 42, 34);
const KEYFRAME_HEADER_BG: Color32 = Color32::from_rgb(24, 27, 33);

const CODE_BG: Color32 = Color32::from_rgb(22, 25, 32);
const CODE_BG_HIGHLIGHT: Color32 = Color32::from_rgb(36, 40, 50);

// Unified ghost-button palette
const GHOST_BG_IDLE: Color32 = Color32::from_rgb(38, 42, 50);
const GHOST_BG_HOVER: Color32 = Color32::from_rgb(60, 66, 78);
const GHOST_ICON_IDLE: Color32 = Color32::from_rgb(140, 150, 170);
const GHOST_ICON_HOVER: Color32 = Color32::from_rgb(210, 215, 230);

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
                Cell::Code { .. } => {
                    render_code_cell(ui, index, cell, &style, highlighted, state, &mut source_changed)
                }
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

// ── Unified ghost icon button ────────────────────────────────────────────

/// A small, consistently-styled icon button with smooth hover / press transitions.
///
/// * Idle:    dark rounded-square bg + muted icon
/// * Hover:   lighter bg + bright icon  (100 ms fade)
/// * Pressed: amber-filled bg + dark icon (instant, snaps back on release)
fn ghost_icon_btn(
    ui: &mut egui::Ui,
    icon: &'static str,
    tooltip: &'static str,
) -> bool {
    let size = Vec2::splat(22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let pressed = response.is_pointer_button_down_on();
    let t = ui.ctx().animate_value_with_time(
        response.id,
        if response.hovered() || pressed { 1.0 } else { 0.0 },
        0.10,
    );

    let bg = if pressed {
        AMBER
    } else {
        lerp_color(GHOST_BG_IDLE, GHOST_BG_HOVER, t)
    };

    let icon_color = if pressed {
        Color32::from_rgb(24, 27, 33)
    } else {
        lerp_color(GHOST_ICON_IDLE, GHOST_ICON_HOVER, t)
    };

    ui.painter()
        .rect_filled(rect, 5.0, bg);

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
        icon_color,
    );

    let clicked = response.clicked();
    if !tooltip.is_empty() {
        response.on_hover_text(tooltip);
    }
    clicked
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
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
            let mut te_state = egui::text_edit::TextEditState::load(ui.ctx(), text_edit_id)
                .unwrap_or_default();
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
    let expanded = cell.is_expanded();
    let bg = if highlighted { CODE_BG_HIGHLIGHT } else { CODE_BG };
    let border_color = diagnostic_border_color(index, state);
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
            Frame::new()
                .fill(bg)
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                            let toggle = if expanded {
                                egui_phosphor::regular::CARET_DOWN
                            } else {
                                egui_phosphor::regular::CARET_RIGHT
                            };
                            if ghost_icon_btn(
                                ui,
                                toggle,
                                if expanded { "Collapse" } else { "Expand" },
                            ) {
                                cell.set_expanded(!expanded);
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("Code {index}"))
                                        .size(11.0)
                                        .color(Color32::from_rgb(140, 150, 170)),
                                )
                                .selectable(false),
                            );
                        });

                        if expanded {
                            ui.add_space(4.0);

                            apply_pending_cursor(ui, index, state);

                            let layouter_style = style.clone();
                            let cell_diags_ref = cell_diags.clone();
                            let mut layouter = move |ui: &egui::Ui,
                                                     buf: &dyn egui::TextBuffer,
                                                     wrap_width: f32| {
                                let mut job = highlight_source(
                                    buf.as_str(),
                                    &layouter_style,
                                    &cell_diags_ref,
                                    None,
                                );
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
    let time_s = cell.time_s().unwrap_or(0.0);
    let bg = if highlighted {
        KEYFRAME_BG_HIGHLIGHT
    } else {
        KEYFRAME_BG
    };
    let border_color = diagnostic_border_color(index, state);
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
            Frame::new()
                .fill(bg)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        // ── Header bar ──────────────────────────────
                        Frame::new()
                            .fill(KEYFRAME_HEADER_BG)
                            .inner_margin(Margin::symmetric(10, 5))
                            .show(ui, |ui| {
                                ui.set_min_height(26.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                                    // Play
                                    if ghost_icon_btn(
                                        ui,
                                        egui_phosphor::regular::PLAY,
                                        "Play from this keyframe",
                                    ) {
                                        on_scrub_to_time(time_s);
                                    }

                                    // Editable timestamp
                                    render_timestamp_editor(
                                        ui,
                                        cell,
                                        source_changed,
                                        state,
                                        index,
                                    );

                                    // Delete button (right-aligned)
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ghost_icon_btn(
                                                ui,
                                                egui_phosphor::regular::TRASH,
                                                "Delete keyframe",
                                            ) {
                                                state.pending_delete_cell = Some(index);
                                            }
                                        },
                                    );
                                });
                            });

                        // ── Body editor ─────────────────────────────
                        Frame::new()
                            .fill(bg)
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                let layouter_style = style.clone();
                                let cell_diags_ref = cell_diags.clone();
                                let mut layouter = move |ui: &egui::Ui,
                                                         buf: &dyn egui::TextBuffer,
                                                         wrap_width: f32| {
                                    let mut job = highlight_source(
                                        buf.as_str(),
                                        &layouter_style,
                                        &cell_diags_ref,
                                        None,
                                    );
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
                            });
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

    let mut edited = raw_ts.clone();

    let ts_response = ui.add(
        egui::TextEdit::singleline(&mut edited)
            .font(egui::FontId::monospace(12.0))
            .desired_width(80.0)
            .frame(Frame::NONE),
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

    track_focus(cell_index, &ts_response, state);
}

// ── Focus tracking ───────────────────────────────────────────────────────

fn track_focus(index: usize, response: &egui::Response, state: &mut CellEditorState) {
    if response.gained_focus() {
        state.focused_cell = Some(index);
        state.highlighted_cell = None;
    }
}

// ── Divider between cells ────────────────────────────────────────────────

fn divider(ui: &mut egui::Ui, after_index: usize, state: &mut CellEditorState) {
    let available = ui.available_width();
    let height = 36.0; // taller for breathing room
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    let center = rect.center();
    let y = center.y;
    let left = rect.left() + 48.0;
    let right = rect.right() - 48.0;

    // Smooth hover transition for the divider line
    let t = ui.ctx().animate_value_with_time(
        ui.id().with(("divider_line", after_index)),
        if response.hovered() { 1.0 } else { 0.0 },
        0.12,
    );

    // ── Divider line (always visible, brightens on hover) ──
    if left < right {
        let line_a = (egui::lerp(20.0..=80.0, t) as u8).min(255);
        ui.painter().line_segment(
            [egui::pos2(left, y), egui::pos2(right, y)],
            Stroke::new(
                egui::lerp(1.0..=1.5, t),
                Color32::from_rgba_premultiplied(100, 108, 125, line_a),
            ),
        );
    }

    // ── Center "add" button — ALWAYS visible ──
    let btn_size = Vec2::splat(24.0);
    let btn_rect = egui::Rect::from_center_size(center, btn_size);

    // Animate the button independently so it has smooth transitions even though
    // it's always on screen.
    let btn_t = ui.ctx().animate_value_with_time(
        ui.id().with(("divider_btn", after_index)),
        if response.hovered() || response.is_pointer_button_down_on() {
            1.0
        } else {
            0.0
        },
        0.10,
    );
    let pressed = response.is_pointer_button_down_on();

    // Background (always visible — no alpha tricks)
    let bg_idle = Color32::from_rgb(32, 36, 44);
    let bg_hover = Color32::from_rgb(50, 55, 66);
    let bg = if pressed {
        AMBER
    } else {
        lerp_color(bg_idle, bg_hover, btn_t)
    };

    // Border (subtle idle, stronger hover)
    let border = if pressed {
        AMBER
    } else {
        let border_a = (egui::lerp(40.0..=100.0, btn_t) as u8).min(255);
        Color32::from_rgba_premultiplied(120, 130, 150, border_a)
    };

    // Icon color
    let icon = if pressed {
        Color32::from_rgb(24, 27, 33)
    } else {
        let icon_a = (egui::lerp(100.0..=220.0, btn_t) as u8).min(255);
        Color32::from_rgba_premultiplied(200, 205, 215, icon_a)
    };

    ui.painter().rect_filled(btn_rect, 6.0, bg);
    ui.painter().rect_stroke(btn_rect, 6.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);

    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::PLUS,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
        icon,
    );

    if response.clicked() {
        state.pending_insert_after = Some(after_index);
    }

    use crate::app::components::context_menu::{render_menu, MenuEntry};

    response.context_menu(|ui| {
        let entries = vec![
            MenuEntry::item_with_icon(egui_phosphor::regular::FILM_STRIP, "Insert keyframe"),
            MenuEntry::item_with_icon(egui_phosphor::regular::CODE, "Insert code block"),
        ];
        if let Some(idx) = render_menu(ui, &entries) {
            match idx {
                0 => state.pending_insert_after = Some(after_index),
                1 => state.pending_insert_code_after = Some(after_index),
                _ => {}
            }
            ui.close();
        }
    });
}
