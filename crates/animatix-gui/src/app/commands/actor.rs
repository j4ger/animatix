use super::Align;

#[derive(Debug, Clone)]
pub enum ActorCommand {
    CreateActor {
        ty: String,
        label: String,
        position: [f32; 2],
        props: Vec<animatix_syntax::ast::Property>,
    },
    RenameActor {
        old_label: String,
        new_label: String,
    },
    DuplicateActor(String),
    DuplicateSelectedActors,
    DeleteSelectedActors,
    ReparentActor {
        actor: String,
        new_parent: Option<String>,
    },
    ExtractScene {
        actor_labels: Vec<String>,
        new_scene_name: String,
    },
    MoveToScene {
        actor_labels: Vec<String>,
        target_scene: String,
    },
    ToggleActorVisibility(String),
    ToggleActorLock(String),
    PasteActors,
    AlignActors(Align),
    DistributeActors(super::Axis),
    GroupSelectedActors,
    UngroupSelectedActors,
}

impl From<ActorCommand> for super::Command {
    fn from(c: ActorCommand) -> Self {
        match c {
            ActorCommand::CreateActor { ty, label, position, props } => {
                super::Command::CreateActor { ty, label, position, props }
            }
            ActorCommand::RenameActor { old_label, new_label } => {
                super::Command::RenameActor { old_label, new_label }
            }
            ActorCommand::DuplicateActor(v) => super::Command::DuplicateActor(v),
            ActorCommand::DuplicateSelectedActors => super::Command::DuplicateSelectedActors,
            ActorCommand::DeleteSelectedActors => super::Command::DeleteSelectedActors,
            ActorCommand::ReparentActor { actor, new_parent } => {
                super::Command::ReparentActor { actor, new_parent }
            }
            ActorCommand::ExtractScene { actor_labels, new_scene_name } => {
                super::Command::ExtractScene { actor_labels, new_scene_name }
            }
            ActorCommand::MoveToScene { actor_labels, target_scene } => {
                super::Command::MoveToScene { actor_labels, target_scene }
            }
            ActorCommand::ToggleActorVisibility(v) => super::Command::ToggleActorVisibility(v),
            ActorCommand::ToggleActorLock(v) => super::Command::ToggleActorLock(v),
            ActorCommand::PasteActors => super::Command::PasteActors,
            ActorCommand::AlignActors(v) => super::Command::AlignActors(v),
            ActorCommand::DistributeActors(v) => super::Command::DistributeActors(v),
            ActorCommand::GroupSelectedActors => super::Command::GroupSelectedActors,
            ActorCommand::UngroupSelectedActors => super::Command::UngroupSelectedActors,
        }
    }
}
