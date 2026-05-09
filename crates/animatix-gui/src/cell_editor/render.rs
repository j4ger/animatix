use egui::{Color32, Frame, Margin, RichText, ScrollArea, Stroke, Vec2};

use crate::cell_editor::{Cell, CellEditorState};
use crate::highlighting::highlight_source;

// ── Palette ──────────────────────────────────────────────────────────────

const AMBER: Color32 = Color32::from_rgb(255, 196, 92);

const KEYFRAME_BG: Color32 = Color32::from_rgb(28, 31, 38);
const KEYFRAME_BG_HIGHLIGHT: Color32 = Color32::from_rgb(44, 42, 34);
const KEYFRAME_HEADER_BG: Color32 = Color32::from_rgb(24, 27, 33);

const CODE_BG: Color32 = Color32::from_rgb(22, 25, 32);
const CODE_BG_HIGHLIGHT: Color32 = Color32::from_rgb(36, 40, 50);

const DIVIDER_LINE: Color32 = Color32::from_rgb(38, 42, 50);
const DIVIDER_LINE_HOVER: Color32 = Color32::from_rgb(80, 88, 102);

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
                    render_code_cell(ui, index, cell, &style, highlighted, &mut source_changed)
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

// ── Code cell ────────────────────────────────────────────────────────────

fn render_code_cell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    source_changed: &mut bool,
) -> egui::Response {
    let expanded = cell.is_expanded();
    let bg = if highlighted { CODE_BG_HIGHLIGHT } else { CODE_BG };

    Frame::new()
        .fill(bg)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    // Use ASCII "v" / ">" instead of Unicode geometric shapes
                    // to avoid tofu blocks in egui's default font atlas.
                    let toggle = if expanded { "v" } else { ">" };
                    if ui
                        .small_button(toggle)
                        .on_hover_text(if expanded { "Collapse" } else { "Expand" })
                        .clicked()
                    {
                        cell.set_expanded(!expanded);
                    }
                    ui.label(
                        RichText::new(format!("Code {index}"))
                            .monospace()
                            .size(11.0)
                            .color(Color32::from_rgb(140, 150, 170)),
                    );
                });

                if expanded {
                    ui.add_space(4.0);

                    let layouter_style = style.clone();
                    let mut layouter = move |ui: &egui::Ui,
                                             buf: &dyn egui::TextBuffer,
                                             wrap_width: f32| {
                        let mut job = highlight_source(buf.as_str(), &layouter_style, &[], None);
                        job.wrap.max_width = wrap_width;
                        ui.fonts_mut(|fonts| fonts.layout_job(job))
                    };

                    let response = ui.add(
                        egui::TextEdit::multiline(cell.body_mut())
                            .code_editor()
                            .frame(Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter),
                    );
                    if response.changed() {
                        *source_changed = true;
                    }
                }
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
    let timestamp = cell.timestamp_text().unwrap_or("0s").to_string();
    let time_s = cell.time_s().unwrap_or(0.0);
    let bg = if highlighted {
        KEYFRAME_BG_HIGHLIGHT
    } else {
        KEYFRAME_BG
    };

    // Amber left border implemented as a nested frame so the widget feels
    // like a single cohesive card.
    Frame::new()
        .fill(AMBER)
        .inner_margin(Margin {
            left: 2,
            right: 0,
            top: 0,
            bottom: 0,
        })
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
                                    // Play — use ASCII ">" to avoid tofu.
                                    if ui
                                        .small_button(">")
                                        .on_hover_text("Play from this keyframe")
                                        .clicked()
                                    {
                                        on_scrub_to_time(time_s);
                                    }

                                    ui.add_space(6.0);

                                    // Timestamp label (no emoji, monospace, amber)
                                    let ts_label = if timestamp.starts_with('+') {
                                        timestamp.clone()
                                    } else {
                                        format!("#{}", timestamp)
                                    };
                                    ui.label(
                                        RichText::new(ts_label)
                                            .monospace()
                                            .size(12.0)
                                            .color(AMBER),
                                    );

                                    // Right-aligned overflow menu
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.menu_button(
                                                RichText::new("...")
                                                    .size(12.0)
                                                    .color(Color32::from_rgb(160, 160, 160)),
                                                |ui| {
                                                    ui.set_min_width(160.0);

                                                    if ui.button("Delete").clicked() {
                                                        state.pending_delete_cell = Some(index);
                                                        ui.close();
                                                    }
                                                    if ui.button("Duplicate").clicked() {
                                                        state.pending_duplicate_cell =
                                                            Some(index);
                                                        ui.close();
                                                    }
                                                    if ui.button("Toggle absolute / relative")
                                                        .clicked()
                                                    {
                                                        cell.toggle_timestamp_type();
                                                        *source_changed = true;
                                                        ui.close();
                                                    }
                                                },
                                            );
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
                                let mut layouter = move |ui: &egui::Ui,
                                                         buf: &dyn egui::TextBuffer,
                                                         wrap_width: f32| {
                                    let mut job =
                                        highlight_source(buf.as_str(), &layouter_style, &[], None);
                                    job.wrap.max_width = wrap_width;
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                };

                                let response = ui.add(
                                    egui::TextEdit::multiline(cell.body_mut())
                                        .code_editor()
                                        .frame(Frame::NONE)
                                        .desired_width(f32::INFINITY)
                                        .layouter(&mut layouter),
                                );
                                if response.changed() {
                                    *source_changed = true;
                                }
                            });
                    });
                });
        })
        .response
}

// ── Divider between cells ────────────────────────────────────────────────

fn divider(ui: &mut egui::Ui, after_index: usize, state: &mut CellEditorState) {
    let available = ui.available_width();
    let height = 22.0; // Taller hit-area for easier interaction
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(available, height), egui::Sense::click());

    let hover = response.hovered();
    let center = rect.center();

    // Background: completely invisible unless hovered
    if hover {
        ui.painter().rect_filled(
            rect,
            4.0,
            Color32::from_rgba_premultiplied(255, 196, 92, 8),
        );
    }

    // Line: subtle by default, brighter on hover, with inset margins
    let line_color = if hover { DIVIDER_LINE_HOVER } else { DIVIDER_LINE };
    let stroke = Stroke::new(if hover { 1.5 } else { 1.0 }, line_color);
    let y = center.y;
    let left = rect.left() + 24.0;
    let right = rect.right() - 24.0;
    if left < right {
        ui.painter()
            .line_segment([egui::pos2(left, y), egui::pos2(right, y)], stroke);
    }

    // Hover-only "+" indicator: purely visual, painted, not a separate widget.
    // The entire divider rect is clickable, so we avoid interaction-layer
    // fights where a child widget steals hover from its parent.
    if hover {
        let btn_size = Vec2::new(22.0, 22.0);
        let btn_rect = egui::Rect::from_center_size(center, btn_size);

        let btn_bg = Color32::from_rgba_premultiplied(255, 196, 92, 25);
        ui.painter().rect_filled(btn_rect, 11.0, btn_bg);

        let btn_stroke = Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 196, 92, 60));
        ui.painter().rect_stroke(btn_rect, 11.0, btn_stroke, egui::StrokeKind::Middle);

        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "+",
            egui::FontId::monospace(14.0),
            AMBER,
        );
    }

    if response.clicked() {
        state.pending_insert_after = Some(after_index);
    }
}
