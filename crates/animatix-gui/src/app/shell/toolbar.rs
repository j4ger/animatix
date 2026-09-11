use egui::{Align, RichText, Stroke, Vec2};

use crate::app::GuiShell;
use crate::app::commands::{
    ActionQueue, Command, DocumentCommand, SceneCommand, ShellAction, ViewAction,
};
use crate::app::components::button::Button;
use crate::app::components::{Tag, text_tooltip};
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::typography::TextRole;

// TOOLBAR_HEIGHT imported via design_tokens::*

impl GuiShell {
    pub(crate) fn toolbar_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let t = eparts::theme(ui);
        let toolbar_bg = t.surface.base;
        let border_color = t.surface.widget;
        let text_primary = t.text.primary;

        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let frame_response = egui::Frame::new()
            .fill(toolbar_bg)
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(sp.toolbar.height);

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_4, 0.0);

                    // App mark
                    let (mark_rect, _response) =
                        ui.allocate_exact_size(Vec2::new(8.0, 8.0), egui::Sense::hover());
                    ui.painter().rect_filled(mark_rect, 2.0, t.accent.primary);

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
                        t.status.warning
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
                        let response = ui.add(Tag::new("last good").color(t.status.error));
                        text_tooltip(
                            ui,
                            response.id.with("last_good_tooltip"),
                            &response,
                            "Build failed — preview shows the last successful build",
                        );
                    } else if self.document_store.snapshot_is_stale() {
                        let response = ui.add(Tag::new("stale").color(t.status.warning));
                        text_tooltip(
                            ui,
                            response.id.with("stale_tooltip"),
                            &response,
                            "Source edited — rebuild pending",
                        );
                    }

                    // Building indicator
                    if self.preview_store.rebuild_in_progress {
                        let response = ui.add(
                            Tag::new(egui_phosphor::regular::ARROW_CLOCKWISE)
                                .color(t.accent.primary),
                        );
                        text_tooltip(
                            ui,
                            response.id.with("building_tooltip"),
                            &response,
                            "Building timeline…",
                        );
                    }

                    // Filename dropdown
                    // Shortcut hints pulled from the shared ShortcutRegistry so the
                    // labels stay in sync with the actual keybindings (and are
                    // platform-aware) instead of being hardcoded.
                    let save_tip = crate::app::interaction::keyboard::tooltip_with_shortcut(
                        &self.shortcut_registry,
                        "Save",
                        &crate::app::interaction::keyboard::KeyboardAction::Save,
                        ui.ctx(),
                    );
                    let reload_tip = crate::app::interaction::keyboard::tooltip_with_shortcut(
                        &self.shortcut_registry,
                        "Reload from disk",
                        &crate::app::interaction::keyboard::KeyboardAction::Reload,
                        ui.ctx(),
                    );
                    let rebuild_tip = crate::app::interaction::keyboard::tooltip_with_shortcut(
                        &self.shortcut_registry,
                        "Rebuild timeline",
                        &crate::app::interaction::keyboard::KeyboardAction::Rebuild,
                        ui.ctx(),
                    );
                    ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                        let new_btn =
                            ui.button(format!("{} New scene", egui_phosphor::regular::FILE_PLUS));
                        if new_btn.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::NewFile));
                            ui.close();
                        }
                        let open_btn = ui
                            .button(format!("{} Open file…", egui_phosphor::regular::FOLDER_OPEN));
                        if open_btn.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::OpenFileDialog));
                            ui.close();
                        }

                        // Recent files, newest first.
                        let recents = self.ui_store.recent_files.clone();
                        if !recents.is_empty() {
                            ui.menu_button(
                                format!(
                                    "{} Open recent",
                                    egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                                ),
                                |ui| {
                                    ui.set_min_width(220.0);
                                    for path in recents {
                                        let exists = path.exists();
                                        let label = path
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("(unknown)")
                                            .to_string();
                                        let entry = ui.add_enabled(
                                            exists,
                                            egui::Button::new(label).frame(false),
                                        );
                                        text_tooltip(
                                            ui,
                                            entry.id.with(("recent", path.display().to_string())),
                                            &entry,
                                            &path.display().to_string(),
                                        );
                                        if entry.clicked() {
                                            commands.push_back(Command::OpenFile(path).into());
                                            ui.close();
                                        }
                                    }
                                },
                            );
                        }
                        ui.separator();

                        let save_btn =
                            ui.button(format!("{} Save", egui_phosphor::regular::FLOPPY_DISK));
                        text_tooltip(ui, save_btn.id.with("save_tip"), &save_btn, &save_tip);
                        if save_btn.clicked() {
                            commands.push_back(DocumentCommand::Save.into());
                            ui.close();
                        }
                        let save_as_btn =
                            ui.button(format!("{} Save As…", egui_phosphor::regular::FLOPPY_DISK));
                        if save_as_btn.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::SaveAsDialog));
                            ui.close();
                        }
                        let export_btn =
                            ui.button(format!("{} Export…", egui_phosphor::regular::EXPORT));
                        text_tooltip(
                            ui,
                            export_btn.id.with("export_tip"),
                            &export_btn,
                            "Export image, video or GIF",
                        );
                        if export_btn.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::OpenExportDialog));
                            ui.close();
                        }
                        ui.separator();
                        let reload_btn = ui.button(format!(
                            "{} Reload from disk",
                            egui_phosphor::regular::ARROW_CLOCKWISE
                        ));
                        text_tooltip(
                            ui,
                            reload_btn.id.with("reload_tip"),
                            &reload_btn,
                            &reload_tip,
                        );
                        if reload_btn.clicked() {
                            commands.push_back(DocumentCommand::Reload.into());
                            ui.close();
                        }
                        let rebuild_btn = ui.button(format!(
                            "{} Rebuild timeline",
                            egui_phosphor::regular::HARD_DRIVES
                        ));
                        text_tooltip(
                            ui,
                            rebuild_btn.id.with("rebuild_tip"),
                            &rebuild_btn,
                            &rebuild_tip,
                        );
                        if rebuild_btn.clicked() {
                            commands.push_back(DocumentCommand::Rebuild.into());
                            ui.close();
                        }
                        ui.separator();
                        let workspace_btn = ui.button(format!(
                            "{} Switch workspace…",
                            egui_phosphor::regular::FOLDER_NOTCH
                        ));
                        text_tooltip(
                            ui,
                            workspace_btn.id.with("workspace_tip"),
                            &workspace_btn,
                            "Change workspace directory",
                        );
                        if workspace_btn.clicked() {
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
                            ui.add_space(sp.base.space_5);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                                for (i, name) in scene_names.iter().enumerate() {
                                    if i > 0 {
                                        ui.label(
                                            RichText::new(egui_phosphor::regular::ARROW_RIGHT)
                                                .size(TextRole::BodyS.size())
                                                .color(t.text.muted),
                                        );
                                    }
                                    let is_active = active_scene == Some(name.as_str());
                                    let color = if is_active {
                                        t.text.primary
                                    } else {
                                        t.text.muted
                                    };
                                    let label = RichText::new(name.as_str())
                                        .size(TextRole::BodyS.size())
                                        .color(color)
                                        .strong();
                                    let btn = egui::Button::new(label)
                                        .frame(false)
                                        .sense(egui::Sense::click());
                                    let scene_btn = ui.add(btn);
                                    text_tooltip(
                                        ui,
                                        scene_btn.id.with(("scene_tip", name.as_str())),
                                        &scene_btn,
                                        &format!("Switch to scene '{}'", name),
                                    );
                                    if scene_btn.clicked() {
                                        commands.push_back(
                                            SceneCommand::SelectScene(name.clone()).into(),
                                        );
                                    }
                                }
                            });
                        }
                    }

                    // ── Center: record + debug. Canvas view controls (grid,
                    // guides, labels, zoom) now live in the preview header. ──
                    ui.add_space(sp.base.space_5);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);

                        // Global playback transport (survives bottom-tab switches)
                        super::transport::transport_ui(
                            ui,
                            &mut self.preview_store.preview,
                            commands,
                        );
                        ui.separator();

                        // Auto-key / record toggle. This is the single control
                        // that decides whether property edits write keyframes at
                        // the playhead, so it must always be visible.
                        let recording = self.ui_store.keyframe_mode;
                        let rec_btn = ui.add(
                            Button::ghost("")
                                .with_icon(egui_phosphor::regular::RECORD)
                                .with_tooltip(if recording {
                                    "Auto-key ON — property edits write keyframes at the playhead"
                                } else {
                                    "Auto-key OFF — edits change the base value; click a ◆ to key"
                                })
                                .active(recording)
                                .icon_color(if recording {
                                    t.status.error
                                } else {
                                    t.text.muted
                                })
                                .hover_icon_color(t.status.error),
                        );
                        if rec_btn.clicked() {
                            self.ui_store.keyframe_mode = !recording;
                        }

                        // Debug dropdown (grouped debug toggles)
                        ui.menu_button(
                            RichText::new("Debug")
                                .size(TextRole::BodyS.size())
                                .color(t.text.secondary),
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
                    });

                    // Right-aligned: inspector + settings + command palette
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);

                        // Plugin status
                        if ui
                            .add(
                                Button::icon(egui_phosphor::regular::PLUG)
                                    .with_tooltip("Plugin status"),
                            )
                            .clicked()
                        {
                            commands.push_back(ShellAction::View(ViewAction::OpenPluginStatus));
                        }

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

                        // Layout presets + reset (design doc §9.2)
                        ui.menu_button("Layout", |ui| {
                            for preset in crate::app::LayoutPreset::ALL {
                                if ui.button(preset.label()).clicked() {
                                    commands.push_back(ShellAction::View(ViewAction::ApplyLayout(
                                        preset,
                                    )));
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Reset layout").clicked() {
                                commands.push_back(ShellAction::View(ViewAction::ResetLayout));
                                ui.close();
                            }
                        });

                        // Detail region: Inspector | Code share one tab group.
                        let active_tab = if self.ui_store.view.detail_visible {
                            crate::app::persistence::active_detail_tab(&self.ui_store.view.tree)
                        } else {
                            None
                        };

                        let inspector_tip =
                            crate::app::interaction::keyboard::tooltip_with_shortcut(
                                &self.shortcut_registry,
                                "Inspector",
                                &crate::app::interaction::keyboard::KeyboardAction::ToggleInspector,
                                ui.ctx(),
                            );
                        let inspector_active =
                            active_tab == Some(crate::app::WorkspaceTab::Inspector);
                        let inspector_resp = ui.add(
                            Button::ghost("")
                                .with_icon(egui_phosphor::regular::SLIDERS)
                                .active(inspector_active),
                        );
                        text_tooltip(
                            ui,
                            inspector_resp.id.with("inspector_tip"),
                            &inspector_resp,
                            &inspector_tip,
                        );
                        if inspector_resp.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::ShowInspector));
                        }

                        let code_tip = crate::app::interaction::keyboard::tooltip_with_shortcut(
                            &self.shortcut_registry,
                            "Code",
                            &crate::app::interaction::keyboard::KeyboardAction::ToggleCode,
                            ui.ctx(),
                        );
                        let code_active = active_tab == Some(crate::app::WorkspaceTab::Code);
                        let code_resp = ui.add(
                            Button::ghost("")
                                .with_icon(egui_phosphor::regular::CODE)
                                .active(code_active),
                        );
                        text_tooltip(ui, code_resp.id.with("code_tip"), &code_resp, &code_tip);
                        if code_resp.clicked() {
                            commands.push_back(ShellAction::View(ViewAction::ShowCode));
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
