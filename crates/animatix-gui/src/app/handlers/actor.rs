use crate::app::commands::{Command, Effect};
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_create_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    ty: String,
    label: String,
    position: [f32; 2],
) -> Vec<Effect> {
    document_store.snapshot(Command::CreateActor {
        ty: ty.clone(),
        label: label.clone(),
        position,
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_create_actor(&ty, &label, position);
    vec![]
}

pub fn handle_rename_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    old_label: String,
    new_label: String,
) -> Vec<Effect> {
    if old_label == new_label {
        return vec![];
    }
    if new_label.is_empty() {
        preview_store.preview.status = "Rename failed — label cannot be empty".to_string();
        return vec![];
    }
    if let Some(ref timeline) = document_store.source.document.timeline {
        if timeline.has_actor(&new_label) {
            preview_store.preview.status =
                format!("Rename failed — '{}' already exists", new_label);
            return vec![];
        }
    }

    document_store.snapshot(Command::RenameActor {
        old_label: old_label.clone(),
        new_label: new_label.clone(),
    });

    let old_label_for_edit = old_label.clone();
    let new_label_for_edit = new_label.clone();

    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let edit = crate::source_edit::SourceEdit::RenameActor {
            old_label: old_label_for_edit,
            new_label: new_label_for_edit,
        };
        if crate::source_edit::apply_edit(stmts, edit).is_err() {
            preview_store.preview.status =
                format!("Rename failed — could not rename '{}' to '{}'", old_label, new_label);
            return vec![];
        }
        let new_source = animatix_syntax::to_source::stmts_to_source(stmts);
        document_store.source.document.source_text = new_source.clone();
        document_store.source.editor.replace_text(new_source);
        document_store.source.document.is_dirty = true;
        document_store.source.document.source_index =
            Some(animatix_syntax::source_index::SourceIndex::build(stmts));
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        preview_store.preview.status = format!("Renamed {} → {}", old_label, new_label);
    } else {
        preview_store.preview.status = "Rename failed — no AST available".to_string();
        return vec![];
    }

    if ui_store.selection.selected_actors.contains(&old_label) {
        ui_store.selection.selected_actors.remove(&old_label);
        ui_store.selection.selected_actors.insert(new_label.clone());
    }
    preview_store.preview_dirty = true;
    vec![]
}

pub fn handle_duplicate_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    original_label: String,
) -> Vec<Effect> {
    document_store.snapshot(Command::DuplicateActor(original_label.clone()));
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_duplicate_actor(&original_label);
    vec![]
}

pub fn handle_delete_selected_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.snapshot(Command::DeleteSelectedActors);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_delete_selected_actors();
    vec![]
}

pub fn handle_paste_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.snapshot(Command::PasteActors);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.paste_actors();
    vec![]
}

pub fn handle_reparent_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    new_parent: Option<String>,
) -> Vec<Effect> {
    document_store.snapshot(Command::ReparentActor {
        actor: actor.clone(),
        new_parent: new_parent.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_reparent_actor(&actor, new_parent);
    vec![]
}

pub fn handle_extract_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor_labels: Vec<String>,
    new_scene_name: String,
) -> Vec<Effect> {
    document_store.snapshot(Command::ExtractScene {
        actor_labels: actor_labels.clone(),
        new_scene_name: new_scene_name.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_extract_scene(actor_labels, new_scene_name);
    vec![]
}

pub fn handle_move_to_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor_labels: Vec<String>,
    target_scene: String,
) -> Vec<Effect> {
    document_store.snapshot(Command::MoveToScene {
        actor_labels: actor_labels.clone(),
        target_scene: target_scene.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_move_to_scene(actor_labels, target_scene);
    vec![]
}

pub fn handle_toggle_actor_visibility(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut UiStore,
    actor: String,
) -> Vec<Effect> {
    if let Some(ref mut timeline) = document_store.source.document.timeline {
        if let Some(track) = timeline.get_track_mut(&actor) {
            track.visible = !track.visible;
            preview_store.preview_dirty = true;
            let status = if track.visible {
                format!("{actor} visible")
            } else {
                format!("{actor} hidden")
            };
            preview_store.preview.status = status;
        }
    }
    vec![]
}

pub fn handle_toggle_actor_lock(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut UiStore,
    actor: String,
) -> Vec<Effect> {
    if let Some(ref mut timeline) = document_store.source.document.timeline {
        if let Some(track) = timeline.get_track_mut(&actor) {
            track.locked = !track.locked;
            preview_store.preview_dirty = true;
            let status = if track.locked {
                format!("{actor} locked")
            } else {
                format!("{actor} unlocked")
            };
            preview_store.preview.status = status;
        }
    }
    vec![]
}
