use egui::{Align, RichText, Stroke, Vec2};

use crate::app::components;
use crate::app::design_tokens::*;
use crate::app::commands::{Command, CommandQueue};
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn toolbar_ui(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut CommandQueue,
    ) {
        let toolbar_bg = BG_BASE;
        let border_color = BG_WIDGET;
        let text_primary = TEXT_PRIMARY;
        let text_muted = TEXT_MUTED;

        let frame_response = egui::Frame::new()
            .fill(toolbar_bg)
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(28.0);

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    // App mark
                    let (mark_rect, _response) =
                        ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(mark_rect, 2.0, ACCENT_BLUE);

                    // Filename with dirty indicator
                    let filename = self
                        .document_store
                        .document
                        .file_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled");

                    let filename_text = if self.document_store.document.is_dirty {
                        format!("{}*", filename)
                    } else {
                        filename.to_string()
                    };
                    let filename_color = if self.document_store.document.is_dirty {
                        AMBER
                    } else {
                        text_primary
                    };

                    ui.add(
                        egui::Label::new(
                            RichText::new(filename_text)
                                .size(FONT_SIZE_M)
                                .color(filename_color),
                        )
                        .selectable(false),
                    );

                    // Filename dropdown
                    ui.menu_button("▾", |ui| {
                        if ui.button("Save").clicked() {
                            commands.push_back(Command::Save);
                            ui.close();
                        }
                        if ui.button("Export…").clicked() {
                            commands.push_back(Command::OpenExportDialog);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Rebuild").clicked() {
                            commands.push_back(Command::Rebuild);
                            ui.close();
                        }
                    });

                    // Breadcrumb for multi-scene compositions
                    if self.document_store.document.is_composition() {
                        let scene_names = self.document_store.document.scene_names();
                        if scene_names.len() >= 2 {
                            let active_scene = self.document_store.document.active_scene.as_deref();
                            // Left-align the breadcrumb with some spacing from the filename
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                                for (i, name) in scene_names.iter().enumerate() {
                                    if i > 0 {
                                        ui.label(
                                            RichText::new("→")
                                                .size(FONT_SIZE_S)
                                                .color(TEXT_MUTED),
                                        );
                                    }
                                    let is_active = active_scene == Some(name.as_str());
                                    let color = if is_active { TEXT_PRIMARY } else { TEXT_MUTED };
                                    let label = RichText::new(name.as_str())
                                        .size(FONT_SIZE_S)
                                        .color(color)
                                        .strong();
                                    let btn = egui::Button::new(label)
                                        .frame(false)
                                        .sense(egui::Sense::click());
                                    if ui.add(btn).clicked() {
                                        commands.push_back(Command::SelectScene(name.clone()));
                                    }
                                }
                            });
                        }
                    }

                    // Right-aligned: play + settings + command palette
                    ui.with_layout(
                        egui::Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                            // Command palette button
                            let shortcut =
                                if cfg!(target_os = "macos") { "⌘K" } else { "Ctrl+K" };
                            if components::icon_button(
                                ui,
                                egui_phosphor::regular::COMMAND,
                                &format!("Command palette ({shortcut})"),
                            )
                            .clicked()
                            {
                                // TODO: open command palette
                            }

                            if components::icon_button(
                                ui,
                                egui_phosphor::regular::GEAR,
                                "Settings",
                            )
                            .clicked()
                            {
                                self.ui_store.settings_open = true;
                            }

                            // Play / Pause
                            let is_playing = self.preview_store.preview.is_playing;
                            let play_icon = if is_playing {
                                egui_phosphor::regular::PAUSE
                            } else {
                                egui_phosphor::regular::PLAY
                            };
                            if components::icon_button(
                                ui,
                                play_icon,
                                "Play/Pause (Space)",
                            )
                            .clicked()
                            {
                                commands.push_back(Command::TogglePlayback);
                            }
                        },
                    );
                });
            });

        // Subtle bottom hairline
        let toolbar_rect = frame_response.response.rect;
        ui.painter().line_segment(
            [
                egui::pos2(toolbar_rect.left(), toolbar_rect.bottom() - 1.0),
                egui::pos2(toolbar_rect.right(), toolbar_rect.bottom() - 1.0),
            ],
            Stroke::new(1.0, border_color),
        );
    }
}
