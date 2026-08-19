//! Plugin status dialog: manifests, loaded libraries, capabilities, errors,
//! reload controls, and explicit plugin path management.

use std::path::PathBuf;

use crate::app::GuiShell;
use crate::app::components::button::Button;
use crate::app::components::dialog::{self, DialogSpec};
use crate::app::components::{Badge, Tag};
use crate::app::design_tokens::spatial::{RADIUS_S, spatial};
use crate::app::design_tokens::typography::TextRole;

impl GuiShell {
    pub(crate) fn plugin_status_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = spatial(ui);
        let spec = DialogSpec::new("plugin_status", [620.0, 520.0])
            .with_min_size([520.0, 380.0])
            .with_max_size([760.0, 640.0]);

        let snapshot = self.plugin_manager.snapshot();
        let mut reload = false;
        let mut add_path: Option<PathBuf> = None;
        let mut remove_path: Option<PathBuf> = None;
        let mut describe_library: Option<PathBuf> = None;

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let title_close = dialog::title_row(ui, "Plugins");
            ui.add_space(sp.base.space_3);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Loaded plugins")
                        .size(TextRole::Title.size())
                        .color(theme.text.primary)
                        .strong(),
                );
                ui.add(Badge::new(snapshot.plugin_names.len().to_string()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            Button::ghost("Reload")
                                .with_icon(egui_phosphor::regular::ARROW_CLOCKWISE),
                        )
                        .clicked()
                    {
                        reload = true;
                    }
                });
            });
            ui.add_space(sp.base.space_2);

            if snapshot.plugin_names.is_empty() {
                ui.label(
                    egui::RichText::new("No native or in-process plugins are installed.")
                        .size(TextRole::BodyS.size())
                        .color(theme.text.muted),
                );
            } else {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for name in &snapshot.plugin_names {
                            ui.add(Tag::new(name.clone()).color(theme.accent.cyan));
                        }
                    });
                });
            }
            ui.add_space(sp.base.space_4);

            ui.label(
                egui::RichText::new("Manifests")
                    .size(TextRole::Title.size())
                    .color(theme.text.primary)
                    .strong(),
            );
            ui.add_space(sp.base.space_2);
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                if snapshot.sources.is_empty() {
                    ui.label(
                        egui::RichText::new("No `.amx-plugin.toml` manifests found.")
                            .size(TextRole::BodyS.size())
                            .color(theme.text.muted),
                    );
                }
                for source in &snapshot.sources {
                    let primitives = source.manifest.primitives.len();
                    let properties = source.manifest.properties.len();
                    let actions = source.manifest.actions.len();
                    let functions = source.manifest.functions.len();
                    let services = source.manifest.services.len();
                    egui::Frame::new()
                        .fill(theme.surface.widget)
                        .stroke(egui::Stroke::new(1.0, theme.border.default))
                        .corner_radius(RADIUS_S)
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(source.path.display().to_string())
                                        .size(TextRole::BodyS.size())
                                        .color(theme.text.primary)
                                        .strong(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(library) = source.manifest.library.as_deref() {
                                            ui.label(
                                                egui::RichText::new(library)
                                                    .size(TextRole::Micro.size())
                                                    .color(theme.text.muted),
                                            );
                                        }
                                    },
                                );
                            });
                            ui.add_space(sp.base.space_1);
                            ui.horizontal_wrapped(|ui| {
                                capability_badge(ui, "P", primitives, theme);
                                capability_badge(ui, "Props", properties, theme);
                                capability_badge(ui, "Actions", actions, theme);
                                capability_badge(ui, "Fns", functions, theme);
                                capability_badge(ui, "Svcs", services, theme);
                            });
                        });
                    ui.add_space(sp.base.space_2);
                }
            });
            ui.add_space(sp.base.space_4);

            ui.label(
                egui::RichText::new("Issues")
                    .size(TextRole::Title.size())
                    .color(theme.text.primary)
                    .strong(),
            );
            ui.add_space(sp.base.space_2);
            if snapshot.issues.is_empty() {
                ui.label(
                    egui::RichText::new("No plugin load or install issues.")
                        .size(TextRole::BodyS.size())
                        .color(theme.text.muted),
                );
            } else {
                for issue in &snapshot.issues {
                    let message = issue
                        .path
                        .as_ref()
                        .map(|path| format!("{}: {}", path.display(), issue.message))
                        .unwrap_or_else(|| issue.message.clone());
                    ui.label(
                        egui::RichText::new(message)
                            .size(TextRole::BodyS.size())
                            .color(theme.status.error),
                    );
                }
            }
            ui.add_space(sp.base.space_4);

            ui.label(
                egui::RichText::new("Explicit plugin paths")
                    .size(TextRole::Title.size())
                    .color(theme.text.primary)
                    .strong(),
            );
            ui.add_space(sp.base.space_2);
            let explicit = self.plugin_manager.explicit_plugin_paths();
            for path in &explicit {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .size(TextRole::Micro.size())
                            .color(theme.text.secondary),
                    );
                    if ui
                        .add(
                            Button::icon(egui_phosphor::regular::X)
                                .with_tooltip("Remove explicit plugin path"),
                        )
                        .clicked()
                    {
                        remove_path = Some(path.clone());
                    }
                });
            }
            ui.horizontal(|ui| {
                eparts::TextField::new(&mut self.ui_store.plugin_path_input)
                    .placeholder("/path/to/plugin.amx-plugin.toml")
                    .desired_width(ui.available_width() - 96.0)
                    .show(ui);
                if ui.add(Button::ghost("Add").with_icon(egui_phosphor::regular::PLUS)).clicked() {
                    let trimmed = self.ui_store.plugin_path_input.trim();
                    if !trimmed.is_empty() {
                        add_path = Some(PathBuf::from(trimmed));
                        self.ui_store.plugin_path_input.clear();
                    }
                }
            });
            #[cfg(feature = "plugin-loading")]
            ui.add_space(sp.base.space_2);
            #[cfg(feature = "plugin-loading")]
            if ui
                .add(
                    Button::ghost("Generate manifest from library…")
                        .with_icon(egui_phosphor::regular::FILE_CODE),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Native plugin", &["so", "dylib", "dll"])
                    .pick_file()
                {
                    describe_library = Some(path);
                }
            }

            title_close
        });

        let path_changed = remove_path.is_some() || add_path.is_some();
        let mut paths = self.plugin_manager.explicit_plugin_paths();
        if let Some(path) = remove_path {
            paths.retain(|existing| existing != &path);
        }
        if let Some(path) = add_path
            && !paths.contains(&path)
        {
            paths.insert(0, path);
        }
        let changed = if reload || path_changed {
            if reload && !path_changed {
                self.plugin_manager.reload()
            } else {
                self.plugin_manager.set_explicit_plugin_paths(paths)
            }
        } else {
            false
        };
        if changed {
            self.apply_plugin_reload();
            self.save_persistence();
        }

        #[cfg(feature = "plugin-loading")]
        if let Some(library) = describe_library {
            let output = library.with_extension("amx-plugin.toml");
            match crate::app::document::plugins::generate_manifest_for_library(&library, &output) {
                Ok(path) => {
                    self.preview_store
                        .preview
                        .set_status_info(format!("Wrote plugin manifest: {path}"));
                },
                Err(err) => {
                    self.preview_store
                        .preview
                        .set_status_error(format!("Plugin describe failed: {err}"));
                },
            }
        }

        if !open {
            self.ui_store.view.plugin_status_open = false;
        }
    }
}

fn capability_badge(ui: &mut egui::Ui, label: &str, count: usize, theme: eparts::Theme) {
    ui.add(Tag::new(format!("{label} {count}")).color(theme.text.muted));
}
