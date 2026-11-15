#[cfg(feature = "theme-json")]
use std::path::PathBuf;

use egui::RichText;
use eparts::{Button, Checkbox};

use crate::app::components::layout;
use crate::app::design_tokens::typography::TextRole;
use crate::app::interaction::keyboard::{ShortcutRegistry, saved_shortcut_from_key};
use crate::app::{GuiShell, components};

mod shortcuts;

const SETTINGS_INPUT_WIDTH: f32 = 120.0;

impl GuiShell {
    pub(crate) fn settings_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = components::dialog::DialogSpec::new("Settings", [420.0, 520.0])
            .with_min_size([380.0, 400.0])
            .with_max_size([600.0, 700.0])
            .with_resizable(true);

        let open = components::dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = components::dialog::title_row(ui, "Settings");
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_3);

            self.settings_preview_section(ui, &sp);
            self.settings_appearance_section(ui, &sp);
            self.settings_colorscheme_section(ui, &sp);
            self.settings_playback_section(ui, &sp);
            self.settings_autosave_section(ui, &sp);
            self.settings_shortcuts_section(ui, &sp);
            self.settings_editor_section(ui, &sp);

            close
        });

        if !open {
            self.ui_store.view.settings_open = false;
        }
    }

    fn settings_preview_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Preview ──
        layout::section_header(ui, egui_phosphor::regular::GRID_FOUR, "Preview", None);
        ui.add_space(sp.base.space_2);

        {
            let mut v = self.preview_store.preview.overlay.grid_size as f64;
            eparts::widget::Form::new("preview_form")
                .label_width(SETTINGS_INPUT_WIDTH)
                .show(ui, |f| {
                    f.field("Grid size", |ui| {
                        eparts::NumberField::new(&mut v)
                            .range(1.0..=200.0)
                            .speed(1.0)
                            .suffix(" px")
                            .show(ui);
                    });
                });
            self.preview_store.preview.overlay.grid_size = v as f32;
        }
        ui.add_space(sp.base.space_3);
    }

    fn settings_appearance_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Appearance (IDE light/dark/auto) ──
        layout::section_header(ui, egui_phosphor::regular::SUN, "Appearance", None);
        ui.add_space(sp.base.space_2);

        {
            let mut theme_idx = match self.ui_store.view.app_theme {
                eparts::AppThemeChoice::Auto => Some(0),
                eparts::AppThemeChoice::Light => Some(1),
                eparts::AppThemeChoice::Dark => Some(2),
            };
            let themes = ["Auto (system)", "Light", "Dark"];

            eparts::widget::Form::new("appearance_form")
                .label_width(SETTINGS_INPUT_WIDTH)
                .show(ui, |f| {
                    f.field("Theme", |ui| {
                        ui.add(
                            eparts::widget::Select::new(
                                "app_theme_select",
                                &mut theme_idx,
                                &themes,
                            )
                            .placeholder("Select theme"),
                        );
                    });
                });

            self.ui_store.view.app_theme = match theme_idx {
                Some(0) => eparts::AppThemeChoice::Auto,
                Some(1) => eparts::AppThemeChoice::Light,
                Some(2) => eparts::AppThemeChoice::Dark,
                _ => self.ui_store.view.app_theme,
            };

            #[cfg(feature = "theme-json")]
            {
                let theme = eparts::theme(ui);
                self.settings_json_theme_section(ui, sp, &theme);
            }
            ui.add_space(sp.base.space_2);

            eparts::widget::Form::new("motion_form").label_width(SETTINGS_INPUT_WIDTH).show(
                ui,
                |f| {
                    f.field("Reduce motion", |ui| {
                        ui.add(
                            Checkbox::new(&mut self.ui_store.view.reduce_motion)
                                .label("Snap animations"),
                        );
                    });
                },
            );

            ui.add_space(sp.base.space_2);

            {
                let mut density_idx = match self.ui_store.view.density {
                    eparts::Density::Default => Some(0usize),
                    eparts::Density::Compact => Some(1),
                };
                let densities = ["Default", "Compact"];

                eparts::widget::Form::new("density_form")
                    .label_width(SETTINGS_INPUT_WIDTH)
                    .show(ui, |f| {
                        f.field("Density", |ui| {
                            ui.add(
                                eparts::widget::Select::new(
                                    "density_select",
                                    &mut density_idx,
                                    &densities,
                                )
                                .placeholder("Select density"),
                            );
                        });
                    });

                self.ui_store.view.density = match density_idx {
                    Some(1) => eparts::Density::Compact,
                    _ => eparts::Density::Default,
                };
            }
        }
        ui.add_space(sp.base.space_3);
    }

    #[cfg(feature = "theme-json")]
    fn settings_json_theme_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
        theme: &eparts::Theme,
    ) {
        let mut theme_dir_text = self
            .ui_store
            .view
            .theme_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut theme_dir_changed = false;
        eparts::widget::Form::new("json_theme_dir_form")
            .label_width(SETTINGS_INPUT_WIDTH)
            .show(ui, |f| {
                f.field("Theme dir", |ui| {
                    let response = eparts::TextField::new(&mut theme_dir_text)
                        .desired_width(SETTINGS_INPUT_WIDTH + 80.0)
                        .show(ui);
                    if response.changed {
                        theme_dir_changed = true;
                    }
                });
            });
        if theme_dir_changed {
            let trimmed = theme_dir_text.trim();
            if trimmed.is_empty() {
                self.ui_store.view.theme_dir = None;
                self.ui_store.view.theme_name = None;
            } else {
                self.ui_store.view.theme_dir = Some(PathBuf::from(trimmed));
            }
        }

        let names = self.ui_store.view.theme_names.clone();
        if !names.is_empty() {
            let mut selected = names
                .iter()
                .position(|name| self.ui_store.view.theme_name.as_deref() == Some(name))
                .or(Some(0));
            eparts::widget::Form::new("json_theme_name_form")
                .label_width(SETTINGS_INPUT_WIDTH)
                .show(ui, |f| {
                    f.field("Theme", |ui| {
                        ui.add(
                            eparts::widget::Select::new("json_theme_select", &mut selected, &names)
                                .placeholder("Select JSON theme"),
                        );
                    });
                });
            if let Some(index) = selected {
                self.ui_store.view.theme_name = Some(names[index].clone());
            }
        }
        if let Some(error) = &self.ui_store.view.theme_error {
            ui.colored_label(theme.palette.status.error, error);
            ui.add_space(sp.base.space_2);
        }
    }

    fn settings_colorscheme_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Colorscheme ──
        layout::section_header(ui, egui_phosphor::regular::PALETTE, "Colorscheme", None);
        ui.add_space(sp.base.space_2);

        let document = &self.document_store.source.document;
        let active = document.active_timeline_ref();
        let current_scheme = active
            .as_ref()
            .map(|active| active.timeline.colorscheme_name())
            .unwrap_or("Unavailable");
        let scene_label = active.as_ref().and_then(|active| active.scene_name).unwrap_or("Scene");
        ui.horizontal(|ui| {
            ui.label("Scene");
            ui.label(egui::RichText::new(scene_label).weak());
        });
        ui.horizontal(|ui| {
            ui.label("Colorscheme");
            ui.label(egui::RichText::new(current_scheme).strong());
        });
        ui.label(
            egui::RichText::new("Edit `colorscheme` in this scene’s `config` block in Write.")
                .small()
                .weak(),
        );
        ui.add_space(sp.base.space_3);
    }

    fn settings_playback_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Playback ──
        layout::section_header(ui, egui_phosphor::regular::PLAY, "Playback", None);
        ui.add_space(sp.base.space_2);

        {
            let mut scrub_step_s = self.ui_store.scrub_step_s;
            eparts::widget::Form::new("playback_form")
                .label_width(SETTINGS_INPUT_WIDTH)
                .show(ui, |f| {
                    f.field("Scrub step", |ui| {
                        eparts::NumberField::new(&mut scrub_step_s)
                            .range(0.01..=1.0)
                            .speed(0.01)
                            .suffix(" s")
                            .show(ui);
                    });
                });
            self.ui_store.scrub_step_s = scrub_step_s;
        }
        ui.add_space(sp.base.space_3);
    }

    fn settings_autosave_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Autosave & Recovery ──
        layout::section_header(ui, egui_phosphor::regular::CLOCK, "Autosave", None);
        ui.add_space(sp.base.space_2);

        {
            let autosave = &mut self.ui_store.view.autosave;
            let mut enabled = autosave.enabled;
            let mut interval_s = autosave.interval.as_secs_f64();
            let previous = (enabled, interval_s);

            eparts::widget::Form::new("autosave_form")
                .label_width(SETTINGS_INPUT_WIDTH)
                .show(ui, |f| {
                    f.field("Autosave", |ui| {
                        ui.add(
                            Checkbox::new(&mut enabled).label("Write recovery file while dirty"),
                        );
                    });
                    f.field("Interval", |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            eparts::NumberField::new(&mut interval_s)
                                .range(
                                    crate::app::stores::ui_store::MIN_AUTOSAVE_INTERVAL_S..=600.0,
                                )
                                .speed(1.0)
                                .suffix(" s")
                                .show(ui);
                        });
                    });
                });

            autosave.enabled = enabled;
            autosave.interval = std::time::Duration::from_secs_f64(interval_s.clamp(
                crate::app::stores::ui_store::MIN_AUTOSAVE_INTERVAL_S,
                crate::app::stores::ui_store::MAX_AUTOSAVE_INTERVAL_S,
            ));
            if (enabled, autosave.interval.as_secs_f64()) != previous {
                // Persist immediately so the preference survives a crash.
                self.persist_autosave_prefs();
            }
        }
        ui.add_space(sp.base.space_3);
    }

    fn settings_editor_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        // ── Editor ──
        layout::section_header(ui, egui_phosphor::regular::PENCIL, "Editor", None);
        ui.add_space(sp.base.space_2);

        {
            let mut rebuild_debounce_ms = self.ui_store.rebuild_debounce_ms as f64;
            let mut undo_limit = self.document_store.history.undo_limit as f64;
            let mut snap_fps = self.ui_store.snap_fps as f64;

            eparts::widget::Form::new("editor_form").label_width(SETTINGS_INPUT_WIDTH).show(
                ui,
                |f| {
                    f.field("Rebuild debounce", |ui| {
                        eparts::NumberField::new(&mut rebuild_debounce_ms)
                            .range(0.0..=1000.0)
                            .speed(10.0)
                            .suffix(" ms")
                            .show(ui);
                    });
                    f.field("Undo limit", |ui| {
                        eparts::NumberField::new(&mut undo_limit)
                            .range(10.0..=1000.0)
                            .speed(10.0)
                            .suffix(" entries")
                            .show(ui);
                    });
                    f.field("Snap FPS", |ui| {
                        eparts::NumberField::new(&mut snap_fps)
                            .range(1.0..=240.0)
                            .speed(1.0)
                            .suffix(" fps")
                            .show(ui);
                    });
                },
            );

            self.ui_store.rebuild_debounce_ms = rebuild_debounce_ms as u64;
            self.document_store.history.undo_limit = undo_limit as usize;
            self.ui_store.snap_fps = snap_fps as f32;
        }
    }
}
