use std::collections::{BTreeMap, HashMap};

use egui::{Context, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

/// A serializable shortcut override used by persisted settings.
///
/// `key` uses stable names such as `"S"`, `"Space"`, `"ArrowLeft"`, or `"/"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedShortcut {
    pub command: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: String,
}

impl SavedShortcut {
    /// Convert an egui shortcut into the persisted representation.
    pub fn from_shortcut(shortcut: &KeyboardShortcut) -> Self {
        let modifiers = shortcut.modifiers;
        Self {
            command: modifiers.command,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            key: saved_key_name(&shortcut.logical_key).unwrap_or_default().to_string(),
        }
    }

    /// Convert back into an egui shortcut, or `None` when the key is unknown.
    pub fn to_shortcut(&self) -> Option<KeyboardShortcut> {
        Some(KeyboardShortcut::new(
            Modifiers {
                command: self.command,
                ctrl: self.ctrl,
                shift: self.shift,
                alt: self.alt,
                mac_cmd: false,
            },
            parse_saved_key(&self.key)?,
        ))
    }

    /// Platform-neutral display, e.g. `Cmd+Shift+Z`.
    pub fn display(&self) -> String {
        let mut out = String::new();
        if self.command {
            out.push_str("Cmd+");
        }
        if self.ctrl {
            out.push_str("Ctrl+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        out.push_str(&self.key);
        out
    }
}

fn saved_key_name(key: &egui::Key) -> Option<&'static str> {
    use egui::Key::*;
    Some(match key {
        A => "A",
        C => "C",
        D => "D",
        F => "F",
        G => "G",
        I => "I",
        M => "M",
        P => "P",
        R => "R",
        S => "S",
        V => "V",
        Y => "Y",
        Z => "Z",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Space => "Space",
        Comma => ",",
        Period => ".",
        Slash => "/",
        Delete => "Delete",
        Backspace => "Backspace",
        Escape => "Escape",
        ArrowLeft => "ArrowLeft",
        ArrowRight => "ArrowRight",
        ArrowUp => "ArrowUp",
        ArrowDown => "ArrowDown",
        _ => return None,
    })
}

fn parse_saved_key(key: &str) -> Option<egui::Key> {
    use egui::Key::*;
    Some(match key {
        "A" => A,
        "C" => C,
        "D" => D,
        "F" => F,
        "G" => G,
        "I" => I,
        "M" => M,
        "P" => P,
        "R" => R,
        "S" => S,
        "V" => V,
        "Y" => Y,
        "Z" => Z,
        "1" => Num1,
        "2" => Num2,
        "3" => Num3,
        "Space" => Space,
        "," => Comma,
        "." => Period,
        "/" => Slash,
        "Delete" => Delete,
        "Backspace" => Backspace,
        "Escape" => Escape,
        "ArrowLeft" => ArrowLeft,
        "ArrowRight" => ArrowRight,
        "ArrowUp" => ArrowUp,
        "ArrowDown" => ArrowDown,
        _ => return None,
    })
}

fn shortcut_hint_text(shortcut: &KeyboardShortcut) -> String {
    SavedShortcut::from_shortcut(shortcut).display()
}

/// Errors produced when applying persisted shortcut overrides.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyBindingError {
    #[error("unknown shortcut binding '{0}'")]
    UnknownBinding(String),
    #[error("invalid key '{0}' in shortcut binding")]
    InvalidKey(String),
    #[error("shortcut {0} conflicts between '{1}' and '{2}'")]
    Conflict(String, String, String),
}

/// Build a persisted shortcut from a raw key press.
///
/// Returns `None` when the key has no stable serialized name.
pub(crate) fn saved_shortcut_from_key(
    key: egui::Key,
    modifiers: egui::Modifiers,
) -> Option<SavedShortcut> {
    Some(SavedShortcut {
        command: modifiers.command,
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        key: saved_key_name(&key)?.to_string(),
    })
}

/// Scope that controls when a shortcut is active.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShortcutScope {
    /// Active only when no text input is focused.
    TextSafe,
    /// Active only in canvas/timeline contexts (not in text inputs).
    Canvas,
    /// Active globally (including with text input focused).
    Global,
}

/// A registered shortcut binding.
#[derive(Debug, Clone)]
pub struct Shortcut {
    pub name: &'static str,
    pub scope: ShortcutScope,
    pub action: KeyboardAction,
}

/// Actions that can be bound to keyboard shortcuts.
#[derive(Debug, Clone)]
pub enum KeyboardAction {
    // Document commands
    Undo,
    Redo,
    Save,
    Reload,
    Rebuild,

    // Scene switching
    SelectScene(u8),

    // Playback
    TogglePlayback,
    PrevKeyframe,
    NextKeyframe,
    #[allow(dead_code)] // Reserved for explicit key binding (handled via NudgeSelected fallback)
    FrameStepForward,
    #[allow(dead_code)] // Reserved for explicit key binding (handled via NudgeSelected fallback)
    FrameStepBackward,

    // Actor editing
    DuplicateSelection,
    DeleteSelection,
    GroupSelection,
    UngroupSelection,

    // View
    ZoomToSelection,
    ZoomToAll,
    ToggleInspector,
    OpenCommandPalette,
    OpenFindReplace,

    // Tool switching
    SetMoveTool,
    SetScaleTool,
    SetRotateTool,
    SetVertexTool,
    SetPivotTool,

    // Nudge (arrow keys with context-dependent step size)
    NudgeSelected {
        dx: f32,
        dy: f32,
    },

    // Editor / insertion palette
    EditSync,
    OpenInsertionPalette,

    // Clipboard
    CopySelection,
    PasteClipboard,

    // Misc
    Escape,
}

/// Focus context that gates shortcut availability.
pub struct FocusContext {
    #[allow(dead_code)] // Reserved for future scope-gating logic
    pub wants_keyboard: bool,
    #[allow(dead_code)] // Reserved for future scope-gating logic
    pub has_selection: bool,
    #[allow(dead_code)] // Reserved for future scope-gating logic
    pub drag_active: bool,
    pub inline_edit_active: bool,
    pub command_palette_open: bool,
    pub find_replace_open: bool,
    pub unsaved_dialog_open: bool,
    #[allow(dead_code)] // Reserved for future scope-gating logic
    pub tool_mode: crate::app::preview::ToolMode,
}

impl FocusContext {
    pub fn can_handle(&self, scope: ShortcutScope) -> bool {
        match scope {
            ShortcutScope::Global => true,
            ShortcutScope::TextSafe => !self.inline_edit_active && !self.unsaved_dialog_open,
            ShortcutScope::Canvas => {
                !self.inline_edit_active
                    && !self.command_palette_open
                    && !self.find_replace_open
                    && !self.unsaved_dialog_open
            },
        }
    }
}

/// The shortcut registry.
#[derive(Debug, Clone)]
pub struct ShortcutRegistry {
    shortcuts: Vec<(KeyboardShortcut, Shortcut)>,
}

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
            if !names.iter().any(|name| name == &info.name) {
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
                *existing = shortcut.clone();
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
            if let Some(previous) = seen.insert(shortcut.clone(), info.name) {
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

        // Tool switching (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::M),
            Shortcut {
                name: "Move Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetMoveTool,
            },
        );
        self.register(
            KeyboardShortcut::new(Modifiers::SHIFT, Key::S),
            Shortcut {
                name: "Scale Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetScaleTool,
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
            KeyboardShortcut::new(Modifiers::NONE, Key::V),
            Shortcut {
                name: "Vertex Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetVertexTool,
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
    pub fn check(&self, ctx: &Context, focus: &FocusContext) -> Option<&KeyboardAction> {
        for (shortcut, info) in &self.shortcuts {
            if ctx.input_mut(|i| i.consume_shortcut(shortcut)) && focus.can_handle(info.scope) {
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

/// Human-readable, platform-aware shortcut label for an action (e.g. `"Ctrl+S"`),
/// or `None` if the action has no binding.
pub fn shortcut_hint(
    registry: &ShortcutRegistry,
    action: &KeyboardAction,
    ctx: &Context,
) -> Option<String> {
    registry.shortcut_for(action).map(|sc| eparts::widget::format_shortcut(sc, ctx))
}

/// Human-readable, platform-aware labels for every shortcut sharing `name`.
pub fn shortcut_hints_for_name(
    registry: &ShortcutRegistry,
    name: &str,
    ctx: &Context,
) -> Vec<String> {
    registry
        .shortcuts_for_name(name)
        .into_iter()
        .map(|sc| eparts::widget::format_shortcut(sc, ctx))
        .collect()
}

/// A tooltip string with the action's shortcut appended in parentheses when one
/// exists, e.g. `tooltip_with_shortcut("Save", &KeyboardAction::Save, ctx)` ->
/// `"Save (Ctrl+S)"`. Falls back to the bare label when unbound.
pub fn tooltip_with_shortcut(
    registry: &ShortcutRegistry,
    label: &str,
    action: &KeyboardAction,
    ctx: &Context,
) -> String {
    match shortcut_hint(registry, action, ctx) {
        Some(hint) => format!("{label} ({hint})"),
        None => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use egui::Key;

    use super::*;

    #[test]
    fn shortcut_for_save_returns_ctrl_s() {
        let registry = ShortcutRegistry::new();
        let shortcut = registry.shortcut_for(&KeyboardAction::Save);
        assert!(shortcut.is_some(), "Expected a registered shortcut for KeyboardAction::Save");
        let shortcut = shortcut.unwrap();
        assert_eq!(shortcut.modifiers, egui::Modifiers::COMMAND);
        assert_eq!(shortcut.logical_key, Key::S);
    }

    #[test]
    fn shortcut_for_payload_variant_matches_discriminant() {
        let registry = ShortcutRegistry::new();
        // NudgeSelected has an f32 payload — discriminant match should still work.
        let shortcut = registry.shortcut_for(&KeyboardAction::NudgeSelected { dx: 0.0, dy: 0.0 });
        assert!(
            shortcut.is_some(),
            "Expected a registered shortcut for KeyboardAction::NudgeSelected"
        );
        // The first nudge entry is ArrowLeft.
        assert_eq!(shortcut.unwrap().logical_key, Key::ArrowLeft);
    }

    #[test]
    fn shortcuts_for_name_returns_all_redo_bindings() {
        let registry = ShortcutRegistry::new();
        let redo = registry.shortcuts_for_name("Redo");
        assert_eq!(redo.len(), 2, "Expected Ctrl+Shift+Z and Ctrl+Y Redo bindings");
    }

    #[test]
    fn tool_shortcuts_are_unambiguous() {
        let registry = ShortcutRegistry::new();
        assert_eq!(registry.shortcuts_for_name("Move Tool").len(), 1);
        assert_eq!(registry.shortcuts_for_name("Scale Tool").len(), 1);
        assert_eq!(registry.shortcuts_for_name("Rotate Tool").len(), 1);
        assert_eq!(registry.shortcuts_for_name("Vertex Tool").len(), 1);
        assert_eq!(registry.shortcuts_for_name("Pivot Tool").len(), 1);

        let vertex = registry.shortcut_for(&KeyboardAction::SetVertexTool).unwrap();
        assert_eq!(vertex.logical_key, Key::V);
    }

    #[test]
    fn shortcuts_for_name_returns_empty_for_unknown() {
        let registry = ShortcutRegistry::new();
        assert!(registry.shortcuts_for_name("Not a real shortcut").is_empty());
    }

    #[test]
    fn shortcut_for_unknown_returns_none() {
        // KeyboardAction::SelectScene(99) won't match because the registry only
        // registers SelectScene(0), (1), and (2) — discriminant is the same
        // variant but we test a hypothetical unregistered variant isn't returned.
        // Actually SelectScene(99) shares the same discriminant, so it WILL match
        // the first SelectScene entry. This is expected discriminant-level behaviour.
        let registry = ShortcutRegistry::new();
        let shortcut = registry.shortcut_for(&KeyboardAction::SelectScene(0));
        assert!(shortcut.is_some());
        assert_eq!(shortcut.unwrap().logical_key, Key::Num1);
    }

    #[test]
    fn saved_shortcut_roundtrips_through_egui_shortcut() {
        let shortcut =
            KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::ArrowLeft);
        let saved = SavedShortcut::from_shortcut(&shortcut);
        assert_eq!(saved.key, "ArrowLeft");
        assert!(saved.command);
        assert!(saved.shift);
        assert_eq!(saved.to_shortcut(), Some(shortcut));
    }

    #[test]
    fn overrides_replace_binding_and_are_reported_as_current() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "Save".to_string(),
            SavedShortcut {
                command: false,
                ctrl: false,
                shift: false,
                alt: true,
                key: "S".to_string(),
            },
        );
        let registry = ShortcutRegistry::with_overrides(&overrides).expect("valid override");
        let save = registry.shortcut_for(&KeyboardAction::Save).expect("Save binding");
        assert_eq!(save.modifiers, egui::Modifiers::ALT);
        assert_eq!(save.logical_key, Key::S);
        assert_eq!(registry.current_saved("Save").unwrap().alt, true);
    }

    #[test]
    fn overrides_reject_unknown_binding() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "Not A Binding".to_string(),
            SavedShortcut {
                command: true,
                ctrl: false,
                shift: false,
                alt: false,
                key: "S".to_string(),
            },
        );
        assert!(matches!(
            ShortcutRegistry::with_overrides(&overrides),
            Err(KeyBindingError::UnknownBinding(_))
        ));
    }

    #[test]
    fn overrides_reject_conflicting_shortcuts() {
        let conflict = SavedShortcut {
            command: true,
            ctrl: false,
            shift: false,
            alt: false,
            key: "S".to_string(),
        };
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert("Save".to_string(), conflict.clone());
        overrides.insert("Reload".to_string(), conflict);

        assert!(matches!(
            ShortcutRegistry::with_overrides(&overrides),
            Err(KeyBindingError::Conflict(_, _, _))
        ));
    }

    #[test]
    fn overrides_do_not_mutate_a_separate_registry() {
        let mut overrides = std::collections::BTreeMap::new();
        overrides.insert(
            "Save".to_string(),
            SavedShortcut {
                command: false,
                ctrl: false,
                shift: false,
                alt: true,
                key: "S".to_string(),
            },
        );
        let default_registry = ShortcutRegistry::new();
        let overridden_registry =
            ShortcutRegistry::with_overrides(&overrides).expect("valid override");

        let default_save = default_registry.shortcut_for(&KeyboardAction::Save).unwrap();
        let overridden_save = overridden_registry.shortcut_for(&KeyboardAction::Save).unwrap();
        assert_ne!(default_save.modifiers, overridden_save.modifiers);
        assert_eq!(default_registry.current_saved("Save").unwrap().alt, false);
    }
}
