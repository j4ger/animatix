use super::begin_snapshot;
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
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        Command::SetTransition {
            from_scene: from_scene.clone(),
            transition: transition.clone(),
        },
    );
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
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        Command::SetPlayTarget {
            from_scene: from_scene.clone(),
            target: target.clone(),
        },
    );
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
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        Command::SetSceneDuration {
            scene: scene.clone(),
            duration_s,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_scene_duration(&scene, duration_s);
    vec![]
}

/// Change a document-level config property (for example `colorscheme`).
pub fn handle_set_config_property(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    key: String,
    value: animatix_syntax::ast::Expr,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        Command::SetConfigProperty {
            key: key.clone(),
            value: value.clone(),
        },
    );
    let Some(ref mut stmts) = document_store.source.document.raw_statements else {
        document_store.abort_snapshot();
        return vec![];
    };
    let edit = crate::source_edit::SourceEdit::SetConfigProperty {
        key: key.clone(),
        value,
    };
    if crate::source_edit::apply_edit(stmts, edit).is_err() {
        document_store.abort_snapshot();
        preview_store.preview.status = format!("Failed to set config '{}'", key);
        return vec![];
    }
    if let Some(stmts) = document_store.source.document.raw_statements.take() {
        crate::app::document_controller::commit_ast(
            document_store,
            preview_store,
            ui_store,
            stmts,
            Vec::new(),
        );
    }
    preview_store.preview.status = format!("Set config '{}'", key);
    vec![]
}
