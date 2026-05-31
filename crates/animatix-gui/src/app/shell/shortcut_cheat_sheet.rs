use egui::{Pos2, Rect, RichText, Stroke, Vec2};

use crate::app::design_tokens::*;
use crate::app::GuiShell;

/// Keyboard shortcut groups.
const SHORTCUT_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "Playback",
        &[
            ("Space", "Play / Pause"),
            (", / .", "Prev / Next keyframe"),
            ("T + drag", "Time lens scrub"),
        ],
    ),
    (
        "Tools",
        &[
            ("V", "Select"),
            ("M / G", "Move"),
            ("S", "Scale"),
            ("R", "Rotate"),
            ("E", "Vertex edit"),
        ],
    ),
    (
        "Canvas",
        &[
            ("Drag body", "Move actor"),
            ("Drag handle", "Scale / Rotate"),
            ("Shift + drag", "Constrain / Snap"),
            ("Alt + drag", "Duplicate"),
            ("Middle-drag", "Pan canvas"),
            ("Scroll", "Zoom"),
        ],
    ),
    (
        "Timeline",
        &[
            ("Click track", "Scrub to time"),
            ("Drag track", "Scrub"),
            ("A", "Action palette"),
        ],
    ),
    (
        "General",
        &[
            ("⌘S", "Save"),
            ("⌘C / ⌘V", "Copy / Paste actors"),
            ("⌘D", "Duplicate selected"),
            ("Delete", "Delete selected"),
            ("Esc", "Deselect / Close"),
            ("⌘K", "Command palette"),
        ],
    ),
];

impl GuiShell {
    pub(crate) fn shortcut_cheat_sheet_ui(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.ctx().viewport_rect();

        // Backdrop
        ui.painter().rect_filled(screen_rect, 0.0, overlay_backdrop());

        let backdrop_response = ui.interact(
            screen_rect,
            ui.id().with("shortcuts_backdrop"),
            egui::Sense::click(),
        );
        if backdrop_response.clicked() {
            self.ui_store.view.shortcuts_open = false;
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ui_store.view.shortcuts_open = false;
        }

        // Centered panel
        let panel_w = 480.0;
        let panel_h = 520.0;
        let panel_pos = Pos2::new(
            (screen_rect.width() - panel_w) / 2.0 + screen_rect.min.x,
            (screen_rect.height() - panel_h) / 2.0 + screen_rect.min.y,
        );
        let panel_rect = Rect::from_min_size(panel_pos, Vec2::new(panel_w, panel_h));

        ui.painter().rect_filled(panel_rect, RADIUS_XL as u8, BG_BASE);
        ui.painter().rect_stroke(
            panel_rect,
            RADIUS_XL as u8,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Outside,
        );

        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(panel_rect));
        content.set_clip_rect(panel_rect);
        content.add_space(SPACE_L);

        // Title
        content.horizontal(|ui| {
            ui.label(
                RichText::new("Keyboard Shortcuts")
                    .size(FONT_SIZE_XL)
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::X)
                    .on_hover_text("Close (Esc)")
                    .clicked()
                {
                    self.ui_store.view.shortcuts_open = false;
                }
            });
        });
        content.add_space(SPACE_L);

        // Two-column layout
        let col_w = (panel_w - SPACE_L * 3.0) / 2.0;
        content.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_L, 0.0);

            let left_groups = &SHORTCUT_GROUPS[..SHORTCUT_GROUPS.len() / 2 + 1];
            let right_groups = &SHORTCUT_GROUPS[SHORTCUT_GROUPS.len() / 2 + 1..];

            shortcut_column(ui, left_groups, col_w);
            shortcut_column(ui, right_groups, col_w);
        });
    }
}

fn shortcut_column(ui: &mut egui::Ui, groups: &[(&str, &[(&str, &str)])], width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        for (title, shortcuts) in groups {
            ui.label(
                RichText::new(*title)
                    .size(FONT_SIZE_S)
                    .color(ACCENT_BLUE)
                    .strong(),
            );
            ui.add_space(SPACE_XS);

            for (key, desc) in *shortcuts {
                ui.horizontal(|ui| {
                    ui.set_width(width);
                    ui.label(
                        RichText::new(*key)
                            .monospace()
                            .size(FONT_SIZE_S)
                            .color(TEXT_SECONDARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(*desc)
                                .size(FONT_SIZE_S)
                                .color(TEXT_PRIMARY),
                        );
                    });
                });
            }
            ui.add_space(SPACE_M);
        }
    });
}
