use super::*;

use crate::app::document::history::UiSnapshot;

/// An entry on the undo stack. Stores the executed command, its display
/// label, the source text before/after the command was applied, and UI state
/// snapshots.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub command: Command,
    /// Human-readable description derived from [`Command::label`] when the
    /// entry was recorded, so undo/redo status text does not need to match on
    /// every command variant.
    pub label: String,
    pub source_before: String,
    pub source_after: String,
    pub ui_before: UiSnapshot,
    pub ui_after: UiSnapshot,
}

impl UndoEntry {
    pub fn new(
        command: Command,
        source_before: String,
        source_after: String,
        ui_before: UiSnapshot,
        ui_after: UiSnapshot,
    ) -> Self {
        let label = command.label();
        Self {
            command,
            label,
            source_before,
            source_after,
            ui_before,
            ui_after,
        }
    }
}

impl Command {
    /// Display label for undo/redo history and status text.
    ///
    /// The label is diagnostic only: undo and redo restore the recorded source
    /// text, they never re-execute the command.
    pub fn label(&self) -> String {
        match self {
            // ── Document / File ──────────────────────────────────────
            Self::OpenFile(path) => format!("Open {}", path.display()),
            Self::Save => "Save".to_string(),
            Self::Reload => "Reload".to_string(),
            Self::ReloadPlugins => "Reload plugins".to_string(),
            Self::Rebuild => "Rebuild".to_string(),
            Self::ToggleExpandDir(path) => format!("Toggle {}", path.display()),
            Self::SwitchWorkspace(path) => format!("Switch workspace to {}", path.display()),

            // ── Playback ─────────────────────────────────────────────
            Self::TogglePlayback => "Toggle playback".to_string(),
            Self::ScrubTo(time_s) => format!("Scrub to {time_s:.2}s"),
            Self::PrevKeyframe => "Previous keyframe".to_string(),
            Self::NextKeyframe => "Next keyframe".to_string(),
            Self::FrameStepForward => "Step forward one frame".to_string(),
            Self::FrameStepBackward => "Step back one frame".to_string(),
            Self::StepNext => "Next step".to_string(),
            Self::StepPrevious => "Previous step".to_string(),
            Self::TogglePresentationMode => "Toggle presentation mode".to_string(),

            // ── Scene ────────────────────────────────────────────────
            Self::SelectScene(scene) => format!("Select scene '{scene}'"),
            Self::ReorderScenes(_) => "Reorder scenes".to_string(),
            Self::SetTransition { from_scene, .. } => {
                format!("Set transition on '{from_scene}'")
            },
            Self::SetPlayTarget { from_scene, .. } => {
                format!("Set play target on '{from_scene}'")
            },
            Self::SetSceneDuration { scene, .. } => {
                format!("Set duration for scene '{scene}'")
            },
            Self::DuplicateScene(scene) => format!("Duplicate scene '{scene}'"),
            Self::DeleteScene(scene) => format!("Delete scene '{scene}'"),
            Self::AddScene(scene) => format!("Add scene '{scene}'"),

            // ── Actor ────────────────────────────────────────────────
            Self::CreateActor { ty, label, .. } => format!("Create {ty} '{label}'"),
            Self::RenameActor {
                old_label,
                new_label,
            } => {
                format!("Rename '{old_label}' to '{new_label}'")
            },
            Self::DuplicateActor(actor) => format!("Duplicate actor '{actor}'"),
            Self::DuplicateSelectedActors => "Duplicate selected actors".to_string(),
            Self::DeleteSelectedActors => "Delete selected actors".to_string(),
            Self::ReparentActor { actor, .. } => format!("Reparent '{actor}'"),
            Self::ExtractScene { new_scene_name, .. } => {
                format!("Extract scene '{new_scene_name}'")
            },
            Self::MoveToScene { target_scene, .. } => {
                format!("Move actors to scene '{target_scene}'")
            },
            Self::ToggleActorVisibility(actor) => format!("Toggle visibility of '{actor}'"),
            Self::ToggleActorLock(actor) => format!("Toggle lock of '{actor}'"),

            // ── Property / Inspector ─────────────────────────────────
            Self::PropertyEdit(edit) => format!("Edit {}.{}", edit.actor, edit.property),
            Self::DetachCallout { actor, .. } => format!("Detach callout '{actor}'"),
            Self::SetConfigProperty { key, .. } => format!("Set config '{key}'"),

            // ── Keyframe ─────────────────────────────────────────────
            Self::SetKeyframeEasing {
                actor, property, ..
            } => {
                format!("Set easing for {actor}.{property}")
            },
            Self::DeleteKeyframe {
                actor, property, ..
            } => {
                format!("Delete keyframe {actor}.{property}")
            },
            Self::SetSelectedKeyframes(ids) => format!("Select {} keyframes", ids.len()),
            Self::MoveKeyframe {
                actor, property, ..
            } => {
                format!("Move keyframe {actor}.{property}")
            },
            Self::SetKeyframeValue {
                actor, property, ..
            } => {
                format!("Set keyframe {actor}.{property}")
            },
            Self::MoveKeyframes(specs) => format!("Move {} keyframes", specs.len()),
            Self::DeleteKeyframes(ids) => format!("Delete {} keyframes", ids.len()),
            Self::ResizeAction { verb, .. } => format!("Resize {verb}"),

            // ── Editor sync modes ────────────────────────────────────
            Self::ToggleEditorSync => "Toggle editor sync".to_string(),
            Self::EditorChanged => "Editor changed".to_string(),

            // ── Clipboard ────────────────────────────────────────────
            Self::PasteActors => "Paste actors".to_string(),

            // ── Undo / Redo ──────────────────────────────────────────
            Self::Undo => "Undo".to_string(),
            Self::Redo => "Redo".to_string(),

            // ── Navigation ───────────────────────────────────────────
            Self::ScrollToLine(line, column) => format!("Scroll to {line}:{column}"),

            // ── Viewport ─────────────────────────────────────────────
            Self::ZoomToSelection => "Zoom to selection".to_string(),
            Self::ZoomToAll => "Zoom to fit".to_string(),

            // ── Alignment ────────────────────────────────────────────
            Self::AlignActors(alignment) => format!("Align actors ({alignment:?})"),
            Self::DistributeActors(axis) => format!("Distribute actors ({axis:?})"),

            // ── Group / Ungroup ──────────────────────────────────────
            Self::GroupSelectedActors => "Group selected actors".to_string(),
            Self::UngroupSelectedActors => "Ungroup selected actors".to_string(),

            // ── Insertion Palette ────────────────────────────────────
            Self::InsertionFromPalette => "Insert from palette".to_string(),

            // ── Find / Replace ───────────────────────────────────────
            Self::FindReplaceAll => "Replace all".to_string(),
        }
    }
}

/// Per-frame action queue consumed by the shell.
pub type ActionQueue = VecDeque<ShellAction>;

// =========================================================================
// ── External command queue ──────────────────────────────────────────────
// =========================================================================

/// App-owned queue for commands submitted outside a frame's UI callbacks.
///
/// The shell drains this into `ActionQueue` at the start of each frame, so
/// integration tests and future remote-control channels can enqueue the same
/// `Command` values the GUI already executes without touching egui or the
/// display.
#[derive(Debug)]
pub struct CommandQueue {
    sender: mpsc::Sender<ShellAction>,
    receiver: mpsc::Receiver<ShellAction>,
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandQueue {
    /// Create an empty command queue.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self { sender, receiver }
    }

    /// Return a cloneable sender for this queue.
    pub fn sender(&self) -> CommandSender {
        CommandSender {
            sender: self.sender.clone(),
        }
    }

    /// Enqueue a shell action from the GUI's own code.
    #[cfg(test)]
    pub fn push(&mut self, action: impl Into<ShellAction>) {
        let _ = self.sender.send(action.into());
    }

    /// Append all currently queued actions to the per-frame action queue.
    pub fn drain_into(&mut self, out: &mut ActionQueue) {
        out.extend(self.receiver.try_iter());
    }

    /// Execute each currently queued action through the provided handler.
    ///
    /// This is primarily useful for headless tests and for callers that want to
    /// consume commands without coupling to the shell's frame action queue.
    #[cfg(test)]
    pub fn drain_with(&mut self, mut handle: impl FnMut(ShellAction)) {
        for action in self.receiver.try_iter() {
            handle(action);
        }
    }
}

/// Cloneable sender that enqueues commands into an owned [`CommandQueue`].
#[derive(Debug, Clone)]
pub struct CommandSender {
    sender: mpsc::Sender<ShellAction>,
}

impl CommandSender {
    /// Enqueue a domain command for the next shell frame.
    pub fn send_command(&self, command: Command) -> Result<(), CommandQueueError> {
        self.sender
            .send(ShellAction::Command(command))
            .map_err(|_| CommandQueueError::Closed)
    }
}

/// Error returned when a command sender's queue has been dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandQueueError {
    /// The owning shell no longer exists.
    Closed,
}

impl std::fmt::Display for CommandQueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => f.write_str("command queue is closed"),
        }
    }
}

impl std::error::Error for CommandQueueError {}
