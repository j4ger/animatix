use std::path::PathBuf;

use animatix_syntax::diagnostics::diagnostics_phase_summary;

use crate::app::commands::Effect;
use crate::app::components::toast::Toast;
use crate::app::document::rebuild::{RebuildResponse, RebuildToken, RebuildWorker};
use crate::app::document::version::SourceEpoch;
use crate::app::file_tree::build_file_tree;
use crate::app::persistence::save_app_state;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore, WorkspaceStore};
use crate::app::utils::has_source_load_failure;
use crate::document::DocumentSession;

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
        preview_store.preview.status = "Save changes before opening another file".to_string();
        return vec![Effect::Toast(Toast::warning(
            "Save changes before opening another file",
        ))];
    }
    match DocumentSession::load(path.clone()) {
        Ok(document) => {
            // Only recompute workspace root if the file is outside the current workspace
            if !path.starts_with(&workspace_store.workspace_root) {
                let new_workspace_root = crate::app::file_tree::workspace_root_for(&path);
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
            document_store.source.editor.set_document(
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
            // Publish initial snapshot (load already ran rebuild)
            document_store.clear_snapshots();
            document_store.publish_rebuild_result(
                document_store.source.document.last_rebuild_error.is_none(),
            );
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
            ui_store.toasts.push(Toast::info(format!("Opened {}", path.display())));
            ui_store.view.welcome_open = false;
            save_app_state(&path);
            vec![]
        },
        Err(error) => {
            preview_store.preview.error = Some(error.to_string());
            preview_store
                .preview
                .set_status_error(format!("Open failed • {}", path.display()));
            vec![]
        },
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
        vec![Effect::Status(format!(
            "Switched workspace to {}",
            path.display()
        ))]
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

/// Persist the current source to disk atomically and mark the document saved.
pub(crate) fn save_document(document_store: &mut DocumentStore) -> Result<(), String> {
    document_store.source.document.save_to_disk().map_err(|err| err.to_string())?;
    document_store.source.mark_saved();
    Ok(())
}

pub fn handle_save(
    document_store: &mut DocumentStore,
    _preview_store: &mut PreviewStore,
) -> Vec<Effect> {
    let path = document_store.source.file_path().to_path_buf();
    match save_document(document_store) {
        Ok(()) => vec![
            Effect::Status(format!("Saved {}", path.display())),
            Effect::Toast(Toast::success(format!("Saved {}", path.display()))),
        ],
        Err(err) => {
            tracing::warn!("Save failed: {}", err);
            vec![Effect::Toast(Toast::error(format!("Save failed: {}", err)))]
        },
    }
}

pub fn handle_reload(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    workspace_store: &mut WorkspaceStore,
) -> Vec<Effect> {
    let path = document_store.source.file_path().to_path_buf();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            document_store.replace_text(text);
            document_store.source.document.is_dirty = false;
            preview_store.pending_rebuild_at = Some(
                std::time::Instant::now()
                    + std::time::Duration::from_millis(ui_store.rebuild_debounce_ms),
            );
            workspace_store.file_tree = build_file_tree(
                &workspace_store.workspace_root,
                &document_store.source.document.file_path,
                &workspace_store.expanded_dirs,
            );
            vec![Effect::Status(format!("Reloaded {}", path.display()))]
        },
        Err(e) => {
            tracing::warn!("Reload failed: {}", e);
            vec![Effect::Toast(Toast::error(format!("Reload failed: {}", e)))]
        },
    }
}

/// Shared Ok body for rebuild: publish snapshot, compute status, sync preview, toast.
pub(crate) fn rebuild_succeeded(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.publish_rebuild_result(true);
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
    ui_store.toasts.push(Toast::success(format!(
        "Built timeline • {:.2}s",
        document_store.source.document.duration_s.max(0.1)
    )));
    vec![]
}

/// Shared Err body for rebuild: last-good fallback, publish failed snapshot, status, error.
pub(crate) fn rebuild_failed(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    error: &str,
) -> Vec<Effect> {
    document_store.publish_rebuild_result(false);

    preview_store.preview.playback.duration_s = document_store.source.document.duration_s.max(0.1);
    preview_store.preview.dimensions = document_store.source.document.scene_dimensions;

    // Last-good fallback for duration/dimensions
    if let Some(last_good) = document_store.last_good_snapshot() {
        preview_store.preview.playback.duration_s = last_good.duration_s.max(0.1);
        preview_store.preview.dimensions = last_good.scene_dimensions;
    }

    let mut status = if has_source_load_failure(&document_store.source.document.diagnostics) {
        format!(
            "Rebuild blocked • parse/load error • {}",
            diagnostics_phase_summary(&document_store.source.document.diagnostics)
        )
    } else {
        "Rebuild blocked".to_string()
    };

    if document_store.showing_last_good() {
        status.push_str(" \u{2022} showing last good build");
    }

    preview_store.preview.playback.clamp_time();
    preview_store.preview.set_status_error(status);
    preview_store.preview.error = Some(error.to_string());
    preview_store.preview_dirty = true;
    ui_store.toasts.push(Toast::error("Rebuild failed"));
    vec![]
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
            rebuild_succeeded(document_store, preview_store, ui_store)
        },
        Err(error) => {
            preview_store.rebuild_in_progress = false;
            rebuild_failed(document_store, preview_store, ui_store, &error.to_string())
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::editor::EditorBuffer;

    #[test]
    fn save_document_reports_error_without_clearing_dirty() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_save_failure_{}_{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("scene.amx");
        std::fs::create_dir_all(&target).unwrap();

        let document = DocumentSession::from_source(target.clone(), "#0s\n".to_string()).unwrap();
        let editor = EditorBuffer::new(&target, document.source_text.clone());
        let mut store = DocumentStore::new(document, editor);
        store.source.document.is_dirty = true;

        assert!(save_document(&mut store).is_err());
        assert!(store.source.document.is_dirty);
    }
}

/// Submit a rebuild request to the background worker.
pub fn handle_rebuild_submit(
    worker: &mut RebuildWorker,
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
) -> Option<RebuildToken> {
    document_store.source.invalidate_cache();
    preview_store.rebuild_in_progress = true;
    preview_store.preview.status = "Building timeline…".to_string();
    preview_store.preview_dirty = true;
    match worker.submit(&document_store.source) {
        Ok(token) => Some(token),
        Err(err) => {
            preview_store.rebuild_in_progress = false;
            preview_store
                .preview
                .set_status_error(format!("Rebuild failed to start: {err}"));
            None
        },
    }
}

/// Handle a completed background rebuild response.
pub fn handle_rebuild_response(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    response: RebuildResponse,
) -> Vec<Effect> {
    // Discard stale responses (a newer rebuild was started)
    if response.source_epoch != document_store.source.epoch() {
        return vec![];
    }

    preview_store.rebuild_in_progress = false;

    match response.result {
        Ok(output) => {
            document_store.source.invalidate_cache();
            document_store
                .source
                .document
                .apply_rebuild_output(output, response.source_hash.0);
            rebuild_succeeded(document_store, preview_store, ui_store)
        },
        Err(failure) => {
            document_store.source.invalidate_cache();
            document_store.source.document.apply_rebuild_failure(&failure);
            rebuild_failed(document_store, preview_store, ui_store, &failure.error)
        },
    }
}
