use egui::{Color32, Id, Rect, Response, Sense, Vec2};

use crate::app::design_tokens::semantic::{accent, surface, text};
use crate::app::design_tokens::spatial::{ROW_M, SPACE_L, SPACE_S};
use crate::app::design_tokens::spatial::component::ICON_SLOT_WIDTH;
use crate::app::design_tokens::typography::{TextRole};

/// Response from a `Row`.
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

    pub fn show(self, ui: &mut egui::Ui, row_id: Id) -> RowResponse {
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, self.height), self.sense);

        let bg = if self.is_selected {
            surface::WIDGET
        } else if row_response.hovered() {
            surface::HOVER
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, 0.0, bg);
        }

        if self.is_selected {
            let accent = Rect::from_min_size(row_rect.min, Vec2::new(2.0, row_rect.height()));
            ui.painter().rect_filled(accent, 0.0, accent::PRIMARY);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + SPACE_S + self.indent;

        let chevron_rect = Rect::from_min_size(
            egui::pos2(cursor_x, row_rect.min.y),
            Vec2::new(ICON_SLOT_WIDTH, self.height),
        );
        let chevron_response =
            ui.interact(chevron_rect, row_id.with("chevron"), Sense::click());

        if self.has_children {
            let icon = if self.is_expanded {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            };
            let color = if chevron_response.hovered() {
                text::SECONDARY
            } else {
                text::MUTED
            };
            ui.painter().text(
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
                egui::pos2(cursor_x, row_rect.min.y),
                Vec2::new(ICON_SLOT_WIDTH, self.height),
            );
            let default_color = if self.is_selected { text::PRIMARY } else { text::MUTED };
            ui.painter().text(
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
                text::PRIMARY
            } else {
                text::SECONDARY
            }
        });
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            self.label,
            TextRole::BodyS.font_id(),
            label_color,
        );

        if let Some(right) = self.right {
            let right_x = row_rect.max.x - SPACE_S;
            let right_rect = Rect::from_min_size(
                egui::pos2(cursor_x + SPACE_L, row_rect.min.y),
                Vec2::new((right_x - cursor_x - SPACE_L).max(20.0), self.height),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(right_rect), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
            });
        }

        RowResponse {
            row_clicked: row_response.clicked() && !chevron_response.clicked(),
            chevron_clicked: chevron_response.clicked(),
            drag_started: row_response.drag_started(),
            hovered: row_response.hovered(),
            row_rect,
            response: row_response,
        }
    }
}
