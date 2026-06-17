use crate::app::commands::{Effect, UndoLabel};
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_set_transition(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    from_scene: String,
    transition: animatix_syntax::ast::Transition,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::SetTransition {
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
    document_store.snapshot(UndoLabel::SetPlayTarget {
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

pub fn handle_set_scene_duration(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: String,
    duration_s: Option<f64>,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::SetSceneDuration {
        scene: scene.clone(),
        duration_s,
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_scene_duration(&scene, duration_s);
    vec![]
}
