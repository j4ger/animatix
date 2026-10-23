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
            if let Some(previous) = seen.insert(*shortcut, info.name) {
                if previous != info.name {
                    return Err(KeyBindingError::Conflict(
                        shortcut_hint_text(shortcut),
                        previous.to_string(),
                        info.name.to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn register(&mut self, shortcut: KeyboardShortcut, info: Shortcut) {
        self.shortcuts.push((shortcut, info));
    }

    fn register_defaults(&mut self) {
        use egui::Key;

        // File operations (TextSafe)
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

        // Undo/Redo (TextSafe)
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::Z),
            Shortcut {
                name: "Undo",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Undo,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z),
            Shortcut {
                name: "Redo",
                scope: ShortcutScope::TextSafe,
                action: KeyboardAction::Redo,
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

        // Playback (Canvas)
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
        // Selection editing (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::D),
            Shortcut {
                name: "Duplicate",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::DuplicateSelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Delete),
            Shortcut {
                name: "Delete",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::DeleteSelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Backspace),
            Shortcut {
                name: "Delete",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::DeleteSelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::G),
            Shortcut {
                name: "Group",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::GroupSelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::G),
            Shortcut {
                name: "Ungroup",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::UngroupSelection,
            },
        );

        // View (Canvas)
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

        // Scene switching (Canvas)
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

        // Clipboard (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::C),
            Shortcut {
                name: "Copy",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::CopySelection,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND, Key::V),
            Shortcut {
                name: "Paste",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::PasteClipboard,
            },
        );

        // Editor sync (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Y),
            Shortcut {
                name: "Editor Sync",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::EditSync,
            },
        );

        // Insertion palette (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::SHIFT, Key::A),
            Shortcut {
                name: "Insertion Palette (Actions)",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::OpenInsertionPalette,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Slash),
            Shortcut {
                name: "Insertion Palette (Universal)",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::OpenInsertionPalette,
            },
        );

        // Escape (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::Escape),
            Shortcut {
                name: "Escape",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::Escape,
            },
        );

        // Nudge arrow keys (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowLeft),
            Shortcut {
                name: "Nudge Left",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::NudgeSelected { dx: -1.0, dy: 0.0 },
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowRight),
            Shortcut {
                name: "Nudge Right",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::NudgeSelected { dx: 1.0, dy: 0.0 },
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowUp),
            Shortcut {
                name: "Nudge Up",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::NudgeSelected { dx: 0.0, dy: -1.0 },
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::ArrowDown),
            Shortcut {
                name: "Nudge Down",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::NudgeSelected { dx: 0.0, dy: 1.0 },
            },
        );

        // Tool switching (Canvas).
        //
        // V/A follow the Adobe convention (select / direct-select) and the rest
        // are mnemonics: R rotate, S scale, G grab/move, P pivot. These are
        // secondary to the visible tool switcher in the preview header.
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::V),
            Shortcut {
                name: "Select Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetSelectTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::A),
            Shortcut {
                name: "Vertex Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetVertexTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::R),
            Shortcut {
                name: "Rotate Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetRotateTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::S),
            Shortcut {
                name: "Scale Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetScaleTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::G),
            Shortcut {
                name: "Move Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetMoveTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::P),
            Shortcut {
                name: "Pivot Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetPivotTool,
            },
        );

        // Toggle Inspector (Global)
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::I),
            Shortcut {
                name: "Toggle Inspector",
                scope: ShortcutScope::Global,
                action: KeyboardAction::ToggleInspector,
            },
        );

        // Toggle Code (Global) — the Inspector and Code share one detail region.
        self.register(
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::E),
            Shortcut {
                name: "Toggle Code",
                scope: ShortcutScope::Global,
                action: KeyboardAction::ToggleCode,
            },
        );

        // Palettes (Global — they check their own open state)
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
    /// `NudgeSelected { dx: f32, dy: f32 }` (`f32` is not `Eq`/`Hash`).
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
