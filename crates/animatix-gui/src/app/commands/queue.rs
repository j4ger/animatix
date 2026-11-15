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
            // ── Editor sync modes ────────────────────────────────────
            Self::ToggleEditorSync => "Toggle editor sync".to_string(),
            Self::EditorChanged => "Editor changed".to_string(),

            // ── Undo / Redo ──────────────────────────────────────────
            Self::Undo => "Undo".to_string(),
            Self::Redo => "Redo".to_string(),

            // ── Navigation ───────────────────────────────────────────
            Self::ScrollToLine(line, column) => format!("Scroll to {line}:{column}"),

            // ── Viewport ─────────────────────────────────────────────
            Self::ZoomToSelection => "Zoom to selection".to_string(),
            Self::ZoomToAll => "Zoom to fit".to_string(),

            // ── Find / Replace ───────────────────────────────────────
            Self::FindReplaceAll => "Replace all".to_string(),
        }
    }
}

/// Per-frame action queue consumed by the shell.
pub type ActionQueue = VecDeque<ShellAction>;

// ── External command queue ──────────────────────────────────────────────

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
