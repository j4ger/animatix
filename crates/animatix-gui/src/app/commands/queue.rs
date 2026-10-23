use super::*;

use crate::app::document::history::UiSnapshot;

/// An entry on the undo stack. Stores the command, the source text
/// before/after the command was applied, and UI state snapshots.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub command: UndoLabel,
    pub source_before: String,
    pub source_after: String,
    pub ui_before: UiSnapshot,
    pub ui_after: UiSnapshot,
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
    #[allow(dead_code)] // Reserved for GUI-owned callers; external senders use CommandSender.
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
    #[allow(dead_code)] // Used by headless tests and reserved for queue-only consumers.
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
    #[allow(dead_code)] // External sender is exercised by headless tests and reserved for remote control.
    pub fn send_command(&self, command: Command) -> Result<(), CommandQueueError> {
        self.sender
            .send(ShellAction::Command(command))
            .map_err(|_| CommandQueueError::Closed)
    }

    /// Enqueue any shell action for the next shell frame.
    #[allow(dead_code)] // External sender is exercised by headless tests and reserved for remote control.
    pub fn send_action(&self, action: ShellAction) -> Result<(), CommandQueueError> {
        self.sender.send(action).map_err(|_| CommandQueueError::Closed)
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
