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
                copy(ui, "Recording…", "正在录入…")
            } else {
                &current
            };
            page::row_labels(ui, &name, &name, Some(hint), |ui| {
                let title = if is_recording {
                    copy(ui, "Cancel", "取消")
                } else {
                    copy(ui, "Rebind", "重设")
                };
                if ui.add(Button::ghost(title)).clicked() {
                    self.ui_store.recording_shortcut = if is_recording {
                        None
                    } else {
                        Some(name.clone())
                    };
                }
                let reset = copy(ui, "Reset", "恢复默认");
                if self.ui_store.shortcut_overrides.contains_key(&name)
                    && ui.add(Button::ghost(reset)).clicked()
                {
                    self.ui_store.shortcut_overrides.remove(&name);
                    match ShortcutRegistry::with_overrides(&self.ui_store.shortcut_overrides) {
                        Ok(registry) => {
                            self.shortcut_registry = registry;
                            self.save_persistence();
                            self.preview_store.preview.status =
                                format!("Shortcut '{}' reset to default", name);
                        },
                        Err(error) => self
                            .preview_store
                            .preview
                            .set_status_error(format!("Shortcut reset failed: {error}")),
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
                    self.preview_store.preview.status =
                        format!("Shortcut '{}' set to {}", name, saved.display_with(ui.ctx()));
                },
                Err(error) => {
                    self.ui_store.recording_shortcut = None;
                    self.preview_store
                        .preview
                        .set_status_error(format!("Shortcut update failed: {error}"));
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
            RichText::new(format!("Press a key for '{name}'…"))
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
