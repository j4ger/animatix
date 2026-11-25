#[cfg(feature = "theme-json")]
use std::path::PathBuf;

use crate::app::design_tokens::typography::TextRole;
use crate::app::i18n::Language;
use crate::app::interaction::keyboard::{ShortcutRegistry, saved_shortcut_from_key};
use crate::app::{Command, GuiShell};
use egui::RichText;
use eparts::{Button, Switch};

mod page;
mod shortcuts;
use page::{copy, row};

fn number(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f32,
    suffix: &str,
) {
    eparts::NumberField::new(value).range(range).speed(step).suffix(suffix).show(ui);
}

impl GuiShell {
    pub(crate) fn open_settings_page(&mut self) {
        if self.is_playing() {
            let effects = self.handle_action(Command::TogglePlayback.into());
            self.apply_effects(effects);
        }
        self.ui_store.view.settings_open = true;
    }

    fn settings_appearance_section(&mut self, ui: &mut egui::Ui) {
        let current = self.ui_store.view.language;
        let mut next = current;
        row(
            ui,
            "interface-language",
            ("Interface language", "界面语言"),
            Some((
                "Changes the settings page language. Project content stays as written.",
                "切换设置页语言，项目内容保持原样。",
            )),
            |ui| {
                ui.horizontal(|ui| {
                    for (language, name) in Language::ALL.into_iter().zip(Language::NAMES) {
                        ui.push_id(language.key(), |ui| {
                            ui.radio_value(&mut next, language, name);
                        });
                    }
                });
            },
        );
        if current != next {
            self.ui_store.view.language = next;
            ui.ctx().request_repaint();
        }

        let mut theme = Some(match self.ui_store.view.app_theme {
            eparts::AppThemeChoice::Auto => 0,
            eparts::AppThemeChoice::Light => 1,
            eparts::AppThemeChoice::Dark => 2,
        });
        let themes = [
            copy(ui, "Auto (system)", "跟随系统"),
            copy(ui, "Light", "浅色"),
            copy(ui, "Dark", "深色"),
        ];
        row(ui, "theme", ("Theme", "主题"), None, |ui| {
            ui.add(eparts::widget::Select::new("app_theme_select", &mut theme, &themes));
        });
        self.ui_store.view.app_theme = match theme {
            Some(1) => eparts::AppThemeChoice::Light,
            Some(2) => eparts::AppThemeChoice::Dark,
            _ => eparts::AppThemeChoice::Auto,
        };
        #[cfg(feature = "theme-json")]
        self.settings_json_theme_section(ui);

        row(
            ui,
            "reduce-motion",
            ("Reduce motion", "减少动效"),
            Some(("Skip transitions when the interface changes.", "界面状态变化时跳过过渡动画。")),
            |ui| {
                ui.add(
                    Switch::new(&mut self.ui_store.view.reduce_motion)
                        .label_side(eparts::Side::Left),
                );
            },
        );
        let mut density = Some(usize::from(self.ui_store.view.density == eparts::Density::Compact));
        let densities = [copy(ui, "Default", "默认"), copy(ui, "Compact", "紧凑")];
        row(
            ui,
            "density",
            ("Interface density", "界面密度"),
            Some((
                "Adjust spacing and control sizes in the workbench.",
                "调整工作台的间距和控件尺寸。",
            )),
            |ui| {
                ui.add(eparts::widget::Select::new("density_select", &mut density, &densities));
            },
        );
        self.ui_store.view.density = if density == Some(1) {
            eparts::Density::Compact
        } else {
            eparts::Density::Default
        };
    }

    #[cfg(feature = "theme-json")]
    fn settings_json_theme_section(&mut self, ui: &mut egui::Ui) {
        let mut path = self
            .ui_store
            .view
            .theme_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut changed = false;
        row(ui, "theme-directory", ("Theme directory", "主题目录"), None, |ui| {
            changed = eparts::TextField::new(&mut path)
                .desired_width(ui.available_width())
                .show(ui)
                .changed;
        });
        if changed {
            let path = path.trim();
            self.ui_store.view.theme_dir = (!path.is_empty()).then(|| PathBuf::from(path));
            if path.is_empty() {
                self.ui_store.view.theme_name = None;
            }
        }
        let names = self.ui_store.view.theme_names.clone();
        if !names.is_empty() {
            let mut selected = names
                .iter()
                .position(|name| self.ui_store.view.theme_name.as_deref() == Some(name))
                .or(Some(0));
            row(ui, "custom-theme", ("Custom theme", "自定义主题"), None, |ui| {
                ui.add(eparts::widget::Select::new("json_theme_select", &mut selected, &names));
            });
            if let Some(index) = selected {
                self.ui_store.view.theme_name = Some(names[index].clone());
            }
        }
        if let Some(error) = &self.ui_store.view.theme_error {
            ui.colored_label(eparts::theme(ui).palette.status.error, error);
        }
    }

    fn settings_preview_section(&mut self, ui: &mut egui::Ui) {
        let mut grid = f64::from(self.preview_store.preview.overlay.grid_size);
        let mut step = self.ui_store.scrub_step_s;
        let mut fps = f64::from(self.ui_store.snap_fps);
        row(
            ui,
            "grid-size",
            ("Grid size", "网格尺寸"),
            Some(("Distance between the preview grid lines.", "预览网格线之间的间距。")),
            |ui| {
                number(ui, &mut grid, 1.0..=200.0, 1.0, " px");
            },
        );
        row(
            ui,
            "scrub-step",
            ("Scrub step", "跳转步长"),
            Some((
                "Distance the playhead moves per scrub shortcut.",
                "使用跳转快捷键时，播放头移动的时长。",
            )),
            |ui| {
                number(ui, &mut step, 0.01..=1.0, 0.01, " s");
            },
        );
        row(ui, "snap-fps", ("Snap frame rate", "吸附帧率"), None, |ui| {
            number(ui, &mut fps, 1.0..=240.0, 1.0, " fps");
        });
        self.preview_store.preview.overlay.grid_size = grid as f32;
        self.ui_store.scrub_step_s = step;
        self.ui_store.snap_fps = fps as f32;
    }

    fn settings_autosave_section(&mut self, ui: &mut egui::Ui) {
        use crate::app::stores::ui_store::{MAX_AUTOSAVE_INTERVAL_S, MIN_AUTOSAVE_INTERVAL_S};
        let autosave = &mut self.ui_store.view.autosave;
        let mut enabled = autosave.enabled;
        let mut interval = autosave.interval.as_secs_f64();
        let previous = (enabled, interval);
        row(
            ui,
            "autosave",
            ("Autosave", "自动保存"),
            Some((
                "Keep a recovery copy when a document has unsaved changes.",
                "有未保存的改动时写入恢复文件。",
            )),
            |ui| {
                ui.add(Switch::new(&mut enabled).label_side(eparts::Side::Left));
            },
        );
        row(ui, "autosave-interval", ("Save interval", "保存间隔"), None, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                number(
                    ui,
                    &mut interval,
                    MIN_AUTOSAVE_INTERVAL_S..=MAX_AUTOSAVE_INTERVAL_S,
                    1.0,
                    " s",
                );
            });
        });
        autosave.enabled = enabled;
        autosave.interval = std::time::Duration::from_secs_f64(
            interval.clamp(MIN_AUTOSAVE_INTERVAL_S, MAX_AUTOSAVE_INTERVAL_S),
        );
        if (enabled, autosave.interval.as_secs_f64()) != previous {
            self.persist_autosave_prefs();
        }
    }

    fn settings_editor_section(&mut self, ui: &mut egui::Ui) {
        let mut debounce = self.ui_store.rebuild_debounce_ms as f64;
        let mut undo = self.document_store.history.undo_limit as f64;
        row(
            ui,
            "rebuild-delay",
            ("Preview rebuild delay", "预览重建延迟"),
            Some((
                "Wait this long after typing before rebuilding the preview.",
                "停止输入后，等待这段时间再重建预览。",
            )),
            |ui| {
                number(ui, &mut debounce, 0.0..=1000.0, 10.0, " ms");
            },
        );
        row(ui, "undo-limit", ("Undo history limit", "撤销历史上限"), None, |ui| {
            number(ui, &mut undo, 10.0..=1000.0, 10.0, "");
        });
        self.ui_store.rebuild_debounce_ms = debounce as u64;
        self.document_store.history.undo_limit = undo as usize;
    }
}
