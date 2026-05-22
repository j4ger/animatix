use egui::{RichText, Stroke};

use crate::app::components;
use crate::app::theme::*;

use crate::app::GuiShell;

const SETTINGS_INPUT_WIDTH: f32 = 120.0;

impl GuiShell {
    pub(crate) fn settings_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(
            screen_rect,
            0.0,
            overlay_backdrop(),
        );

        // Capture clicks on backdrop to close
        let backdrop_response = ui.interact(
            screen_rect,
            ui.id().with("settings_backdrop"),
            egui::Sense::click(),
        );
        if backdrop_response.clicked() {
            self.settings_open = false;
        }

        // Close on Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        // Centered dialog using egui window for proper layout
        let window_response = egui::Window::new("Settings")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([420.0, 520.0])
            .min_size([380.0, 400.0])
            .max_size([600.0, 700.0])
            .resizable(true)
            .collapsible(false)
            .title_bar(false)
            .frame(
                egui::Frame::new()
                    .fill(BG_BASE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(RADIUS_XL)
                    .inner_margin(egui::Margin::same(SPACE_XL as i8)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_min_width(360.0);

                // Title row
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Settings")
                            .size(FONT_SIZE_XL)
                            .color(TEXT_PRIMARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_resp = ui.button(egui_phosphor::regular::X);
                        if close_resp.clicked() {
                            self.settings_open = false;
                        }
                    });
                });
                ui.add_space(SPACE_M);
                ui.separator();
                ui.add_space(SPACE_M);

                // ── Preview ──
                components::section_header(ui, egui_phosphor::regular::GRID_FOUR, "Preview", None);
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Grid size").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.grid_size)
                                .speed(1.0)
                                .range(1.0..=200.0)
                                .suffix(" px"),
                        );
                    },
                );
                ui.add_space(SPACE_M);

                // ── Input ──
                components::section_header(ui, egui_phosphor::regular::CURSOR_CLICK, "Input", None);
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Nudge step").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.nudge_step_px)
                                .speed(0.5)
                                .range(0.1..=50.0)
                                .suffix(" px"),
                        );
                    },
                );
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Nudge step (Shift)").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.nudge_step_shift_px)
                                .speed(0.5)
                                .range(1.0..=200.0)
                                .suffix(" px"),
                        );
                    },
                );
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Rotation snap").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.rotation_snap_degrees)
                                .speed(1.0)
                                .range(1.0..=90.0)
                                .suffix("°"),
                        );
                    },
                );
                ui.add_space(SPACE_M);

                // ── Playback ──
                components::section_header(ui, egui_phosphor::regular::PLAY, "Playback", None);
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Scrub step").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.scrub_step_s)
                                .speed(0.01)
                                .range(0.01..=1.0)
                                .suffix(" s"),
                        );
                    },
                );
                ui.add_space(SPACE_M);

                // ── Editor ──
                components::section_header(ui, egui_phosphor::regular::PENCIL, "Editor", None);
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Rebuild debounce").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.rebuild_debounce_ms)
                                .speed(10.0)
                                .range(0..=1000)
                                .suffix(" ms"),
                        );
                    },
                );
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Undo limit").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.undo_limit)
                                .speed(10.0)
                                .range(10..=1000)
                                .suffix(" entries"),
                        );
                    },
                );
                ui.add_space(SPACE_S);

                components::labeled_row(
                    ui,
                    RichText::new("Keyframe merge window").size(FONT_SIZE_S).color(TEXT_SECONDARY),
                    SETTINGS_INPUT_WIDTH,
                    |ui| {
                        let mut value_ms = (self.keyframe_merge_window_s * 1000.0) as f32;
                        ui.add(
                            egui::DragValue::new(&mut value_ms)
                                .speed(1.0)
                                .range(0.0..=500.0)
                                .suffix(" ms"),
                        );
                        self.keyframe_merge_window_s = (value_ms as f64 / 1000.0).max(0.0);
                    },
                );
                ui.add_space(SPACE_S);

                let mut debug = self.debug_bounds;
                ui.checkbox(
                    &mut debug,
                    RichText::new("Draw debug bounding boxes")
                        .size(FONT_SIZE_S)
                        .color(TEXT_SECONDARY),
                );
                self.debug_bounds = debug;
            });

        if window_response.is_none() {
            // Window was closed via egui chrome
            self.settings_open = false;
        }
    }
}
