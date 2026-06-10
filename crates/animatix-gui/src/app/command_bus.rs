//! Typed command/event bus for panel-to-shell communication.
//!
//! Panels emit typed actions through the bus instead of mutating stores
//! directly. The shell drains the bus after each frame and dispatches actions.

use std::collections::VecDeque;

use crate::app::commands::ShellAction;

/// A typed command bus that collects actions from panels and drains them
/// in the shell's frame pipeline.
#[allow(dead_code)] // CommandBus will replace ActionQueue once panels migrate to view models (R7).
pub struct CommandBus {
    queue: VecDeque<ShellAction>,
}

#[allow(dead_code)] // CommandBus will replace ActionQueue once panels migrate to view models (R7).
impl CommandBus {
    pub fn new() -> Self {
        Self { queue: VecDeque::new() }
    }

    /// Emit an action to be processed by the shell.
    pub fn emit(&mut self, action: impl Into<ShellAction>) {
        self.queue.push_back(action.into());
    }

    /// Drain all pending actions for processing.
    pub fn drain(&mut self) -> Vec<ShellAction> {
        self.queue.drain(..).collect()
    }

    /// Returns true if there are pending actions.
    pub fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }
}
