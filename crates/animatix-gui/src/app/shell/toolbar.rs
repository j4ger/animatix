use egui::{Align, RichText, Stroke, Vec2};

use crate::app::GuiShell;
use crate::app::commands::{
    ActionQueue, Command, DocumentCommand, SceneCommand, ShellAction, ViewAction,
};
use crate::app::components::button::Button;
use crate::app::design_tokens::semantic::accent::PRIMARY as ACCENT_BLUE;
use crate::app::design_tokens::semantic::status::DIAGNOSTIC_ERROR as DIAGNOSTIC_RED;
use crate::app::design_tokens::semantic::status::WARNING as AMBER;
use crate::app::design_tokens::semantic::surface::BASE as BG_BASE;
use crate::app::design_tokens::semantic::surface::WIDGET as BG_WIDGET;
use crate::app::design_tokens::semantic::text::MUTED as TEXT_MUTED;
use crate::app::design_tokens::semantic::text::PRIMARY as TEXT_PRIMARY;
use crate::app::design_tokens::semantic::text::SECONDARY as TEXT_SECONDARY;
use crate::app::design_tokens::spatial::toolbar::HEIGHT as TOOLBAR_HEIGHT;
use crate::app::design_tokens::spatial::{RADIUS_M, SPACE_L, SPACE_S, SPACE_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;

// TOOLBAR_HEIGHT imported via design_tokens::*

impl GuiShell {
    pub(crate) fn toolbar_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let toolbar_bg = BG_BASE;
        let border_color = BG_WIDGET;
        let text_primary = TEXT_PRIMARY;

        let frame_response = egui::Frame::new()
            .fill(toolbar_bg)
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(TOOLBAR_HEIGHT);

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(SPACE_L, 0.0);

                    // App mark
                    let (mark_rect, _response) =
                        ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(mark_rect, 2.0, ACCENT_BLUE);

                    // Filename with dirty indicator
                    let filename = self
                        .document_store
                        .source
                        .document
                        .file_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled");

                    let filename_text = if self.document_store.source.document.is_dirty {
                        format!("{}*", filename)
                    } else {
                        filename.to_string()
                    };
                    let filename_color = if self.document_store.source.document.is_dirty {
                        AMBER
                    } else {
                        text_primary
                    };

                    ui.add(
                        egui::Label::new(
                            RichText::new(filename_text)
                                .size(TextRole::Body.size())
                                .color(filename_color),
                        )
                        .selectable(false),
                    );

                    // Status badge: last-good or stale
                    if self.document_store.showing_last_good() {
                        ui.add_space(SPACE_S);
                        let response = egui::Frame::new()
                            .fill(DIAGNOSTIC_RED)
                            .corner_radius(RADIUS_M)
                            .inner_margin(egui::Margin::symmetric(4, 1))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("last good")
                                            .size(TextRole::Micro.size())
                                            .color(BG_BASE),
                                    )
                                    .selectable(false),
                                );
                            });
                        response.response.on_hover_text(
                            "Build failed — preview shows the last successful build",
                        );
                    } else if self.document_store.snapshot_is_stale() {
                        ui.add_space(SPACE_S);
                        let response = egui::Frame::new()
                            .fill(AMBER)
                            .corner_radius(RADIUS_M)
                            .inner_margin(egui::Margin::symmetric(4, 1))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new("stale")
                                            .size(TextRole::Micro.size())
                                            .color(BG_BASE),
                                    )
                                    .selectable(false),
                                );
                            });
                        response.response.on_hover_text("Source edited — rebuild pending");
                    }

                    // Building indicator (pulsing)
                    if self.preview_store.rebuild_in_progress {
                        ui.add_space(SPACE_S);
                        let t = ui.ctx().animate_value_with_time(
                            ui.id().with("build_spinner"),
                            0.0,
                            0.8,
                        );
                        let pulse = ((t as f64 * std::f64::consts::TAU).sin() * 0.3 + 0.7) as f32;
                        let response = egui::Frame::new()
                            .fill(ACCENT_BLUE.linear_multiply(pulse))
                            .corner_radius(RADIUS_M)
                            .inner_margin(egui::Margin::symmetric(4, 1))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(egui_phosphor::regular::ARROW_CLOCKWISE)
                                            .size(TextRole::Micro.size())
                                            .color(BG_BASE),
                                    )
                                    .selectable(false),
                                );
                            });
                        response.response.on_hover_text("Building timeline…");
                    }

                    // Filename dropdown
                    ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                        if ui
                            .button(format!("{} Save", egui_phosphor::regular::FLOPPY_DISK))
                            .on_hover_text("Save (Ctrl+S)")
                            .clicked()
                        {
                            commands.push_back(DocumentCommand::Save.into());
                            ui.close();
                        }
                        if ui
                            .button(format!("{} Export…", egui_phosphor::regular::EXPORT))
                            .on_hover_text("Export image, video or GIF")
                            .clicked()
                        {
                            commands.push_back(ShellAction::View(ViewAction::OpenExportDialog));
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button(format!(
                                "{} Reload from disk",
                                egui_phosphor::regular::ARROW_CLOCKWISE
                            ))
                            .on_hover_text("Reload from disk (Ctrl+R)")
                            .clicked()
                        {
                            commands.push_back(DocumentCommand::Reload.into());
                            ui.close();
                        }
                        if ui
                            .button(format!(
                                "{} Rebuild timeline",
                                egui_phosphor::regular::HARD_DRIVES
                            ))
                            .on_hover_text("Rebuild timeline (Ctrl+Shift+R)")
                            .clicked()
                        {
                            commands.push_back(DocumentCommand::Rebuild.into());
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button(format!(
                                "{} Switch workspace…",
                                egui_phosphor::regular::FOLDER_NOTCH
                            ))
                            .on_hover_text("Change workspace directory")
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                            }
                            ui.close();
                        }
                    });

                    // Breadcrumb for multi-scene compositions
                    if self.document_store.source.document.is_composition() {
                        let scene_names = self.document_store.source.document.scene_names();
                        if scene_names.len() >= 2 {
                            let active_scene =
                                self.document_store.source.document.active_scene.as_deref();
                            // Left-align the breadcrumb with some spacing from the filename
                            ui.add_space(SPACE_XL);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);
                                for (i, name) in scene_names.iter().enumerate() {
                                    if i > 0 {
                                        ui.label(
                                            RichText::new(egui_phosphor::regular::ARROW_RIGHT)
                                                .size(TextRole::BodyS.size())
                                                .color(TEXT_MUTED),
                                        );
                                    }
                                    let is_active = active_scene == Some(name.as_str());
                                    let color = if is_active { TEXT_PRIMARY } else { TEXT_MUTED };
                                    let label = RichText::new(name.as_str())
                                        .size(TextRole::BodyS.size())
                                        .color(color)
                                        .strong();
                                    let btn = egui::Button::new(label)
                                        .frame(false)
                                        .sense(egui::Sense::click());
                                    if ui
                                        .add(btn)
                                        .on_hover_text(format!("Switch to scene '{}'", name))
                                        .clicked()
                                    {
                                        commands.push_back(
                                            SceneCommand::SelectScene(name.clone()).into(),
                                        );
                                    }
                                }
                            });
                        }
                    }

                    // ── Center: viewport toggles + zoom cycle ──
                    ui.add_space(SPACE_XL);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

                        // Grid toggle
                        let grid = self.preview_store.preview.overlay.show_grid;
                        if ui.selectable_label(grid, "Grid").on_hover_text("Toggle grid").clicked()
                        {
                            self.preview_store.preview.overlay.show_grid = !grid;
                        }

                        // Guides toggle
                        let guides = self.preview_store.preview.overlay.show_guides;
                        if ui
                            .selectable_label(guides, "Guides")
                            .on_hover_text("Toggle guides")
                            .clicked()
                        {
                            self.preview_store.preview.overlay.show_guides = !guides;
                        }

                        // Labels toggle
                        let labels = self.preview_store.preview.overlay.show_actor_labels;
                        if ui
                            .selectable_label(labels, "Labels")
                            .on_hover_text("Toggle actor labels")
                            .clicked()
                        {
                            self.preview_store.preview.overlay.show_actor_labels = !labels;
                        }

                        // Debug dropdown (grouped debug toggles)
                        ui.menu_button(
                            RichText::new("Debug")
                                .size(TextRole::BodyS.size())
                                .color(TEXT_SECONDARY),
                            |ui| {
                                let mut bounds = self.ui_store.view.debug_bounds;
                                if ui.checkbox(&mut bounds, "Bounds").clicked() {
                                    self.ui_store.view.debug_bounds = bounds;
                                }
                                let mut layout_debug = self.ui_store.view.debug_layout;
                                if ui.checkbox(&mut layout_debug, "Layout").clicked() {
                                    self.ui_store.view.debug_layout = layout_debug;
                                }
                                let mut spacing = self.ui_store.view.debug_spacing;
                                if ui.checkbox(&mut spacing, "Spacing").clicked() {
                                    self.ui_store.view.debug_spacing = spacing;
                                }
                                ui.separator();
                                let mut perf =
                                    self.preview_store.preview.overlay.show_performance_hud;
                                if ui.checkbox(&mut perf, "Performance HUD").clicked() {
                                    self.preview_store.preview.overlay.show_performance_hud = perf;
                                }
                            },
                        );

                        ui.separator();

                        // Zoom dropdown
                        let zoom = self.preview_store.preview.viewport.preview_zoom;
                        let zoom_label = if (zoom - 1.0).abs() < 0.05 {
                            "100%"
                        } else if (zoom - 1.5).abs() < 0.05 {
                            "150%"
                        } else if (zoom - 2.0).abs() < 0.05 {
                            "200%"
                        } else {
                            "Fit"
                        };
                        ui.menu_button(
                            RichText::new(zoom_label)
                                .size(TextRole::BodyS.size())
                                .color(TEXT_SECONDARY),
                            |ui| {
                                ui.set_min_width(80.0);
                                if ui.selectable_label(false, "Fit").clicked() {
                                    self.preview_store.preview.fit_zoom_requested = true;
                                    ui.close();
                                }
                                if ui.selectable_label((zoom - 1.0).abs() < 0.05, "100%").clicked()
                                {
                                    self.preview_store.preview.viewport.preview_zoom = 1.0;
                                    self.preview_store.preview.viewport.preview_pan = Vec2::new(
                                        self.preview_store.preview.dimensions.width as f32 / 2.0,
                                        self.preview_store.preview.dimensions.height as f32 / 2.0,
                                    );
                                    ui.close();
                                }
                                if ui.selectable_label((zoom - 1.5).abs() < 0.05, "150%").clicked()
                                {
                                    self.preview_store.preview.viewport.preview_zoom = 1.5;
                                    self.preview_store.preview.viewport.preview_pan = Vec2::new(
                                        self.preview_store.preview.dimensions.width as f32 / 2.0,
                                        self.preview_store.preview.dimensions.height as f32 / 2.0,
                                    );
                                    ui.close();
                                }
                                if ui.selectable_label((zoom - 2.0).abs() < 0.05, "200%").clicked()
                                {
                                    self.preview_store.preview.viewport.preview_zoom = 2.0;
                                    self.preview_store.preview.viewport.preview_pan = Vec2::new(
                                        self.preview_store.preview.dimensions.width as f32 / 2.0,
                                        self.preview_store.preview.dimensions.height as f32 / 2.0,
                                    );
                                    ui.close();
                                }
                            },
                        );
                    });

                    // Right-aligned: inspector + settings + command palette
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

                        // Command palette / shortcut reference button
                        if ui
                            .add(
                                Button::icon(egui_phosphor::regular::COMMAND)
                                    .with_tooltip("Keyboard shortcuts"),
                            )
                            .clicked()
                        {
                            self.ui_store.view.shortcuts_open = true;
                        }

                        if ui
                            .add(
                                Button::icon(egui_phosphor::regular::GEAR).with_tooltip("Settings"),
                            )
                            .clicked()
                        {
                            self.ui_store.view.settings_open = true;
                        }

                        // Diagnostics toggle
                        let diag_active = self.ui_store.view.diagnostics_panel_visible;
                        if ui
                            .add(
                                Button::ghost("")
                                    .with_icon(egui_phosphor::regular::WARNING_OCTAGON)
                                    .with_tooltip("Toggle diagnostics panel")
                                    .active(diag_active),
                            )
                            .clicked()
                        {
                            self.ui_store.view.diagnostics_panel_visible = !diag_active;
                        }

                        // Inspector toggle
                        let inspector_active = self.ui_store.view.inspector_visible;
                        if ui
                            .add(
                                Button::ghost("")
                                    .with_icon(egui_phosphor::regular::SLIDERS)
                                    .with_tooltip("Toggle Inspector")
                                    .active(inspector_active),
                            )
                            .clicked()
                        {
                            commands.push_back(ShellAction::View(ViewAction::ShowInspector));
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
            Stroke::new(STROKE_WIDTH, border_color),
        );
    }
}
