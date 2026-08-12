#[derive(Debug, Clone)]
pub enum KeyframeCommand {
    SetKeyframeEasing {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    DeleteKeyframe {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
    },
    #[allow(dead_code)] // Constructed via Command directly, not through KeyframeCommand
    MoveKeyframe {
        scene: Option<String>,
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
    #[allow(dead_code)] // Constructed via Command directly, not through KeyframeCommand
    ResizeAction {
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    },
}

impl From<KeyframeCommand> for super::Command {
    fn from(c: KeyframeCommand) -> Self {
        match c {
            KeyframeCommand::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            } => super::Command::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            },
            KeyframeCommand::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            } => super::Command::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            },
            KeyframeCommand::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            } => super::Command::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            },
            KeyframeCommand::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            } => super::Command::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            },
        }
    }
}
