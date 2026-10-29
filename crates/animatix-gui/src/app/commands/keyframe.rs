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
        }
    }
}
