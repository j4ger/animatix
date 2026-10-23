use super::*;

pub(crate) fn cell_card_frame(bg: Color32, border_color: Color32) -> Frame {
    Frame::new()
        .fill(bg)
        .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
        .stroke(Stroke::new(STROKE_WIDTH, border_color))
        .inner_margin(Margin::same(0))
}

pub(crate) fn cell_header_frame(theme: eparts::Theme, expanded: bool, sp: &Spatial) -> Frame {
    let r = RADIUS_M as u8;
    Frame::new()
        .fill(theme.palette.surface.base)
        .corner_radius(egui::CornerRadius {
            nw: r,
            ne: r,
            sw: if expanded { 0 } else { r },
            se: if expanded { 0 } else { r },
        })
        .inner_margin(Margin::symmetric(sp.base.space_4 as i8, (sp.base.space_1 + 1.0) as i8))
}

pub(crate) fn cell_body_frame(bg: Color32, sp: &Spatial) -> Frame {
    let r = RADIUS_M as u8;
    Frame::new()
        .fill(bg)
        .corner_radius(egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: r,
            se: r,
        })
        .inner_margin(Margin {
            left: sp.base.space_4 as i8,
            right: sp.base.space_4 as i8,
            top: sp.base.space_2 as i8,
            bottom: sp.base.space_3 as i8,
        })
}

pub(super) fn toggle_cell_expansion(
    cell: &mut Cell,
    index: usize,
    expanded: bool,
    state: &mut CellEditorState,
) {
    match cell {
        Cell::Code { .. } => cell.set_expanded(!expanded),
        Cell::Keyframe { .. } => {
            if expanded {
                state.collapsed_cells.insert(index);
            } else {
                state.collapsed_cells.remove(&index);
            }
        },
    }
}

pub(crate) fn render_line_range_badge(
    ui: &mut egui::Ui,
    start_line: usize,
    end_line: usize,
    theme: eparts::Theme,
) {
    let line_range_text = if start_line + 1 >= end_line {
        format!("Ln {}", start_line + 1)
    } else {
        format!("Ln {}–{}", start_line + 1, end_line)
    };
    ui.label(
        RichText::new(line_range_text)
            .size(TextRole::Micro.size())
            .color(theme.palette.text.muted),
    );
}

pub(crate) fn render_cell_header_actions(
    ui: &mut egui::Ui,
    index: usize,
    item_type_label: &str,
    state: &mut CellEditorState,
    sp: &Spatial,
) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
        let del_tip = if item_type_label == "keyframe" {
            "Delete keyframe"
        } else {
            "Delete code block"
        };
        if header_btn(ui, egui_phosphor::regular::TRASH, del_tip) {
            state.pending_delete_cell = Some(index);
        }
        let dup_tip = if item_type_label == "keyframe" {
            "Duplicate keyframe"
        } else {
            "Duplicate code block"
        };
        if header_btn(ui, egui_phosphor::regular::COPY, dup_tip) {
            state.pending_duplicate_cell = Some(index);
        }
        if header_btn(ui, egui_phosphor::regular::ARROW_DOWN, "Move down") {
            state.pending_move_down = Some(index);
        }
        if header_btn(ui, egui_phosphor::regular::ARROW_UP, "Move up") {
            state.pending_move_up = Some(index);
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_cell_shell(
    ui: &mut egui::Ui,
    index: usize,
    cell: &mut Cell,
    style: &egui::Style,
    highlighted: bool,
    state: &mut CellEditorState,
    source_changed: &mut bool,
    body_start_line: usize,
    start_line: usize,
    end_line: usize,
    item_type_label: &'static str,
    render_header_middle: impl FnOnce(
        &mut egui::Ui,
        &mut Cell,
        &mut CellEditorState,
        &mut bool,
        eparts::Theme,
        &Spatial,
    ),
) -> egui::Response {
    let theme = eparts::theme(ui);
    let sp = spatial(ui);
    let expanded = cell.is_expanded(index, &state.collapsed_cells);
    let bg = if highlighted {
        theme.palette.surface.surface
    } else {
        theme.palette.surface.panel
    };
    let border_color = if state.focused_cell == Some(index) {
        theme.palette.accent.primary
    } else if let Some(diag_color) = diagnostic_border_color(index, state, theme) {
        diag_color
    } else {
        theme.palette.border.subtle
    };
    let cell_diags = cell_analyzer_diagnostics(index, state);

    cell_card_frame(bg, border_color)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                cell_header_frame(theme, expanded, &sp).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);

                        let toggle = if expanded {
                            egui_phosphor::regular::CARET_DOWN
                        } else {
                            egui_phosphor::regular::CARET_RIGHT
                        };
                        if header_btn(ui, toggle, if expanded { "Collapse" } else { "Expand" }) {
                            toggle_cell_expansion(cell, index, expanded, state);
                        }

                        render_header_middle(ui, cell, state, source_changed, theme, &sp);

                        render_line_range_badge(ui, start_line, end_line, theme);

                        render_cell_header_actions(ui, index, item_type_label, state, &sp);
                    });
                });

                if expanded {
                    cell_body_frame(bg, &sp).show(ui, |ui| {
                        render_cell_body(
                            ui,
                            index,
                            cell.body_mut(),
                            body_start_line,
                            state,
                            source_changed,
                            theme,
                            style,
                            &cell_diags,
                        );
                    });
                }
            });
        })
        .response
}

// ── Code cell ────────────────────────────────────────────────────────────
