use egui::{Align, RichText, Stroke, Vec2};

use crate::app::components;
use crate::app::icons::{actor_icon, actor_palette};
use crate::app::theme::*;
use animatix::timeline::ActorCategory;
use crate::app::panels::UiActions;
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn toolbar_ui(&mut self, ui: &mut egui::Ui, actions: &mut UiActions) {
        let toolbar_bg = BG_BASE;
        let border_color = BG_WIDGET;
        let text_primary = TEXT_PRIMARY;
        let text_secondary = TEXT_SECONDARY;
        let text_muted = TEXT_MUTED;

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
                    ui.painter().rect_filled(mark_rect, 3.0, ACCENT_BLUE);

                    ui.add(
                        egui::Label::new(RichText::new("Animatix").size(FONT_SIZE_L).color(text_muted))
                            .selectable(false),
                    );

                    ui.add(
                        egui::Label::new(RichText::new("·").size(FONT_SIZE_L).color(text_muted))
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
                        AMBER
                    } else {
                        text_primary
                    };

                    ui.add(
                        egui::Label::new(
                            RichText::new(filename_text).size(FONT_SIZE_L).color(filename_color),
                        )
                        .selectable(false),
                    );

                    // Right-aligned icon buttons
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                        if components::icon_button(ui, egui_phosphor::regular::GEAR, "Settings").clicked() {
                            self.settings_open = true;
                        }
                        if components::icon_button(ui, egui_phosphor::regular::EXPORT, "Export").clicked() {
                            actions.open_export_dialog = true;
                        }
                        if components::icon_button(ui, egui_phosphor::regular::SIDEBAR_SIMPLE, "Inspector (⌘I)").clicked() {
                            actions.show_inspector = true;
                        }
                        if components::icon_button(ui, egui_phosphor::regular::ARROWS_CLOCKWISE, "Rebuild").clicked() {
                            actions.rebuild = true;
                        }
                        if components::icon_button(ui, egui_phosphor::regular::FLOPPY_DISK, "Save (⌘S)").clicked() {
                            actions.save = true;
                        }

                        // Add actor palette
                        ui.menu_button(egui_phosphor::regular::PLUS, |ui| {
                            ui.set_min_width(180.0);
                            ui.label(RichText::new("Add Actor").size(FONT_SIZE_M).color(text_muted));
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
                                            .size(FONT_SIZE_S)
                                            .color(text_muted),
                                    );
                                    last_category = Some(meta.category);
                                }

                                let icon_meta = actor_icon(meta.kind);
                                let ty = meta.type_name;
                                let response = ui.button(
                                    RichText::new(format!("{}  {}", icon_meta.icon, icon_meta.label))
                                        .size(FONT_SIZE_L)
                                        .color(text_secondary),
                                );
                                if response.clicked() {
                                    let label = self.unique_label(ty);
                                    let pos = [
                                        self.document.scene_dimensions.width as f32 / 2.0,
                                        self.document.scene_dimensions.height as f32 / 2.0,
                                    ];
                                    actions.create_actor = Some((ty.into(), label, pos));
                                    ui.close();
                                }
                            }

                            // Advanced submenu
                            let advanced: Vec<_> = palette.iter().filter(|m| m.advanced).collect();
                            if !advanced.is_empty() {
                                ui.separator();
                                ui.menu_button(
                                    RichText::new("More shapes…").size(FONT_SIZE_L).color(text_secondary),
                                    |ui| {
                                        let mut last_cat: Option<ActorCategory> = None;
                                        for meta in advanced {
                                            if last_cat != Some(meta.category) {
                                                if last_cat.is_some() {
                                                    ui.separator();
                                                }
                                                ui.label(
                                                    RichText::new(meta.category.label())
                                                        .size(FONT_SIZE_S)
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
                                                    .size(FONT_SIZE_L)
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
                                                ui.close();
                                            }
                                        }
                                    },
                                );
                            }
                        });
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
