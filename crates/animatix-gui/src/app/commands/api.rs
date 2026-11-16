use super::{Command, ShellAction};

impl From<Command> for ShellAction {
    fn from(command: Command) -> Self {
        Self::Command(command)
    }
}
