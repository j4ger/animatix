use crate::app::commands::{Command, Effect};
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_set_transition(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    from_scene: String,
    transition: animatix_syntax::ast::Transition,
) -> Vec<Effect> {
    document_store.snapshot(Command::SetTransition {
        from_scene: from_scene.clone(),
        transition: transition.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_transition(&from_scene, transition);
    vec![]
}

pub fn handle_set_play_target(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    from_scene: String,
    target: Option<String>,
) -> Vec<Effect> {
    document_store.snapshot(Command::SetPlayTarget {
        from_scene: from_scene.clone(),
        target: target.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_play_target(&from_scene, target);
    vec![]
}
