//! Keyboard-shortcut settings section.

use super::*;

impl GuiShell {
    pub(super) fn settings_shortcuts_section(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        // ── Shortcuts ──

        let recording = self.ui_store.recording_shortcut.clone();
        let captured = self.settings_shortcut_capture_prompt(ui, &theme, recording.as_ref());
        for name in self.shortcut_registry.names() {
            let current = self
                .shortcut_registry
                .current_saved(&name)
                .map(|shortcut| shortcut.display_with(ui.ctx()))
                .unwrap_or_default();
            let is_recording = recording.as_deref() == Some(name.as_str());
            let hint = if is_recording {
                text(ui, "gui-shortcut-recording")
            } else {
                current
            };
            let label = shortcut_label(ui, &name);
            page::row_labels(ui, &name, &label, Some(&hint), |ui| {
                let title = if is_recording {
                    text(ui, "gui-action-cancel")
                } else {
                    text(ui, "gui-shortcut-rebind")
                };
                if ui.add(Button::ghost(title)).clicked() {
                    self.ui_store.recording_shortcut = if is_recording {
                        None
                    } else {
                        Some(name.clone())
                    };
                }
                let reset = text(ui, "gui-shortcut-reset");
                if self.ui_store.shortcut_overrides.contains_key(&name)
                    && ui.add(Button::ghost(reset)).clicked()
                {
                    self.ui_store.shortcut_overrides.remove(&name);
                    match ShortcutRegistry::with_overrides(&self.ui_store.shortcut_overrides) {
                        Ok(registry) => {
                            self.shortcut_registry = registry;
                            self.save_persistence();
                            self.preview_store.preview.status =
                                crate::app::i18n::StatusText::message(
                                    "gui-shortcut-reset-success",
                                    [("name", label.clone())],
                                );
                        },
                        Err(error) => self.preview_store.preview.set_status_error(
                            crate::app::i18n::StatusText::message(
                                "gui-shortcut-reset-failed",
                                [("detail", error.to_string())],
                            ),
                        ),
                    }
                }
            });
        }

        if let Some((name, saved)) = captured {
            let mut overrides = self.ui_store.shortcut_overrides.clone();
            overrides.insert(name.clone(), saved.clone());
            match ShortcutRegistry::with_overrides(&overrides) {
                Ok(registry) => {
                    self.shortcut_registry = registry;
                    self.ui_store.shortcut_overrides = overrides;
                    self.ui_store.recording_shortcut = None;
                    self.save_persistence();
                    self.preview_store.preview.status = crate::app::i18n::StatusText::message(
                        "gui-shortcut-update-success",
                        [
                            ("name", shortcut_label(ui, &name)),
                            ("key", saved.display_with(ui.ctx())),
                        ],
                    );
                },
                Err(error) => {
                    self.ui_store.recording_shortcut = None;
                    self.preview_store.preview.set_status_error(
                        crate::app::i18n::StatusText::message(
                            "gui-shortcut-update-failed",
                            [("detail", error.to_string())],
                        ),
                    );
                },
            }
        }
    }

    /// Show the "press a key" prompt while recording and return the captured
    /// binding, if any.
    fn settings_shortcut_capture_prompt(
        &self,
        ui: &mut egui::Ui,
        theme: &eparts::Theme,
        recording: Option<&String>,
    ) -> Option<(String, crate::app::interaction::keyboard::SavedShortcut)> {
        let name = recording?;
        ui.label(
            RichText::new(crate::app::i18n::render(
                ui,
                "gui-shortcut-press-key",
                &[("name", shortcut_label(ui, name))],
            ))
            .size(TextRole::BodyS.size())
            .color(theme.palette.accent.primary),
        );
        let captured_key = ui.input(|i| {
            i.events.iter().find_map(|event| {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    saved_shortcut_from_key(*key, *modifiers)
                } else {
                    None
                }
            })
        });
        captured_key.map(|saved| (name.clone(), saved))
    }
}

fn shortcut_label(ui: &egui::Ui, name: &str) -> String {
    let message = match name {
        "Command Palette" => "gui-shortcut-command-palette",
        "Editor Sync" => "gui-shortcut-editor-sync",
        "Escape" => "gui-shortcut-escape",
        "Find/Replace" => "gui-shortcut-find-replace",
        "Next Keyframe" => "gui-shortcut-next-keyframe",
        "Play/Pause" => "gui-shortcut-play-pause",
        "Prev Keyframe" => "gui-shortcut-prev-keyframe",
        "Scene 1" => "gui-shortcut-scene-1",
        "Scene 2" => "gui-shortcut-scene-2",
        "Scene 3" => "gui-shortcut-scene-3",
        "Scrub Backward" => "gui-shortcut-scrub-backward",
        "Scrub Forward" => "gui-shortcut-scrub-forward",
        "Step Back 1 Frame" => "gui-shortcut-step-back-1-frame",
        "Step Forward 1 Frame" => "gui-shortcut-step-forward-1-frame",
        "Zoom to All" => "gui-shortcut-zoom-to-all",
        "Save" => "gui-command-save",
        "Rebuild" => "gui-command-rebuild",
        "Redo" => "gui-command-redo",
        "Reload" => "gui-command-reload",
        "Undo" => "gui-command-undo",
        "Step Next" => "gui-command-step-next",
        "Step Previous" => "gui-command-step-previous",
        "Toggle Code" => "gui-command-code",
        "Toggle Inspector" => "gui-command-inspector",
        "Toggle Presentation Mode" => "gui-command-presentation",
        "Zoom to Selection" => "gui-command-zoom-selection",
        _ => return name.to_owned(),
    };
    text(ui, message)
}
