use super::{Command, ShellAction};

impl From<Command> for ShellAction {
    fn from(c: Command) -> Self {
        ShellAction::Command(c)
    }
}

impl Command {
    /// Exhaustive boundary: review UI can navigate and edit source text, never
    /// reverse-engineer runtime entities into AST writes.
    pub(crate) fn allowed_in_workbench(&self) -> bool {
        match self {
            Self::OpenFile(_)
            | Self::Save
            | Self::Reload
            | Self::ReloadPlugins
            | Self::Rebuild
            | Self::ToggleExpandDir(_)
            | Self::SwitchWorkspace(_)
            | Self::TogglePlayback
            | Self::ScrubTo(_)
            | Self::PrevKeyframe
            | Self::NextKeyframe
            | Self::FrameStepForward
            | Self::FrameStepBackward
            | Self::StepNext
            | Self::StepPrevious
            | Self::TogglePresentationMode
            | Self::SelectScene(_)
            | Self::SetSelectedKeyframes(_)
            | Self::ToggleEditorSync
            | Self::EditorChanged
            | Self::Undo
            | Self::Redo
            | Self::ScrollToLine(_, _)
            | Self::ZoomToSelection
            | Self::ZoomToAll
            | Self::FindReplaceAll => true,
            Self::ReorderScenes(_)
            | Self::SetTransition { .. }
            | Self::SetPlayTarget { .. }
            | Self::SetSceneDuration { .. }
            | Self::DuplicateScene(_)
            | Self::DeleteScene(_)
            | Self::AddScene(_)
            | Self::CreateActor { .. }
            | Self::RenameActor { .. }
            | Self::DuplicateActor(_)
            | Self::DuplicateSelectedActors
            | Self::DeleteSelectedActors
            | Self::ReparentActor { .. }
            | Self::ExtractScene { .. }
            | Self::MoveToScene { .. }
            | Self::ToggleActorVisibility(_)
            | Self::ToggleActorLock(_)
            | Self::PropertyEdit(_)
            | Self::SetConfigProperty { .. }
            | Self::DetachCallout { .. }
            | Self::SetKeyframeEasing { .. }
            | Self::DeleteKeyframe { .. }
            | Self::MoveKeyframe { .. }
            | Self::SetKeyframeValue { .. }
            | Self::MoveKeyframes(_)
            | Self::DeleteKeyframes(_)
            | Self::ResizeAction { .. }
            | Self::PasteActors
            | Self::AlignActors(_)
            | Self::DistributeActors(_)
            | Self::GroupSelectedActors
            | Self::UngroupSelectedActors
            | Self::InsertionFromPalette => false,
        }
    }
}

impl ShellAction {
    pub(crate) fn allowed_in_workbench(&self) -> bool {
        match self {
            Self::Command(command) => command.allowed_in_workbench(),
            Self::View(super::ViewAction::OpenInsertionPalette) | Self::Drag(_) => false,
            Self::View(_) => true,
        }
    }
}
