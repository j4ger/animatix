use egui::{Context, KeyboardShortcut, Modifiers};

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
    pub wants_keyboard: bool,
    pub has_selection: bool,
    pub drag_active: bool,
    pub inline_edit_active: bool,
    pub command_palette_open: bool,
    pub find_replace_open: bool,
    pub unsaved_dialog_open: bool,
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

        // Tool switching (Canvas)
        self.register(
            KeyboardShortcut::new(Modifiers::NONE, Key::V),
            Shortcut {
                name: "Move Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetMoveTool,
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
            KeyboardShortcut::new(Modifiers::NONE, Key::R),
            Shortcut {
                name: "Rotate Tool",
                scope: ShortcutScope::Canvas,
                action: KeyboardAction::SetRotateTool,
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
}
