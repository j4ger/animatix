use egui::{Color32, Id, Rect, Response, Sense, Vec2};

use crate::tokens::spatial::component::ICON_SLOT_WIDTH;
use crate::tokens::spatial::{ROW_M, SPACE_L, SPACE_S};
use crate::tokens::typography::TextRole;

/// Response from a `Row`.
// eparts principle 3: default arrow cursor for buttons/rows, pointer only for links.
// The Row's clickable sense would otherwise show a PointingHand on hover — overridden below.
pub struct RowResponse {
    pub row_clicked: bool,
    pub chevron_clicked: bool,
    pub drag_started: bool,
    pub hovered: bool,
    pub row_rect: Rect,
    /// The underlying egui response for the whole row.
    /// Use this to attach context menus without creating a second interactable.
    pub response: Response,
}

/// A full-width interactive row used in sidebars, property lists, and keyframe groups.
pub struct Row<'a> {
    pub height: f32,
    pub indent: f32,
    pub has_children: bool,
    pub is_expanded: bool,
    pub is_selected: bool,
    pub secondary_selected: bool,
    pub confirmed: bool,
    pub icon: Option<&'static str>,
    pub label: &'a str,
    pub label_color: Option<Color32>,
    #[allow(clippy::type_complexity)]
    pub right: Option<Box<dyn FnOnce(&mut egui::Ui) + 'a>>,
    pub sense: egui::Sense,
}

impl<'a> Row<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            height: ROW_M,
            indent: 0.0,
            has_children: false,
            is_expanded: false,
            is_selected: false,
            secondary_selected: false,
            confirmed: false,
            icon: None,
            label,
            label_color: None,
            right: None,
            sense: egui::Sense::click(),
        }
    }

    pub fn sense(mut self, sense: egui::Sense) -> Self {
        self.sense = sense;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn indent(mut self, px: f32) -> Self {
        self.indent = px;
        self
    }

    pub fn expanded(mut self, yes: bool) -> Self {
        self.is_expanded = yes;
        self
    }

    pub fn selected(mut self, yes: bool) -> Self {
        self.is_selected = yes;
        self
    }

    pub fn secondary_selected(mut self, yes: bool) -> Self {
        self.secondary_selected = yes;
        self
    }

    pub fn confirmed(mut self, yes: bool) -> Self {
        self.confirmed = yes;
        self
    }

    pub fn icon(mut self, icon: Option<&'static str>) -> Self {
        self.icon = icon;
        self
    }

    pub fn has_children(mut self, yes: bool) -> Self {
        self.has_children = yes;
        self
    }

    pub fn label_color(mut self, c: Color32) -> Self {
        self.label_color = Some(c);
        self
    }

    pub fn right<F: FnOnce(&mut egui::Ui) + 'a>(mut self, f: F) -> Self {
        self.right = Some(Box::new(f));
        self
    }

    // ── Primary entry point (preserves existing behaviour) ──────────

    /// Standard render: allocate a full-width rect from the ui, paint, and return.
    pub fn show(self, ui: &mut egui::Ui, row_id: Id) -> RowResponse {
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, self.height), self.sense);
        let painter = ui.painter_at(row_rect);
        self.show_in_rect(ui, row_rect, row_response, row_id, &painter)
    }

    // ── Rect-mode entry point (for Tree / List) ─────────────────────

    /// Render into a pre-allocated `rect` using `ui` for interaction.
    ///
    /// A scoped child Ui is built with `max_rect = rect` so the `right` slot
    /// and any sub-widgets are confined to the row bounds without consuming
    /// the parent layout cursor.
    pub fn show_in_rect(
        self,
        ui: &mut egui::Ui,
        rect: Rect,
        row_response: Response,
        row_id: Id,
        painter: &egui::Painter,
    ) -> RowResponse {
        let t = crate::tokens::theme::theme(ui);
        let row_clicked = row_response.clicked();
        let hovered = row_response.hovered();

        let bg = if self.is_selected {
            t.surface.widget
        } else if self.secondary_selected {
            t.accent.faint
        } else if hovered {
            t.surface.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            painter.rect_filled(rect, 0.0, bg);
        }

        if self.is_selected {
            let accent = Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height()));
            painter.rect_filled(accent, 0.0, t.accent.primary);
        } else if self.secondary_selected {
            let accent = Rect::from_min_size(rect.min, Vec2::new(2.0, rect.height()));
            painter.rect_filled(accent, 0.0, t.accent.faint);
        }

        let baseline_y = rect.center().y;
        let mut cursor_x = rect.min.x + SPACE_S + self.indent;

        let chevron_rect = Rect::from_min_size(
            egui::pos2(cursor_x, rect.min.y),
            Vec2::new(ICON_SLOT_WIDTH, self.height),
        );
        let chevron_resp = ui.interact(chevron_rect, row_id.with("chevron"), Sense::click());

        if self.has_children {
            let icon = if self.is_expanded {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            };
            let color = if chevron_resp.hovered() {
                t.text.secondary
            } else {
                t.text.muted
            };
            painter.text(
                egui::pos2(chevron_rect.center().x, baseline_y),
                egui::Align2::CENTER_CENTER,
                icon,
                TextRole::Body.font_id(),
                color,
            );
        }
        cursor_x += ICON_SLOT_WIDTH;

        if let Some(icon_str) = self.icon {
            cursor_x += SPACE_S;
            let icon_rect = Rect::from_min_size(
                egui::pos2(cursor_x, rect.min.y),
                Vec2::new(ICON_SLOT_WIDTH, self.height),
            );
            let default_color = if self.is_selected {
                t.text.primary
            } else {
                t.text.muted
            };
            painter.text(
                egui::pos2(icon_rect.center().x, baseline_y),
                egui::Align2::CENTER_CENTER,
                icon_str,
                TextRole::Body.font_id(),
                self.label_color.unwrap_or(default_color),
            );
            cursor_x += ICON_SLOT_WIDTH + SPACE_S;
        } else {
            cursor_x += SPACE_S * 2.0;
        }

        let label_color = self.label_color.unwrap_or({
            if self.is_selected {
                t.text.primary
            } else {
                t.text.secondary
            }
        });
        painter.text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            self.label,
            TextRole::BodyS.font_id(),
            label_color,
        );

        // right slot — render inside a scoped child confined to rect
        if let Some(right_fn) = self.right {
            let check_width = if self.confirmed { ICON_SLOT_WIDTH + SPACE_S } else { 0.0 };
            let right_area_width = (rect.max.x - SPACE_S - check_width - cursor_x - SPACE_L).max(20.0);
            let right_rect = Rect::from_min_size(
                egui::pos2(cursor_x + SPACE_L, rect.min.y),
                Vec2::new(right_area_width, self.height),
            );
            let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(right_rect));
            child_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right_fn);
        }

        if self.confirmed {
            let check_width = ICON_SLOT_WIDTH + SPACE_S;
            let check_x = rect.max.x - SPACE_S - check_width / 2.0;
            painter.text(
                egui::pos2(check_x, baseline_y),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::CHECK,
                TextRole::Body.font_id(),
                if self.is_selected { t.text.on_accent } else { t.accent.primary },
            );
        }

        RowResponse {
            row_clicked: row_clicked && !chevron_resp.clicked(),
            chevron_clicked: chevron_resp.clicked(),
            drag_started: row_response.drag_started(),
            hovered,
            row_rect: rect,
            // Principle 3: override egui's default PointingHand with Default arrow.
            response: row_response.on_hover_cursor(egui::CursorIcon::Default),
        }
    }
}

impl crate::Selectable for Row<'_> {
    fn selected(mut self, yes: bool) -> Self {
        self.is_selected = yes;
        self
    }

    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::traits::Selectable;

    #[test]
    fn builder_secondary_selected_and_confirmed() {
        let row = Row::new("test")
            .secondary_selected(true)
            .confirmed(true);
        assert!(row.secondary_selected);
        assert!(row.confirmed);
    }

    #[test]
    fn selectable_trait_for_row() {
        let mut row = Row::new("test");
        assert!(!row.is_selected());
        row = row.selected(true);
        assert!(row.is_selected());
    }
}
