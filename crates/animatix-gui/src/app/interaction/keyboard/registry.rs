use egui::Key;

use super::saved::shortcut_hint_text;
use super::*;

impl ShortcutRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            shortcuts: Vec::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Build a registry with persisted overrides applied on top of the defaults.
    ///
    /// Overrides are keyed by the stable binding name (`Shortcut.name`). An
    /// override replaces every default entry with that name, then validates that
    /// no two distinct bindings share the same shortcut.
    pub fn with_overrides(
        overrides: &BTreeMap<String, SavedShortcut>,
    ) -> Result<Self, KeyBindingError> {
        let mut registry = Self::new();
        for (name, saved) in overrides {
            let shortcut = saved
                .to_shortcut()
                .ok_or_else(|| KeyBindingError::InvalidKey(saved.key.clone()))?;
            registry.replace_binding(name, shortcut)?;
        }
        registry.validate_conflicts()?;
        Ok(registry)
    }

    /// Stable names in the order they were registered.
    pub fn names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for (_, info) in &self.shortcuts {
            if !names.iter().any(|name: &String| name.as_str() == info.name) {
                names.push(info.name.to_string());
            }
        }
        names
    }

    /// The persisted shortcut currently bound to a stable name.
    pub fn current_saved(&self, name: &str) -> Option<SavedShortcut> {
        self.shortcuts
            .iter()
            .find(|(_, info)| info.name == name)
            .map(|(shortcut, _)| SavedShortcut::from_shortcut(shortcut))
    }

    fn replace_binding(
        &mut self,
        name: &str,
        shortcut: KeyboardShortcut,
    ) -> Result<(), KeyBindingError> {
        let mut replaced = false;
        for (existing, info) in &mut self.shortcuts {
            if info.name == name {
                *existing = shortcut;
                replaced = true;
            }
        }
        if replaced {
            Ok(())
        } else {
            Err(KeyBindingError::UnknownBinding(name.to_string()))
        }
    }

    fn validate_conflicts(&self) -> Result<(), KeyBindingError> {
        let mut seen: HashMap<KeyboardShortcut, &str> = HashMap::new();
        for (shortcut, info) in &self.shortcuts {
            if let Some(previous) = seen.insert(*shortcut, info.name)
                && previous != info.name
            {
                return Err(KeyBindingError::Conflict(
                    shortcut_hint_text(shortcut),
                    previous.to_string(),
                    info.name.to_string(),
                ));
            }
        }
        Ok(())
    }

    fn register(&mut self, shortcut: KeyboardShortcut, info: Shortcut) {
        self.shortcuts.push((shortcut, info));
    }

    fn register_defaults(&mut self) {
        self.register_file_shortcuts();
        self.register_edit_shortcuts();
        self.register_playback_shortcuts();
        self.register_view_shortcuts();
        self.register_scene_shortcuts();
        self.register_editor_sync_shortcuts();
        self.register_escape_shortcut();
        self.register_scrub_shortcuts();
        self.register_global_shortcuts();
    }

    /// File operations (TextSafe).
    fn register_file_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::S),
            Shortcut {
                name: "Save",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Save,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::R),
            Shortcut {
                name: "Rebuild",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Rebuild,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::R),
            Shortcut {
                name: "Reload",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Reload,
            },
        );
    }

    /// Undo/Redo (TextSafe).
    fn register_edit_shortcuts(&mut self) {
        // Egui's logical match also accepts an unspecified Shift modifier.
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z),
            Shortcut {
                name: "Redo",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Redo,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::Z),
            Shortcut {
                name: "Undo",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Undo,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::Y),
            Shortcut {
                name: "Redo",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Redo,
            },
        );
    }

    /// Playback (Canvas).
    fn register_playback_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Space),
            Shortcut {
                name: "Play/Pause",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::TogglePlayback,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Comma),
            Shortcut {
                name: "Prev Keyframe",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::PrevKeyframe,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Period),
            Shortcut {
                name: "Next Keyframe",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::NextKeyframe,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::SHIFT, Key::Comma),
            Shortcut {
                name: "Step Back 1 Frame",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::FrameStepBackward,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::SHIFT, Key::Period),
            Shortcut {
                name: "Step Forward 1 Frame",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::FrameStepForward,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::PageDown),
            Shortcut {
                name: "Step Next",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::StepNext,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::PageUp),
            Shortcut {
                name: "Step Previous",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::StepPrevious,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::F5),
            Shortcut {
                name: "Toggle Presentation Mode",
                scope: ShortcutScope::Global,
                action: KeyboardAction::TogglePresentationMode,
            },
        );
    }

    /// View (Canvas).
    fn register_view_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::F),
            Shortcut {
                name: "Zoom to Selection",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::ZoomToSelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::F),
            Shortcut {
                name: "Zoom to All",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::ZoomToAll,
            },
        );
    }

    /// Scene switching (Canvas).
    fn register_scene_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Num1),
            Shortcut {
                name: "Scene 1",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SelectScene(0),
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Num2),
            Shortcut {
                name: "Scene 2",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SelectScene(1),
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Num3),
            Shortcut {
                name: "Scene 3",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SelectScene(2),
            },
        );
    }

    /// Editor sync (Canvas).
    fn register_editor_sync_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Y),
            Shortcut {
                name: "Editor Sync",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::EditSync,
            },
        );
    }

    /// Escape (Canvas).
    fn register_escape_shortcut(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Escape),
            Shortcut {
                name: "Escape",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::Escape,
            },
        );
    }

    /// Review scrubbing (Canvas).
    fn register_scrub_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowLeft),
            Shortcut {
                name: "Scrub Backward",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::ScrubBackward,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowRight),
            Shortcut {
                name: "Scrub Forward",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::ScrubForward,
            },
        );
    }

    /// Global toggles (Global).
    fn register_global_shortcuts(&mut self) {
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::I),
            Shortcut {
                name: "Toggle Inspector",
                scope: ShortcutScope::Global,
                action: KeyboardAction::ToggleInspector,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::E),
            Shortcut {
                name: "Toggle Code",
                scope: ShortcutScope::Global,
                action: KeyboardAction::ToggleCode,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P),
            Shortcut {
                name: "Command Palette",
                scope: ShortcutScope::Global,
                action: KeyboardAction::OpenCommandPalette,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::F),
            Shortcut {
                name: "Find/Replace",
                scope: ShortcutScope::Global,
                action: KeyboardAction::OpenFindReplace,
            },
        );
    }

    /// Check if a shortcut was pressed and return its action, respecting the focus context.
    ///
    /// The shortcut is only consumed when its scope allows it. This matters for
    /// keys such as `Backspace`, which TextEdit needs even while a canvas scope
    /// binding for the same key is registered.
    pub fn check(&self, ctx: &Context, focus: &FocusContext) -> Option<&KeyboardAction> {
        for (shortcut, info) in &self.shortcuts {
            if !focus.can_handle(info.scope) {
                continue;
            }
            // TextEdit owns its text undo history. The shell runs before the
            // editor, so consuming these here would swallow source undo/redo.
            if focus.text_edit_focused
                && matches!(info.action, KeyboardAction::Undo | KeyboardAction::Redo)
            {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(shortcut)) {
                return Some(&info.action);
            }
        }
        None
    }

    /// Look up every registered [`KeyboardShortcut`] with the given display name.
    pub fn shortcuts_for_name(&self, name: &str) -> Vec<&KeyboardShortcut> {
        self.shortcuts
            .iter()
            .filter(|(_, info)| info.name == name)
            .map(|(shortcut, _)| shortcut)
            .collect()
    }

    /// Look up the first registered [`KeyboardShortcut`] for the given [`KeyboardAction`].
    ///
    /// Matching is performed by *discriminant* (variant-level equality) using
    /// [`std::mem::discriminant`]. This avoids requiring `KeyboardAction` to implement
    /// `Eq`/`Hash`, which it cannot because of payload variants such as
    /// `SelectScene(u8)`.
    ///
    /// Returns [`None`] when no binding exists for the given action variant.
    pub fn shortcut_for(&self, action: &KeyboardAction) -> Option<&KeyboardShortcut> {
        self.shortcuts
            .iter()
            .find(|(_, info)| {
                std::mem::discriminant(action) == std::mem::discriminant(&info.action)
            })
            .map(|(shortcut, _)| shortcut)
    }
}
