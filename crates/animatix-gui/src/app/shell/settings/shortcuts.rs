//! Keyboard-shortcut settings section.

use super::*;

impl GuiShell {
    pub(super) fn settings_shortcuts_section(
        &mut self,
        ui: &mut egui::Ui,
        sp: &crate::app::design_tokens::spatial::Spatial,
    ) {
        let theme = eparts::theme(ui);
        // ── Shortcuts ──
        layout::section_header(ui, egui_phosphor::regular::KEYBOARD, "Shortcuts", None);
        ui.add_space(sp.base.space_2);

        let recording = self.ui_store.recording_shortcut.clone();
        let captured = self.settings_shortcut_capture_prompt(ui, &theme, recording.as_ref());
        let binding_names = self.shortcut_registry.names();
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for name in binding_names {
                let current = self
                    .shortcut_registry
                    .current_saved(&name)
                    .map(|s| s.display_with(ui.ctx()))
                    .unwrap_or_default();
                let is_recording = recording.as_deref() == Some(name.as_str());
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [160.0, sp.base.row_s],
                        egui::Label::new(
                            RichText::new(&name)
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.secondary),
                        ),
                    );
                    ui.add_sized(
                        [110.0, sp.base.row_s],
                        egui::Label::new(
                            RichText::new(if is_recording {
                                "Recording…".to_string()
                            } else {
                                current
                            })
                            .monospace()
                            .size(TextRole::BodyS.size())
                            .color(theme.palette.text.primary),
                        ),
                    );
                    if is_recording {
                        if ui.add(Button::ghost("Cancel")).clicked() {
                            self.ui_store.recording_shortcut = None;
                        }
                    } else if ui.add(Button::ghost("Record")).clicked() {
                        self.ui_store.recording_shortcut = Some(name.clone());
                    }
                    let has_override = self.ui_store.shortcut_overrides.contains_key(&name);
                    if has_override && ui.add(Button::ghost("Reset")).clicked() {
                        self.ui_store.shortcut_overrides.remove(&name);
                        match ShortcutRegistry::with_overrides(&self.ui_store.shortcut_overrides) {
                            Ok(registry) => {
                                self.shortcut_registry = registry;
                                self.save_persistence();
                                self.preview_store.preview.status =
                                    format!("Shortcut '{}' reset to default", name);
                            },
                            Err(error) => {
                                self.preview_store
                                    .preview
                                    .set_status_error(format!("Shortcut reset failed: {error}"));
                            },
                        }
                    }
                });
            }
        });

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
        ui.add_space(sp.base.space_3);
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
