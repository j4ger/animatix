use super::PropertyEdit;

#[derive(Debug, Clone)]
pub enum DocumentCommand {
    OpenFile(std::path::PathBuf),
    Save,
    Reload,
    ReloadPlugins,
    Rebuild,
    #[allow(dead_code)] // Constructed via Command directly, not through DocumentCommand
    ToggleExpandDir(std::path::PathBuf),
    SwitchWorkspace(std::path::PathBuf),
    Undo,
    Redo,
    #[allow(dead_code)] // Constructed via Command directly, not through DocumentCommand
    InsertionFromPalette,
    #[allow(dead_code)] // Constructed via Command directly, not through DocumentCommand
    FindReplaceAll,
    PropertyEdit(PropertyEdit),
}

impl From<DocumentCommand> for super::Command {
    fn from(c: DocumentCommand) -> Self {
        match c {
            DocumentCommand::OpenFile(v) => super::Command::OpenFile(v),
            DocumentCommand::Save => super::Command::Save,
            DocumentCommand::Reload => super::Command::Reload,
            DocumentCommand::ReloadPlugins => super::Command::ReloadPlugins,
            DocumentCommand::Rebuild => super::Command::Rebuild,
            DocumentCommand::ToggleExpandDir(v) => super::Command::ToggleExpandDir(v),
            DocumentCommand::SwitchWorkspace(v) => super::Command::SwitchWorkspace(v),
            DocumentCommand::Undo => super::Command::Undo,
            DocumentCommand::Redo => super::Command::Redo,
            DocumentCommand::InsertionFromPalette => super::Command::InsertionFromPalette,
            DocumentCommand::FindReplaceAll => super::Command::FindReplaceAll,
            DocumentCommand::PropertyEdit(v) => super::Command::PropertyEdit(v),
        }
    }
}
