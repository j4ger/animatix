use egui::{Align, Color32, RichText, Stroke, Vec2};

use crate::app::icons::{actor_icon, actor_palette};
use animatix::timeline::ActorCategory;
use crate::app::panels::UiActions;
use crate::app::theme::*;
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn toolbar_ui(&mut self, ui: &mut egui::Ui, actions: &mut UiActions) {
        let toolbar_bg = Color32::from_rgb(12, 14, 18);
        let border_color = Color32::from_rgb(32, 36, 44);
        let text_primary = Color32::from_rgb(228, 232, 243);
        let text_secondary = Color32::from_rgb(150, 158, 175);
        let text_muted = Color32::from_rgb(90, 96, 110);

        let frame_response = egui::Frame::new()
            .fill(toolbar_bg)
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                    // App mark
                    let (mark_rect, _response) =
                        ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(mark_rect, 3.0, Color32::from_rgb(84, 110, 255));

                    ui.add(
                        egui::Label::new(RichText::new("Animatix").size(12.0).color(text_muted))
                            .selectable(false),
                    );

                    ui.add(
                        egui::Label::new(RichText::new("·").size(12.0).color(text_muted))
                            .selectable(false),
                    );

                    // Filename
                    let filename = self
                        .document
                        .file_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled");

                    let filename_text = if self.document.is_dirty {
                        format!("{} ·", filename)
                    } else {
                        filename.to_string()
                    };
                    let filename_color = if self.document.is_dirty {
                        Color32::from_rgb(255, 196, 92)
                    } else {
                        text_primary
                    };

                    ui.add(
                        egui::Label::new(
                            RichText::new(filename_text).size(12.0).color(filename_color),
                        )
                        .selectable(false),
                    );

                    // Right-aligned icon buttons
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                        let icon_btn = |ui: &mut egui::Ui, icon: &str, tooltip: &str| -> bool {
                            let size = Vec2::new(28.0, 28.0);
                            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(32, 36, 44));
                            }
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                icon,
                                egui::TextStyle::Body.resolve(ui.style()),
                                if response.hovered() { text_primary } else { text_secondary },
                            );
                            response.on_hover_text(tooltip).clicked()
                        };

                        // Add actor palette
                        ui.menu_button(egui_phosphor::regular::PLUS, |ui| {
                            ui.set_min_width(180.0);
                            ui.label(RichText::new("Add Actor").size(11.0).color(text_muted));
                            ui.separator();

                            let palette = actor_palette();
                            let mut last_category: Option<ActorCategory> = None;
                            for meta in palette.iter().filter(|m| !m.advanced) {
                                if last_category != Some(meta.category) {
                                    if last_category.is_some() {
                                        ui.separator();
                                    }
                                    ui.label(
                                        RichText::new(meta.category.label())
                                            .size(10.0)
                                            .color(text_muted),
                                    );
                                    last_category = Some(meta.category);
                                }

                                let icon_meta = actor_icon(meta.kind);
                                let ty = meta.type_name;
                                let response = ui.button(
                                    RichText::new(format!("{}  {}", icon_meta.icon, icon_meta.label))
                                        .size(12.0)
                                        .color(text_secondary),
                                );
                                if response.clicked() {
                                    let label = self.unique_label(ty);
                                    let pos = [
                                        self.document.scene_dimensions.width as f32 / 2.0,
                                        self.document.scene_dimensions.height as f32 / 2.0,
                                    ];
                                    actions.create_actor = Some((ty.into(), label, pos));
                                    ui.close_menu();
                                }
                            }

                            // Advanced submenu
                            let advanced: Vec<_> = palette.iter().filter(|m| m.advanced).collect();
                            if !advanced.is_empty() {
                                ui.separator();
                                ui.menu_button(
                                    RichText::new("More shapes…").size(12.0).color(text_secondary),
                                    |ui| {
                                        let mut last_cat: Option<ActorCategory> = None;
                                        for meta in advanced {
                                            if last_cat != Some(meta.category) {
                                                if last_cat.is_some() {
                                                    ui.separator();
                                                }
                                                ui.label(
                                                    RichText::new(meta.category.label())
                                                        .size(10.0)
                                                        .color(text_muted),
                                                );
                                                last_cat = Some(meta.category);
                                            }
                                            let icon_meta = actor_icon(meta.kind);
                                            let ty = meta.type_name;
                                            if ui
                                                .button(
                                                    RichText::new(format!(
                                                        "{}  {}",
                                                        icon_meta.icon, icon_meta.label
                                                    ))
                                                    .size(12.0)
                                                    .color(text_secondary),
                                                )
                                                .clicked()
                                            {
                                                let label = self.unique_label(ty);
                                                let pos = [
                                                    self.document.scene_dimensions.width as f32
                                                        / 2.0,
                                                    self.document.scene_dimensions.height as f32
                                                        / 2.0,
                                                ];
                                                actions.create_actor =
                                                    Some((ty.into(), label, pos));
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );
                            }
                        });

                        if icon_btn(ui, egui_phosphor::regular::GEAR, "Settings") {
                            self.settings_open = true;
                        }
                        if icon_btn(ui, egui_phosphor::regular::SIDEBAR_SIMPLE, "Inspector (⌘I)") {
                            actions.show_inspector = true;
                        }
                        if icon_btn(ui, egui_phosphor::regular::ARROWS_CLOCKWISE, "Rebuild") {
                            actions.rebuild = true;
                        }
                        if icon_btn(ui, egui_phosphor::regular::FLOPPY_DISK, "Save (⌘S)") {
                            actions.save = true;
                        }
                    });
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
