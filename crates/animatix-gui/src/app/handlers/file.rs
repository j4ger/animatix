use std::path::PathBuf;

use crate::app::commands::Effect;
use crate::app::components::toast::Toast;
use crate::app::document::version::SourceEpoch;
use crate::app::file_tree::build_file_tree;
use crate::app::persistence::save_app_state;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore, WorkspaceStore};
use crate::app::utils::has_source_load_failure;
use crate::document::DocumentSession;
use animatix_syntax::diagnostics::diagnostics_phase_summary;

/// Sync preview playback state from document metadata.
pub(crate) fn sync_preview_from_document(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    status: String,
    reset_time: bool,
    stop_playback: bool,
) {
    preview_store.preview.playback.duration_s = document_store.source.document.duration_s.max(0.1);
    preview_store.preview.dimensions = document_store.source.document.scene_dimensions;
    if reset_time {
        preview_store.preview.playback.scrub_to(0.0);
        preview_store.preview.viewport.preview_zoom = 1.0;
        preview_store.preview.viewport.preview_pan = egui::Vec2::new(
            document_store.source.document.scene_dimensions.width as f32 / 2.0,
            document_store.source.document.scene_dimensions.height as f32 / 2.0,
        );
    } else {
        preview_store.preview.playback.clamp_time();
    }
    if stop_playback {
        preview_store.preview.playback.is_playing = false;
    }
    preview_store.preview.error = None;
    preview_store.preview.status = status;
    preview_store.preview_dirty = true;
}

pub fn handle_open_file(
    document_store: &mut DocumentStore,
    workspace_store: &mut WorkspaceStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    path: PathBuf,
) -> Vec<Effect> {
    // P0.3: Refuse to open if there are unsaved changes.
    if document_store.source.is_dirty() {
        preview_store.preview.status =
            "Save changes before opening another file".to_string();
        return vec![Effect::Toast(Toast::warning(
            "Save changes before opening another file"
        ))];
    }
    match DocumentSession::load(path.clone()) {
        Ok(document) => {
            // Only recompute workspace root if the file is outside the current workspace
            if !path.starts_with(&workspace_store.workspace_root) {
                let new_workspace_root =
                    crate::app::file_tree::workspace_root_for(&path);
                if new_workspace_root != workspace_store.workspace_root {
                    workspace_store.workspace_root = new_workspace_root;
                    workspace_store.expanded_dirs =
                        std::collections::HashSet::from([workspace_store.workspace_root.clone()]);
                }
            }
            workspace_store.file_tree = build_file_tree(
                &workspace_store.workspace_root,
                &path,
                &workspace_store.expanded_dirs,
            );
            document_store.source.source_epoch = SourceEpoch::initial();
            document_store.source.document = document;
            document_store.source.invalidate_cache();
            document_store
                .source
                .editor
                .set_document(
                    &document_store.source.document.file_path,
                    document_store.source.document.source_text.clone(),
                );
            document_store.history.undo_stack.clear();
            document_store.history.redo_stack.clear();
            ui_store.interaction.reset_drag_state();
            if let Some(ref mut reloader) = workspace_store.hot_reloader {
                if let Err(e) =
                    reloader.update_watched_file(&document_store.source.document.file_path)
                {
                    tracing::warn!("Failed to update watched file: {}", e);
                }
            }
            let status = if has_source_load_failure(&document_store.source.document.diagnostics) {
                format!(
                    "Opened {} • parse/load error • {}",
                    document_store.source.document.file_path.display(),
                    diagnostics_phase_summary(&document_store.source.document.diagnostics)
                )
            } else {
                document_store.document_status(format!(
                    "Opened {}",
                    document_store.source.document.file_path.display()
                ))
            };
            let error = document_store.source.document.last_rebuild_error.clone();
            sync_preview_from_document(document_store, preview_store, status, true, true);
            preview_store.preview.error = error;
            ui_store
                .toasts
                .push(Toast::info(format!("Opened {}", path.display())));
            ui_store.view.welcome_open = false;
            save_app_state(&path);
            vec![]
        }
        Err(error) => {
            preview_store.preview.error = Some(error.to_string());
            preview_store.preview.status = format!("Open failed • {}", path.display());
            vec![]
        }
    }
}

pub fn handle_switch_workspace(
    workspace_store: &mut WorkspaceStore,
    document_store: &DocumentStore,
    path: PathBuf,
) -> Vec<Effect> {
    if path.exists() && path.is_dir() {
        workspace_store.workspace_root = path.clone();
        workspace_store.expanded_dirs = std::collections::HashSet::from([path.clone()]);
        workspace_store.file_tree = build_file_tree(
            &workspace_store.workspace_root,
            &document_store.source.document.file_path,
            &workspace_store.expanded_dirs,
        );
        vec![Effect::Status(format!("Switched workspace to {}", path.display()))]
    } else {
        vec![Effect::Toast(Toast::error(format!(
            "Not a valid directory: {}",
            path.display()
        )))]
    }
}

pub fn handle_toggle_expand_dir(
    workspace_store: &mut WorkspaceStore,
    document_store: &DocumentStore,
    path: PathBuf,
) -> Vec<Effect> {
    if workspace_store.expanded_dirs.contains(&path) {
        workspace_store.expanded_dirs.remove(&path);
    } else {
        workspace_store.expanded_dirs.insert(path.clone());
    }
    workspace_store.file_tree = build_file_tree(
        &workspace_store.workspace_root,
        &document_store.source.document.file_path,
        &workspace_store.expanded_dirs,
    );
    vec![]
}

pub fn handle_save(
    document_store: &mut DocumentStore,
    _preview_store: &mut PreviewStore,
) -> Vec<Effect> {
    let path = document_store.source.file_path().to_path_buf();
    let text = document_store.source.text().to_string();
    match std::fs::write(&path, &text) {
        Ok(()) => {
            document_store.source.mark_saved();
            vec![
                Effect::Status(format!("Saved {}", path.display())),
                Effect::Toast(Toast::success(format!("Saved {}", path.display()))),
            ]
        }
        Err(err) => {
            tracing::warn!("Save failed: {}", err);
            vec![Effect::Toast(Toast::error(format!("Save failed: {}", err)))]
        }
    }
}

pub fn handle_reload(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    workspace_store: &mut WorkspaceStore,
) -> Vec<Effect> {
    // P0.3: Refuse to reload if there are unsaved changes.
    if document_store.source.is_dirty() {
        preview_store.preview.status =
            "Save changes before reloading".to_string();
        return vec![Effect::Toast(Toast::warning(
            "Save changes before reloading"
        ))];
    }
    let path = document_store.source.file_path().to_path_buf();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            document_store.source.replace_text(text);
            document_store.source.document.is_dirty = false;
            if let Err(e) = document_store.source.document.rebuild() {
                tracing::warn!("Document reload rebuild failed: {}", e);
            }
            let status = if has_source_load_failure(&document_store.source.document.diagnostics) {
                format!(
                    "Reloaded {} • parse/load error • {}",
                    document_store.source.document.file_path.display(),
                    diagnostics_phase_summary(&document_store.source.document.diagnostics)
                )
            } else {
                document_store.document_status(format!(
                    "Reloaded {}",
                    document_store.source.document.file_path.display()
                ))
            };
            let error = document_store.source.document.last_rebuild_error.clone();
            sync_preview_from_document(document_store, preview_store, status, false, false);
            preview_store.preview.error = error;
            workspace_store.file_tree = build_file_tree(
                &workspace_store.workspace_root,
                &document_store.source.document.file_path,
                &workspace_store.expanded_dirs,
            );
            vec![]
        }
        Err(e) => {
            tracing::warn!("Reload failed: {}", e);
            vec![Effect::Toast(Toast::error(format!("Reload failed: {}", e)))]
        }
    }
}

pub fn handle_rebuild(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.source.invalidate_cache();
    preview_store.rebuild_in_progress = true;
    preview_store.preview.status = "Building timeline…".to_string();
    preview_store.preview_dirty = true;

    match document_store.source.document.rebuild() {
        Ok(()) => {
            preview_store.rebuild_in_progress = false;
            let status = if document_store.source.document.diagnostics.is_empty() {
                format!(
                    "Built timeline • {:.2}s total duration",
                    document_store.source.document.duration_s.max(0.1)
                )
            } else {
                format!(
                    "Built timeline • {:.2}s total duration • {}",
                    document_store.source.document.duration_s.max(0.1),
                    diagnostics_phase_summary(&document_store.source.document.diagnostics)
                )
            };
            sync_preview_from_document(document_store, preview_store, status, false, false);
            ui_store
                .toasts
                .push(Toast::success(format!(
                    "Built timeline • {:.2}s",
                    document_store.source.document.duration_s.max(0.1)
                )));
            vec![]
        }
        Err(error) => {
            preview_store.rebuild_in_progress = false;
            let status = if has_source_load_failure(&document_store.source.document.diagnostics) {
                format!(
                    "Rebuild blocked • parse/load error • {}",
                    diagnostics_phase_summary(&document_store.source.document.diagnostics)
                )
            } else {
                "Rebuild blocked".to_string()
            };
            preview_store.preview.playback.duration_s =
                document_store.source.document.duration_s.max(0.1);
            preview_store.preview.dimensions = document_store.source.document.scene_dimensions;
            preview_store.preview.playback.clamp_time();
            preview_store.preview.status = status;
            preview_store.preview.error = Some(error.to_string());
            preview_store.preview_dirty = true;
            ui_store.toasts.push(Toast::error("Rebuild failed"));
            vec![]
        }
    }
}
