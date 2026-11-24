use egui::{Align, Layout, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;
use eparts::Checkbox;
use eparts::widget::UiExt;

use crate::app::commands::{ActionQueue, Command, ShellAction, ViewAction};
use crate::app::components::button::Button;
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::{STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::interaction::keyboard::{KeyboardAction, tooltip_with_shortcut};
use crate::app::{GuiShell, LayoutPreset, WorkspaceTab};

impl GuiShell {
    pub(crate) fn toolbar_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let theme = eparts::theme(ui);
        let sp = spatial(ui);
        let response = egui::Frame::new()
            .fill(theme.palette.surface.panel)
            .inner_margin(egui::Margin::symmetric(sp.base.space_5 as i8, 0))
            .show(ui, |ui| {
                let width = ui.available_width();
                let wide =
                    !crate::app::persistence::compact_for_width(width + 2.0 * sp.base.space_5);
                let show_export_text = width >= 520.0;
                let height = sp.toolbar.height;
                let right_width = if show_export_text { 246.0 } else { 195.0 };
                let preset_width = if wide { 332.0 } else { 0.0 };
                let (row, _) =
                    ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
                let document_right = if wide {
                    row.center().x - preset_width * 0.5 - sp.base.space_4
                } else {
                    row.right() - right_width - sp.base.space_4
                };
                let document_rect = row.with_max_x(document_right.max(row.left()));
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(document_rect)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| self.toolbar_document(ui, commands),
                );

                if wide {
                    let preset_rect = egui::Rect::from_center_size(
                        row.center(),
                        Vec2::new(preset_width, sp.base.row_m + sp.base.space_4),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(preset_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| self.toolbar_presets(ui, commands),
                    );
                }

                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(row.with_min_x(row.right() - right_width))
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| self.toolbar_controls(ui, commands, wide, show_export_text),
                );
            });
        let rect = response.response.rect;
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        );
    }

    fn toolbar_document(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let theme = eparts::theme(ui);
        let sp = spatial(ui);
        if ui.available_width() > 120.0 {
            paint_app_icon(ui, sp.base.space_8);
        }

        let document = &self.document_store.source.document;
        let path = document.file_path.display().to_string();
        let filename = document
            .file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        let dirty = self.document_store.source.is_dirty();
        let title = if dirty {
            format!("{filename} *")
        } else {
            filename.to_owned()
        };
        let color = theme.palette.text.primary;
        let title_width = ui
            .painter()
            .layout_no_wrap(title.clone(), TextRole::Subheading.font_id(), color)
            .size()
            .x
            + ui.spacing().button_padding.x * 2.0;
        let label_width = title_width.min((ui.available_width() - 34.0).clamp(0.0, 294.0));
        let trigger = ui.add_sized(
            Vec2::new(label_width, 30.0),
            egui::Button::new(
                RichText::new(title).font(TextRole::Subheading.font_id()).color(color),
            )
            .truncate()
            .frame(false)
            .sense(egui::Sense::click()),
        );
        let trigger = trigger.on_hover_text(if dirty {
            format!("{path} · Unsaved changes")
        } else {
            path
        });
        let caret = ui.add(Button::icon(icons::CARET_DOWN).with_tooltip("File actions"));
        let menu_trigger = trigger.union(caret);

        let tip = |label, action| {
            tooltip_with_shortcut(&self.shortcut_registry, label, &action, ui.ctx())
        };
        let save = tip("Save", KeyboardAction::Save);
        let reload = tip("Reload from disk", KeyboardAction::Reload);
        let rebuild = tip("Rebuild timeline", KeyboardAction::Rebuild);
        self.toolbar_file_menu(commands, &menu_trigger, &save, &reload, &rebuild);
    }

    fn toolbar_file_menu(
        &mut self,
        commands: &mut ActionQueue,
        anchor: &egui::Response,
        save_tip: &str,
        reload_tip: &str,
        rebuild_tip: &str,
    ) {
        egui::Popup::menu(anchor).show(|ui| {
            let new_btn = ui.button(format!("{} New scene", egui_phosphor::regular::FILE_PLUS));
            if new_btn.clicked() {
                commands.push_back(ShellAction::View(ViewAction::NewFile));
                ui.close();
            }
            ui.menu_button("New project…", |ui| {
                for template in animatix_render::delivery::ProjectTemplate::ALL {
                    if ui.button(template.title()).clicked() {
                        commands.push_back(ShellAction::View(ViewAction::NewProject(template)));
                        ui.close();
                    }
                }
            });
            if ui.button("Voiceover…").clicked() {
                self.open_narration_takes(ui.ctx());
                ui.close();
            }
            let open_btn = ui.button(format!("{} Open file…", egui_phosphor::regular::FOLDER_OPEN));
            if open_btn.clicked() {
                commands.push_back(ShellAction::View(ViewAction::OpenFileDialog));
                ui.close();
            }

            // Recent files, newest first.
            self.toolbar_recent_files_menu(ui, commands);
            ui.separator();

            let save_btn = ui.button(format!("{} Save", egui_phosphor::regular::FLOPPY_DISK));
            text_tooltip(ui, save_btn.id.with("save_tip"), &save_btn, save_tip);
            if save_btn.clicked() {
                commands.push_back(Command::Save.into());
                ui.close();
            }
            let save_as_btn =
                ui.button(format!("{} Save As…", egui_phosphor::regular::FLOPPY_DISK));
            if save_as_btn.clicked() {
                commands.push_back(ShellAction::View(ViewAction::SaveAsDialog));
                ui.close();
            }
            ui.separator();
            let reload_btn =
                ui.button(format!("{} Reload from disk", egui_phosphor::regular::ARROW_CLOCKWISE));
            text_tooltip(ui, reload_btn.id.with("reload_tip"), &reload_btn, reload_tip);
            if reload_btn.clicked() {
                commands.push_back(Command::Reload.into());
                ui.close();
            }
            let rebuild_btn =
                ui.button(format!("{} Rebuild timeline", egui_phosphor::regular::HARD_DRIVES));
            text_tooltip(ui, rebuild_btn.id.with("rebuild_tip"), &rebuild_btn, rebuild_tip);
            if rebuild_btn.clicked() {
                commands.push_back(Command::Rebuild.into());
                ui.close();
            }
            ui.separator();
            let workspace_btn =
                ui.button(format!("{} Switch workspace…", egui_phosphor::regular::FOLDER_NOTCH));
            text_tooltip(
                ui,
                workspace_btn.id.with("workspace_tip"),
                &workspace_btn,
                "Change workspace directory",
            );
            if workspace_btn.clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    commands.push_back(Command::SwitchWorkspace(path).into());
                }
                ui.close();
            }
        });
    }

    fn toolbar_recent_files_menu(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let recents = self.ui_store.recent_files.clone();
        if recents.is_empty() {
            return;
        }
        ui.menu_button(
            format!("{} Open recent", egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE),
            |ui| {
                ui.set_min_width(220.0);
                for path in recents {
                    let exists = path.exists();
                    let label = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("(unknown)")
                        .to_string();
                    let entry = ui.add_enabled(exists, egui::Button::new(label).frame(false));
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

    fn toolbar_presets(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let presets = LayoutPreset::ALL;
        let labels = presets.map(LayoutPreset::label);
        let tips = presets.map(LayoutPreset::description);
        let mut active = presets
            .iter()
            .position(|&p| p == self.ui_store.view.layout_preset)
            .expect("the toolbar includes every layout preset");
        if eparts::TabBar::new(ui.scope_id().with("layouts"), &mut active, &labels)
            .with_tooltips(&tips)
            .height(34.0)
            .show(ui)
            .clicked()
        {
            commands.push_back(ShellAction::View(ViewAction::ApplyLayout(presets[active])));
        }
    }

    fn toolbar_controls(
        &mut self,
        ui: &mut egui::Ui,
        commands: &mut ActionQueue,
        wide: bool,
        show_export_text: bool,
    ) {
        let export = if show_export_text {
            Button::secondary("Export").with_icon(icons::EXPORT)
        } else {
            Button::icon(icons::EXPORT)
        };
        if ui.add(export.with_tooltip("Export image, video or GIF")).clicked() {
            commands.push_back(ShellAction::View(ViewAction::OpenExportDialog));
        }

        crate::app::components::icon_menu(ui, icons::DOTS_THREE, "Workspace actions", |ui| {
            self.toolbar_workspace_menu(ui, commands);
        });

        let frame_view = &self.ui_store.view;
        let drawer_open =
            frame_view.compact_drawer == Some(crate::app::panels::CompactDrawer::Detail);
        let inspector_open =
            drawer_open || crate::app::persistence::detail_visible(&frame_view.tree);
        let editor_open =
            crate::app::persistence::pane_visible(&frame_view.tree, WorkspaceTab::Code);
        for (active, icon, label, key, action) in [
            (
                inspector_open,
                icons::MAGNIFYING_GLASS,
                "Inspector",
                KeyboardAction::ToggleInspector,
                ViewAction::ShowInspector,
            ),
            (
                editor_open,
                icons::CODE,
                "Source editor",
                KeyboardAction::ToggleCode,
                ViewAction::ShowCode,
            ),
        ] {
            let tip = tooltip_with_shortcut(&self.shortcut_registry, label, &key, ui.ctx());
            if ui.add(Button::icon(icon).with_tooltip(tip).active(active)).clicked() {
                commands.push_back(ShellAction::View(action));
            }
        }

        if !wide {
            let preset = self.ui_store.view.layout_preset;
            let button = if show_export_text {
                Button::ghost(preset.label()).with_icon(preset.icon())
            } else {
                Button::icon(preset.icon())
            };
            let response = ui.add(button.with_tooltip("Workspace layout"));
            egui::Popup::menu(&response).show(|ui| self.toolbar_layout_menu(ui, commands));
        }
    }

    fn toolbar_layout_menu(&self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        for preset in LayoutPreset::ALL {
            let label = format!("{}  {}", preset.icon(), preset.label());
            if ui
                .stable_selectable_label(self.ui_store.view.layout_preset == preset, label)
                .on_hover_text(preset.description())
                .clicked()
            {
                commands.push_back(ShellAction::View(ViewAction::ApplyLayout(preset)));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Reset layout").clicked() {
            commands.push_back(ShellAction::View(ViewAction::ResetLayout));
            ui.close();
        }
    }

    fn toolbar_workspace_menu(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        if ui.button(format!("{}  Settings", icons::GEAR)).clicked() {
            self.ui_store.view.settings_open = true;
            ui.close();
        }
        if ui.button(format!("{}  Keyboard shortcuts", icons::COMMAND)).clicked() {
            self.ui_store.view.shortcuts_open = true;
            ui.close();
        }
        if ui.button(format!("{}  Plugin status", icons::PLUG)).clicked() {
            commands.push_back(ShellAction::View(ViewAction::OpenPluginStatus));
            ui.close();
        }
        let diagnostics = &mut self.ui_store.view.diagnostics_panel_visible;
        ui.add(Checkbox::new(diagnostics).label("Diagnostics panel"));
        ui.menu_button(format!("{}  Preview overlays", icons::BUG), |ui| {
            let view = &mut self.ui_store.view;
            ui.add(Checkbox::new(&mut view.debug_bounds).label("Bounds"));
            ui.add(Checkbox::new(&mut view.debug_layout).label("Layout"));
            ui.add(Checkbox::new(&mut view.debug_spacing).label("Spacing"));
            ui.add(
                Checkbox::new(&mut self.preview_store.preview.overlay.show_performance_hud)
                    .label("Performance HUD"),
            );
        });
        ui.separator();
        if ui.button("Reset layout").clicked() {
            commands.push_back(ShellAction::View(ViewAction::ResetLayout));
            ui.close();
        }
    }
}

/// Use the same app icon as the operating system's window chrome.
fn paint_app_icon(ui: &mut egui::Ui, size: f32) {
    let id = egui::Id::unique("animatix_toolbar_icon");
    let texture = ui.data(|data| data.get_temp::<egui::TextureHandle>(id)).or_else(|| {
        let icon =
            eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png")).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let texture =
            ui.ctx().load_texture("Animatix app icon", image, egui::TextureOptions::LINEAR);
        ui.data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        ui.add(egui::Image::new((texture.id(), Vec2::splat(size))));
    }
}
