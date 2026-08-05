#[derive(Debug, Clone)]
pub enum ViewCommand {
    ScrollToLine(usize, usize),
    ZoomToSelection,
    ZoomToAll,
}

impl From<ViewCommand> for super::Command {
    fn from(c: ViewCommand) -> Self {
        match c {
            ViewCommand::ScrollToLine(a, b) => super::Command::ScrollToLine(a, b),
            ViewCommand::ZoomToSelection => super::Command::ZoomToSelection,
            ViewCommand::ZoomToAll => super::Command::ZoomToAll,
        }
    }
}
