use crate::app::commands::{Command, Effect};
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_set_keyframe_easing(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    property: String,
    time_s: f64,
    easing: animatix_syntax::easing::Easing,
) -> Vec<Effect> {
    document_store.snapshot(Command::SetKeyframeEasing {
        actor: actor.clone(),
        property: property.clone(),
        time_s,
        easing,
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_keyframe_easing(&actor, &property, time_s, easing);
    vec![]
}

pub fn handle_delete_keyframe(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    property: String,
    time_s: f64,
) -> Vec<Effect> {
    document_store.snapshot(Command::DeleteKeyframe {
        actor: actor.clone(),
        property: property.clone(),
        time_s,
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_delete_keyframe(&actor, &property, time_s);
    vec![]
}

pub fn handle_move_keyframe(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    property: String,
    old_time_s: f64,
    new_time_s: f64,
) -> Vec<Effect> {
    document_store.snapshot(Command::MoveKeyframe {
        actor: actor.clone(),
        property: property.clone(),
        old_time_s,
        new_time_s,
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_move_keyframe(&actor, &property, old_time_s, new_time_s);
    vec![]
}
