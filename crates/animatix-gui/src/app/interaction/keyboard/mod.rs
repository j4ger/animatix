use std::collections::{BTreeMap, HashMap};

use egui::{Context, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

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
    FrameStepForward,
    FrameStepBackward,
    StepNext,
    StepPrevious,
    TogglePresentationMode,

    // Actor editing
    DuplicateSelection,
    DeleteSelection,
    GroupSelection,
    UngroupSelection,

    // View
    ZoomToSelection,
    ZoomToAll,
    ToggleInspector,
    ToggleCode,
    OpenCommandPalette,
    OpenFindReplace,

    // Tool switching
    SetSelectTool,
    SetMoveTool,
    SetScaleTool,
    SetRotateTool,
    SetVertexTool,
    SetPivotTool,

    // Nudge (arrow keys with context-dependent step size)
    NudgeSelected { dx: f32, dy: f32 },

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
    /// True when an egui `TextEdit` currently owns keyboard focus.
    pub text_edit_focused: bool,
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
                !self.text_edit_focused
                    && !self.inline_edit_active
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

mod hints;
mod registry;
mod saved;
#[cfg(test)]
mod tests;

pub use hints::{shortcut_hints_for_name, tooltip_with_shortcut};
pub(crate) use saved::saved_shortcut_from_key;
pub use saved::{KeyBindingError, SavedShortcut};
