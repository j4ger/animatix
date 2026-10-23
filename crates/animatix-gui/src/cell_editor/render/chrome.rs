use super::*;

pub(super) fn header_btn(ui: &mut egui::Ui, icon: &'static str, tooltip: &'static str) -> bool {
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
        theme.palette.surface.active
    } else {
        lerp_color(Color32::TRANSPARENT, theme.palette.surface.hover, t)
    };

    let icon_color = if response.is_pointer_button_down_on() {
        theme.palette.text.primary
    } else {
        lerp_color(theme.palette.text.muted, theme.palette.text.primary, t)
    };

    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, RADIUS_S, bg);
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
        text_tooltip(ui, response.id.with("tooltip"), &response, tooltip);
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
pub(super) fn apply_pending_cursor(ui: &mut egui::Ui, index: usize, state: &mut CellEditorState) {
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

// ── Shared cell body with gutter line numbers ────────────────────────────
