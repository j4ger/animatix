use super::{Command, ShellAction};

impl From<Command> for ShellAction {
    fn from(c: Command) -> Self {
        ShellAction::Command(c)
    }
}
