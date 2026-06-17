use animatix_syntax::ast::Transition;

#[derive(Debug, Clone)]
pub enum SceneCommand {
    SelectScene(String),
    ReorderScenes(Vec<String>),
    SetTransition {
        from_scene: String,
        transition: Transition,
    },
    SetPlayTarget {
        from_scene: String,
        target: Option<String>,
    },
    SetSceneDuration {
        scene: String,
        duration_s: Option<f64>,
    },
    DuplicateScene(String),
    DeleteScene(String),
}

impl From<SceneCommand> for super::Command {
    fn from(c: SceneCommand) -> Self {
        match c {
            SceneCommand::SelectScene(v) => super::Command::SelectScene(v),
            SceneCommand::ReorderScenes(v) => super::Command::ReorderScenes(v),
            SceneCommand::SetTransition { from_scene, transition } => {
                super::Command::SetTransition { from_scene, transition }
            }
            SceneCommand::SetPlayTarget { from_scene, target } => {
                super::Command::SetPlayTarget { from_scene, target }
            }
            SceneCommand::SetSceneDuration { scene, duration_s } => {
                super::Command::SetSceneDuration { scene, duration_s }
            }
            SceneCommand::DuplicateScene(v) => super::Command::DuplicateScene(v),
            SceneCommand::DeleteScene(v) => super::Command::DeleteScene(v),
        }
    }
}
