#[derive(Debug, Clone)]
pub enum PlaybackCommand {
    TogglePlayback,
    ScrubTo(f64),
    PrevKeyframe,
    NextKeyframe,
    FrameStepForward,
    FrameStepBackward,
    ToggleEditorSync,
    EditorChanged,
}

impl From<PlaybackCommand> for super::Command {
    fn from(c: PlaybackCommand) -> Self {
        match c {
            PlaybackCommand::TogglePlayback => super::Command::TogglePlayback,
            PlaybackCommand::ScrubTo(v) => super::Command::ScrubTo(v),
            PlaybackCommand::PrevKeyframe => super::Command::PrevKeyframe,
            PlaybackCommand::NextKeyframe => super::Command::NextKeyframe,
            PlaybackCommand::FrameStepForward => super::Command::FrameStepForward,
            PlaybackCommand::FrameStepBackward => super::Command::FrameStepBackward,
            PlaybackCommand::ToggleEditorSync => super::Command::ToggleEditorSync,
            PlaybackCommand::EditorChanged => super::Command::EditorChanged,
        }
    }
}
