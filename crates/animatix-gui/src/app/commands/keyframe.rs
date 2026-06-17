#[derive(Debug, Clone)]
pub enum KeyframeCommand {
    SetKeyframeEasing {
        actor: String,
        property: String,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    DeleteKeyframe {
        actor: String,
        property: String,
        time_s: f64,
    },
    MoveKeyframe {
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
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
            KeyframeCommand::SetKeyframeEasing { actor, property, time_s, easing } => {
                super::Command::SetKeyframeEasing { actor, property, time_s, easing }
            }
            KeyframeCommand::DeleteKeyframe { actor, property, time_s } => {
                super::Command::DeleteKeyframe { actor, property, time_s }
            }
            KeyframeCommand::MoveKeyframe { actor, property, old_time_s, new_time_s } => {
                super::Command::MoveKeyframe { actor, property, old_time_s, new_time_s }
            }
            KeyframeCommand::ResizeAction { verb, targets, old_start_s, new_start_s, new_duration_s } => {
                super::Command::ResizeAction { verb, targets, old_start_s, new_start_s, new_duration_s }
            }
        }
    }
}
