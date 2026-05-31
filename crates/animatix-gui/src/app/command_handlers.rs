//! Standalone command handler functions extracted from `GuiShell`.
//!
//! Each command in the `Command` enum has a corresponding free function that
//! takes only the stores it needs.  This makes handlers testable without a
//! full `GuiShell` (no temp dirs, no filesystem) and keeps the dispatch logic
//! thin.
//!
//! The original `GuiShell::handle_command` is now a thin dispatcher that
//! forwards to these free functions.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::app::commands::{Command, Effect, UndoEntry};
use crate::app::components::toast::Toast;
use crate::app::document_controller::DocumentController;
use crate::app::file_tree::build_file_tree;
use crate::app::persistence::save_app_state;
use crate::app::preview::DragState;
use crate::app::stores::*;
use crate::app::utils::has_source_load_failure;
use crate::app::GuiShell;
use crate::document::{DocumentSession, timeline_keyframe_times_s};
use animatix::diagnostics::diagnostics_phase_summary;

// =========================================================================
// Helper: sync preview dimensions / duration / time from document state
// =========================================================================

/// Sync preview playback state from document metadata.
fn sync_preview_from_document(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    status: String,
    reset_time: bool,
    stop_playback: bool,
) {
    preview_store.preview.playback.duration_s = document_store.document.duration_s.max(0.1);
    preview_store.preview.dimensions = document_store.document.scene_dimensions;
    if reset_time {
        preview_store.preview.playback.current_time_s = 0.0;
        preview_store.preview.viewport.preview_zoom = 1.0;
        preview_store.preview.viewport.preview_pan = egui::Vec2::new(
            document_store.document.scene_dimensions.width as f32 / 2.0,
            document_store.document.scene_dimensions.height as f32 / 2.0,
        );
    } else {
        preview_store.preview.playback.clamp_time();
    }
    if stop_playback {
        preview_store.preview.playback.is_playing = false;
    }
    // Clear error + set status
    preview_store.preview.error = None;
    preview_store.preview.status = status;
    preview_store.preview_dirty = true;
}

fn sync_active_scene_from_time(
    document_store: &mut DocumentStore,
    preview_store: &PreviewStore,
) {
    if let Some(composition) = document_store.document.composition.as_ref() {
        let (scene, _, _) =
            composition.evaluate(preview_store.preview.playback.current_time_s);
        document_store.document.active_scene = (!scene.is_empty()).then_some(scene);
    }
}

// =========================================================================
// Helper: suggest export filename (extracted from GuiShell)
// =========================================================================

fn suggest_export_filename(
    export_store: &ExportStore,
    document_store: &DocumentStore,
) -> PathBuf {
    let ext = match export_store.export_state.format {
        crate::app::shell::export_dialog::ExportFormat::Image => "png",
        crate::app::shell::export_dialog::ExportFormat::Video => "mp4",
        crate::app::shell::export_dialog::ExportFormat::Gif => "gif",
    };
    let stem = document_store
        .document
        .file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("animatix");
    let workspace = document_store
        .document
        .file_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    workspace.join(format!("{}_export.{ext}", stem))
}

// =========================================================================
// Helper: editor-sync effects (shared by scrub, prev/next keyframe)
// =========================================================================

fn editor_sync_effects(
    document_store: &DocumentStore,
    ui_store: &UiStore,
    time_s: f64,
) -> Vec<Effect> {
    let mut effects = vec![];
    if ui_store.editor_sync_enabled {
        if let Some(line) = document_store.document.find_keyframe_line_at(time_s) {
            // Note: can't use self.document_store.editor in a free function
            // that borrows document_store immutably.  The caller (dispatch)
            // applies scrolling effects via apply_effects.
            effects.push(Effect::EditorScroll(line));
            effects.push(Effect::EditorHighlight(line));
        }
    }
    effects
}

// =========================================================================
// Handler: OpenFile
// =========================================================================

pub fn handle_open_file(
    document_store: &mut DocumentStore,
    workspace_store: &mut WorkspaceStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    path: PathBuf,
) -> Vec<Effect> {
    match DocumentSession::load(path.clone()) {
        Ok(document) => {
            let new_workspace_root =
                crate::app::file_tree::workspace_root_for(&path);
            if new_workspace_root != workspace_store.workspace_root {
                workspace_store.workspace_root = new_workspace_root;
                workspace_store.expanded_dirs =
                    std::collections::HashSet::from([workspace_store.workspace_root.clone()]);
            }
            workspace_store.file_tree = build_file_tree(
                &workspace_store.workspace_root,
                &path,
                &workspace_store.expanded_dirs,
            );
            document_store.document = document;
            document_store.invalidate_cache();
            document_store
                .editor
                .set_document(
                    &document_store.document.file_path,
                    document_store.document.source_text.clone(),
                );
            document_store.undo_stack.clear();
            document_store.redo_stack.clear();
            ui_store.interaction.drag_snapshot_taken = false;
            ui_store.interaction.inspector_input_drag_active = false;
            if let Some(ref mut reloader) = workspace_store.hot_reloader {
                if let Err(e) =
                    reloader.update_watched_file(&document_store.document.file_path)
                {
                    tracing::warn!("Failed to update watched file: {}", e);
                }
            }
            let status = if has_source_load_failure(&document_store.document.diagnostics) {
                format!(
                    "Opened {} • parse/load error • {}",
                    document_store.document.file_path.display(),
                    diagnostics_phase_summary(&document_store.document.diagnostics)
                )
            } else {
                document_store.document_status(format!(
                    "Opened {}",
                    document_store.document.file_path.display()
                ))
            };
            let error = document_store.document.last_rebuild_error.clone();
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

// =========================================================================
// Handler: ToggleExpandDir
// =========================================================================

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
        &document_store.document.file_path,
        &workspace_store.expanded_dirs,
    );
    vec![]
}

// =========================================================================
// Handler: ShowInspector
// =========================================================================

pub fn handle_show_inspector(ui_store: &mut UiStore) -> Vec<Effect> {
    let new_visible = !ui_store.view.inspector_visible;
    ui_store.view.inspector_visible = new_visible;
    ui_store.view.tree = crate::app::persistence::build_tree(new_visible);
    vec![]
}

// =========================================================================
// Handler: OpenExportDialog
// =========================================================================

pub fn handle_open_export_dialog(
    export_store: &mut ExportStore,
    document_store: &DocumentStore,
) -> Vec<Effect> {
    export_store.export_dialog_open = true;
    if export_store.export_state.output_path.is_empty() {
        let path = suggest_export_filename(export_store, document_store);
        export_store.export_state.output_path = path.to_string_lossy().to_string();
    }
    vec![]
}

// =========================================================================
// Handler: ToggleDiagnosticsPanel
// =========================================================================

pub fn handle_toggle_diagnostics_panel(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.view.diagnostics_panel_visible = !ui_store.view.diagnostics_panel_visible;
    vec![]
}

// =========================================================================
// Handler: Save
// =========================================================================

pub fn handle_save(
    document_store: &mut DocumentStore,
    _preview_store: &mut PreviewStore,
) -> Vec<Effect> {
    let text = document_store.editor.text().to_string();
    let path = document_store.document.file_path.clone();
    match std::fs::write(&path, &text) {
        Ok(()) => {
            document_store.document.source_text = text;
            document_store.document.is_dirty = false;
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

// =========================================================================
// Handler: Reload
// =========================================================================

pub fn handle_reload(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    workspace_store: &mut WorkspaceStore,
) -> Vec<Effect> {
    document_store.invalidate_cache();
    match document_store.document.reload_from_disk() {
        Ok(()) => {
            document_store
                .editor
                .set_document(
                    &document_store.document.file_path,
                    document_store.document.source_text.clone(),
                );
            let status = if has_source_load_failure(&document_store.document.diagnostics) {
                format!(
                    "Reloaded {} • parse/load error • {}",
                    document_store.document.file_path.display(),
                    diagnostics_phase_summary(&document_store.document.diagnostics)
                )
            } else {
                document_store.document_status(format!(
                    "Reloaded {}",
                    document_store.document.file_path.display()
                ))
            };
            let error = document_store.document.last_rebuild_error.clone();
            sync_preview_from_document(document_store, preview_store, status, false, false);
            preview_store.preview.error = error;
            workspace_store.file_tree = build_file_tree(
                &workspace_store.workspace_root,
                &document_store.document.file_path,
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

// =========================================================================
// Handler: Rebuild
// =========================================================================

pub fn handle_rebuild(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.invalidate_cache();
    preview_store.rebuild_in_progress = true;
    preview_store.preview.status = "Building timeline…".to_string();
    preview_store.preview_dirty = true;

    // Phase 6.5: Background rebuild is deferred because Timeline contains
    // RefCell fields and is not Send. Once those are replaced with thread-safe
    // alternatives, the actual rebuild can be spawned on a worker thread.
    match document_store.document.rebuild() {
        Ok(()) => {
            preview_store.rebuild_in_progress = false;
            let status = if document_store.document.diagnostics.is_empty() {
                format!(
                    "Built timeline • {:.2}s total duration",
                    document_store.document.duration_s.max(0.1)
                )
            } else {
                format!(
                    "Built timeline • {:.2}s total duration • {}",
                    document_store.document.duration_s.max(0.1),
                    diagnostics_phase_summary(&document_store.document.diagnostics)
                )
            };
            sync_preview_from_document(document_store, preview_store, status, false, false);
            ui_store
                .toasts
                .push(Toast::success(format!(
                    "Built timeline • {:.2}s",
                    document_store.document.duration_s.max(0.1)
                )));
            vec![]
        }
        Err(error) => {
            preview_store.rebuild_in_progress = false;
            let status = if has_source_load_failure(&document_store.document.diagnostics) {
                format!(
                    "Rebuild blocked • parse/load error • {}",
                    diagnostics_phase_summary(&document_store.document.diagnostics)
                )
            } else {
                "Rebuild blocked".to_string()
            };
            preview_store.preview.playback.duration_s =
                document_store.document.duration_s.max(0.1);
            preview_store.preview.dimensions = document_store.document.scene_dimensions;
            preview_store.preview.playback.clamp_time();
            preview_store.preview.status = status;
            preview_store.preview.error = Some(error.to_string());
            preview_store.preview_dirty = true;
            ui_store.toasts.push(Toast::error("Rebuild failed"));
            vec![]
        }
    }
}

// =========================================================================
// Handler: ScrubTo
// =========================================================================

pub fn handle_scrub_to(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    next_time: f64,
) -> Vec<Effect> {
    preview_store.preview.playback.current_time_s = next_time;
    preview_store.preview.playback.clamp_time();
    preview_store.preview.playback.is_playing = false;
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects: Vec<Effect> = vec![];
    if ui_store.editor_sync_enabled {
        if let Some(line) = document_store
            .document
            .find_keyframe_line_at(preview_store.preview.playback.current_time_s)
        {
            effects.push(Effect::EditorScroll(line));
            effects.push(Effect::EditorHighlight(line));
        }
    }
    effects
}

// =========================================================================
// Handler: TogglePlayback
// =========================================================================

pub fn handle_toggle_playback(preview_store: &mut PreviewStore) -> Vec<Effect> {
    preview_store.preview.playback.toggle_playback();
    preview_store.preview_dirty = true;
    vec![Effect::Repaint]
}

// =========================================================================
// Handler: ToggleEditorSync
// =========================================================================

pub fn handle_toggle_editor_sync(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.editor_sync_enabled = !ui_store.editor_sync_enabled;
    vec![if ui_store.editor_sync_enabled {
        Effect::Status("Editor sync ON".to_string())
    } else {
        Effect::Status("Editor sync OFF".to_string())
    }]
}

// =========================================================================
// Handler: ToggleKeyframeMode (no-op)
// =========================================================================

pub fn handle_toggle_keyframe_mode() -> Vec<Effect> {
    vec![]
}

// =========================================================================
// Handler: EditorChanged
// =========================================================================

pub fn handle_editor_changed(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    document_store
        .document
        .set_source_text(document_store.editor.text().to_string());
    preview_store.pending_rebuild_at =
        Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
    preview_store.preview.error = None;
    document_store.document.diagnostics.clear();
    vec![
        Effect::Status("Editing source • rebuild scheduled".to_string()),
        Effect::RebuildScheduled,
    ]
}

// =========================================================================
// Handler: RequestRepaint
// =========================================================================

pub fn handle_request_repaint(preview_store: &mut PreviewStore) -> Vec<Effect> {
    preview_store.preview_dirty = true;
    vec![Effect::Repaint]
}

// =========================================================================
// Handler: PrevKeyframe
// =========================================================================

pub fn handle_prev_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = timeline_keyframe_times_s(
        if document_store.document.composition.is_some() {
            None
        } else {
            document_store.document.active_timeline()
        },
        document_store.document.composition.as_ref(),
        document_store.document.active_scene.as_deref(),
    );
    preview_store.preview.playback.go_to_previous_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    let status = format!(
        "Previous keyframe • t = {:.2}s / {:.2}s",
        preview_store.preview.playback.current_time_s,
        preview_store.preview.playback.duration_s
    );
    let mut effects = vec![Effect::Status(status)];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

// =========================================================================
// Handler: NextKeyframe
// =========================================================================

pub fn handle_next_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = timeline_keyframe_times_s(
        if document_store.document.composition.is_some() {
            None
        } else {
            document_store.document.active_timeline()
        },
        document_store.document.composition.as_ref(),
        document_store.document.active_scene.as_deref(),
    );
    preview_store.preview.playback.go_to_next_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    let status = format!(
        "Next keyframe • t = {:.2}s / {:.2}s",
        preview_store.preview.playback.current_time_s,
        preview_store.preview.playback.duration_s
    );
    let mut effects = vec![Effect::Status(status)];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

// =========================================================================
// Handler: PrevScene / NextScene
// =========================================================================

pub fn handle_prev_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
) -> Vec<Effect> {
    if let Some(composition) = document_store.document.composition.as_ref() {
        let current_idx = document_store
            .document
            .active_scene
            .as_deref()
            .and_then(|name| composition.declaration_order.iter().position(|n| n == name))
            .unwrap_or(0);
        let target_idx = current_idx.saturating_sub(1);
        if let Some(target_name) = composition.declaration_order.get(target_idx) {
            document_store.document.active_scene = Some(target_name.clone());
            if let Some(start) = composition.scene_start_times.get(target_name) {
                preview_store.preview.playback.current_time_s = *start;
                preview_store.preview.playback.clamp_time();
                preview_store.preview.playback.is_playing = false;
                preview_store.preview_dirty = true;
                return vec![Effect::Status(format!(
                    "Scene {} • t = {:.2}s / {:.2}s",
                    target_name,
                    preview_store.preview.playback.current_time_s,
                    preview_store.preview.playback.duration_s
                ))];
            }
        }
    }
    vec![]
}

pub fn handle_next_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
) -> Vec<Effect> {
    if let Some(composition) = document_store.document.composition.as_ref() {
        let current_idx = document_store
            .document
            .active_scene
            .as_deref()
            .and_then(|name| composition.declaration_order.iter().position(|n| n == name))
            .unwrap_or(0);
        let target_idx =
            (current_idx + 1).min(composition.declaration_order.len().saturating_sub(1));
        if let Some(target_name) = composition.declaration_order.get(target_idx) {
            document_store.document.active_scene = Some(target_name.clone());
            if let Some(start) = composition.scene_start_times.get(target_name) {
                preview_store.preview.playback.current_time_s = *start;
                preview_store.preview.playback.clamp_time();
                preview_store.preview.playback.is_playing = false;
                preview_store.preview_dirty = true;
                return vec![Effect::Status(format!(
                    "Scene {} • t = {:.2}s / {:.2}s",
                    target_name,
                    preview_store.preview.playback.current_time_s,
                    preview_store.preview.playback.duration_s
                ))];
            }
        }
    }
    vec![]
}

// =========================================================================
// Handler: SelectScene
// =========================================================================

pub fn handle_select_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    scene: String,
) -> Vec<Effect> {
    if let Some(composition) = document_store.document.composition.as_ref() {
        if composition.scenes.contains_key(&scene) {
            document_store.document.active_scene = Some(scene.clone());
            if let Some(start) = composition.scene_start_times.get(&scene) {
                let mut target_time = *start;
                for edge in composition.edges.values() {
                    if edge.to_scene == scene {
                        target_time += edge.transition.duration_ms as f64 / 1000.0;
                        break;
                    }
                }
                preview_store.preview.playback.current_time_s = target_time;
                preview_store.preview.playback.clamp_time();
                preview_store.preview.playback.is_playing = false;
                preview_store.preview_dirty = true;
                return vec![Effect::Status(format!(
                    "Scene {} • t = {:.2}s / {:.2}s",
                    scene,
                    preview_store.preview.playback.current_time_s,
                    preview_store.preview.playback.duration_s
                ))];
            }
        }
    }
    vec![]
}

// =========================================================================
// Handler: DeleteScene
// =========================================================================

pub fn handle_delete_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    scene: String,
) -> Vec<Effect> {
    if let Some(ref mut stmts) = document_store.document.raw_statements {
        let edit = crate::source_edit::SourceEdit::DeleteScene { name: scene.clone() };
        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            document_store.document.source_text = new_source.clone();
            document_store.editor.replace_text(new_source);
            document_store.document.is_dirty = true;
            document_store.document.source_index =
                Some(animatix::source_index::SourceIndex::build(stmts));
            preview_store.pending_rebuild_at =
                Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
            if document_store.document.active_scene.as_ref() == Some(&scene) {
                document_store.document.active_scene = None;
            }
            return vec![Effect::Status(format!("Deleted scene {}", scene))];
        }
    }
    vec![]
}

// =========================================================================
// Handler: AddScene
// =========================================================================

pub fn handle_add_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let existing: std::collections::HashSet<String> =
        document_store.document.scene_names().iter().cloned().collect();
    if let Some(ref mut stmts) = document_store.document.raw_statements {
        let mut i = 1;
        let new_name = loop {
            let candidate = format!("Scene{}", i);
            if !existing.contains(&candidate) {
                break candidate;
            }
            i += 1;
        };
        let edit = crate::source_edit::SourceEdit::AddScene {
            name: new_name.clone(),
        };
        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            document_store.document.source_text = new_source.clone();
            document_store.editor.replace_text(new_source);
            document_store.document.is_dirty = true;
            document_store.document.source_index =
                Some(animatix::source_index::SourceIndex::build(stmts));
            preview_store.pending_rebuild_at =
                Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
            return vec![Effect::Status(format!("Added scene {}", new_name))];
        }
    }
    vec![]
}

// =========================================================================
// Handler: RenameScene
// =========================================================================

pub fn handle_rename_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    old_name: String,
    new_name: String,
) -> Vec<Effect> {
    if old_name != new_name && !new_name.is_empty() {
        if let Some(ref mut stmts) = document_store.document.raw_statements {
            let edit = crate::source_edit::SourceEdit::RenameScene {
                old_name,
                new_name: new_name.clone(),
            };
            if crate::source_edit::apply_edit(stmts, edit) {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                document_store.document.source_text = new_source.clone();
                document_store.editor.replace_text(new_source);
                document_store.document.is_dirty = true;
                document_store.document.source_index =
                    Some(animatix::source_index::SourceIndex::build(stmts));
                preview_store.pending_rebuild_at =
                    Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
                return vec![Effect::Status(format!("Renamed scene to {}", new_name))];
            }
        }
    }
    vec![]
}

// =========================================================================
// Handler: ReorderScenes
// =========================================================================

pub fn handle_reorder_scenes(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    new_order: Vec<String>,
) -> Vec<Effect> {
    if let Some(ref mut stmts) = document_store.document.raw_statements {
        let edit = crate::source_edit::SourceEdit::ReorderScenes {
            new_order: new_order.clone(),
        };
        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            document_store.document.source_text = new_source.clone();
            document_store.editor.replace_text(new_source);
            document_store.document.is_dirty = true;
            document_store.document.source_index =
                Some(animatix::source_index::SourceIndex::build(stmts));
            preview_store.pending_rebuild_at =
                Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
            return vec![Effect::Status("Reordered scenes".to_string())];
        }
    }
    vec![]
}

// =========================================================================
// Handler: Undo
// =========================================================================

pub fn handle_undo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if let Some(entry) = document_store.undo_stack.pop() {
        document_store.redo_stack.push(UndoEntry {
            command: entry.command,
            source_before: document_store.document.source_text.clone(),
        });
        document_store.document.source_text = entry.source_before.clone();
        document_store.editor.replace_text(entry.source_before);
        document_store.document.is_dirty = true;
        preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
        preview_store.preview.status = "Undo".to_string();
        ui_store.toasts.push(Toast::info("Undo"));
    }
    vec![]
}

// =========================================================================
// Handler: Redo
// =========================================================================

pub fn handle_redo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if let Some(entry) = document_store.redo_stack.pop() {
        document_store.undo_stack.push(UndoEntry {
            command: entry.command,
            source_before: document_store.document.source_text.clone(),
        });
        document_store.document.source_text = entry.source_before.clone();
        document_store.editor.replace_text(entry.source_before);
        document_store.document.is_dirty = true;
        preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
        preview_store.preview.status = "Redo".to_string();
        ui_store.toasts.push(Toast::info("Redo"));
    }
    vec![]
}

// =========================================================================
// Handler: ScrollToLine
// =========================================================================

pub fn handle_scroll_to_line(
    document_store: &mut DocumentStore,
    line: usize,
    column: usize,
) -> Vec<Effect> {
    document_store.editor.focus_diagnostic(line, column);
    vec![]
}

// =========================================================================
// Handler: CreateActor
// =========================================================================

pub fn handle_create_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    ty: String,
    label: String,
    position: [f32; 2],
) -> Vec<Effect> {
    // Take snapshot
    document_store.snapshot(Command::CreateActor {
        ty: ty.clone(),
        label: label.clone(),
        position,
    });

    // Use DocumentController for the actor insertion work
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_create_actor(&ty, &label, position);
    vec![]
}

// =========================================================================
// Handler: RenameActor
// =========================================================================

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
    // Check uniqueness
    if let Some(ref timeline) = document_store.document.timeline {
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

    if let Some(ref mut stmts) = document_store.document.raw_statements {
        let edit = crate::source_edit::SourceEdit::RenameActor {
            old_label: old_label_for_edit,
            new_label: new_label_for_edit,
        };
        crate::source_edit::apply_edit(stmts, edit);
        let new_source = animatix::to_source::stmts_to_source(stmts);
        document_store.document.source_text = new_source.clone();
        document_store.editor.replace_text(new_source);
        document_store.document.is_dirty = true;
        document_store.document.source_index =
            Some(animatix::source_index::SourceIndex::build(stmts));
        preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(100));
        preview_store.preview.status = format!("Renamed {} → {}", old_label, new_label);
    } else {
        preview_store.preview.status = "Rename failed — no AST available".to_string();
        return vec![];
    }

    // Update selection to the new name
    if ui_store.selection.selected_actors.contains(&old_label) {
        ui_store.selection.selected_actors.remove(&old_label);
        ui_store.selection.selected_actors.insert(new_label.clone());
    }
    preview_store.preview_dirty = true;
    vec![]
}

// =========================================================================
// DocumentController-based handlers
// =========================================================================

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

pub fn handle_set_transition(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    from_scene: String,
    transition: animatix::ast::Transition,
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

pub fn handle_set_keyframe_easing(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    property: String,
    time_s: f64,
    easing: animatix::easing::Easing,
) -> Vec<Effect> {
    document_store.snapshot(Command::SetKeyframeEasing {
        actor: actor.clone(),
        property: property.clone(),
        time_s,
        easing: easing.clone(),
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

// =========================================================================
// Handler: PropertyEdit
//
// NOTE: The GuiShell method `handle_property_edit` (in `actions/mod.rs`)
// is kept because it is also called from `runtime.rs`. The command
// dispatcher delegates to it via `self.handle_property_edit(edit)`.
// =========================================================================

// =========================================================================
// Handler: DragEnded
// =========================================================================

pub fn handle_drag_ended(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.drag_state = DragState::None;
    ui_store.interaction.drag_snapshot_taken = false;
    vec![]
}

// =========================================================================
// Handler: InspectorInputDragStarted
// =========================================================================

pub fn handle_inspector_input_drag_started(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.inspector_input_drag_active = true;
    vec![]
}

// =========================================================================
// Handler: InspectorInputDragEnded
// =========================================================================

pub fn handle_inspector_input_drag_ended(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.inspector_input_drag_active = false;
    ui_store.interaction.drag_snapshot_taken = false;
    vec![]
}

// =========================================================================
// Handler: MoveKeyframe (no-op placeholder)
// =========================================================================

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

// =========================================================================
// ── Thin dispatcher (still on GuiShell) ────────────────────────────────
// =========================================================================

impl GuiShell {
    /// Handle a single command, returning any collected side effects.
    pub(crate) fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::OpenFile(path) => handle_open_file(
                &mut self.document_store,
                &mut self.workspace_store,
                &mut self.preview_store,
                &mut self.ui_store,
                path,
            ),
            Command::ToggleExpandDir(path) => {
                handle_toggle_expand_dir(&mut self.workspace_store, &self.document_store, path)
            }
            Command::ShowInspector => handle_show_inspector(&mut self.ui_store),
            Command::OpenExportDialog => {
                handle_open_export_dialog(&mut self.export_store, &self.document_store)
            }
            Command::ToggleDiagnosticsPanel => {
                handle_toggle_diagnostics_panel(&mut self.ui_store)
            }
            Command::Save => handle_save(&mut self.document_store, &mut self.preview_store),
            Command::Reload => handle_reload(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.workspace_store,
            ),
            Command::Rebuild => {
                handle_rebuild(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::ScrubTo(next_time) => handle_scrub_to(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                next_time,
            ),
            Command::TogglePlayback => handle_toggle_playback(&mut self.preview_store),
            Command::ToggleEditorSync => handle_toggle_editor_sync(&mut self.ui_store),
            Command::ToggleKeyframeMode => handle_toggle_keyframe_mode(),
            Command::EditorChanged => handle_editor_changed(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::RequestRepaint => handle_request_repaint(&mut self.preview_store),
            Command::PrevKeyframe => handle_prev_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::NextKeyframe => handle_next_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::PrevScene => {
                handle_prev_scene(&mut self.document_store, &mut self.preview_store)
            }
            Command::NextScene => {
                handle_next_scene(&mut self.document_store, &mut self.preview_store)
            }
            Command::SelectScene(scene) => {
                handle_select_scene(&mut self.document_store, &mut self.preview_store, scene)
            }
            Command::DeleteScene(scene) => handle_delete_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                scene,
            ),
            Command::AddScene => {
                handle_add_scene(&mut self.document_store, &mut self.preview_store, &self.ui_store)
            }
            Command::RenameScene { old_name, new_name } => handle_rename_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                old_name,
                new_name,
            ),
            Command::ReorderScenes(new_order) => handle_reorder_scenes(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                new_order,
            ),
            Command::CreateActor { ty, label, position } => handle_create_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                ty,
                label,
                position,
            ),
            Command::DuplicateActor(original_label) => handle_duplicate_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                original_label,
            ),
            Command::DeleteSelectedActors => handle_delete_selected_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::PasteActors => handle_paste_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::SetTransition {
                from_scene,
                transition,
            } => handle_set_transition(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                transition,
            ),
            Command::SetPlayTarget {
                from_scene,
                target,
            } => handle_set_play_target(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                target,
            ),
            Command::RenameActor { old_label, new_label } => handle_rename_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                old_label,
                new_label,
            ),
            Command::SetKeyframeEasing {
                actor,
                property,
                time_s,
                easing,
            } => handle_set_keyframe_easing(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                time_s,
                easing,
            ),
            Command::DeleteKeyframe {
                actor,
                property,
                time_s,
            } => handle_delete_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                time_s,
            ),
            Command::MoveKeyframe {
                actor,
                property,
                old_time_s,
                new_time_s,
            } => handle_move_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                old_time_s,
                new_time_s,
            ),
            Command::InspectorInputDragStarted => {
                handle_inspector_input_drag_started(&mut self.ui_store)
            }
            Command::ReparentActor { actor, new_parent } => handle_reparent_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                new_parent,
            ),
            Command::ExtractScene {
                actor_labels,
                new_scene_name,
            } => handle_extract_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                new_scene_name,
            ),
            Command::MoveToScene {
                actor_labels,
                target_scene,
            } => handle_move_to_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                target_scene,
            ),
            Command::PropertyEdit(edit) => {
                self.handle_property_edit(edit);
                vec![]
            }
            Command::DragEnded => handle_drag_ended(&mut self.ui_store),
            Command::InspectorInputDragEnded => {
                handle_inspector_input_drag_ended(&mut self.ui_store)
            }
            Command::Undo => {
                handle_undo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::Redo => {
                handle_redo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::ScrollToLine(line, column) => handle_scroll_to_line(&mut self.document_store, line, column),
        }
    }
}

// =========================================================================
// ── Tests ─────────────────────────────────────────────────────────────
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::Effect;
    use crate::app::persistence::default_tree;
    use crate::app::PreviewPaneState;
    use crate::editor::EditorBuffer;
    use animatix::timeline::SceneDimensions;
    use std::collections::HashSet;

    // ── Test helpers (no filesystem needed) ────────────────────────────

    fn make_document_store() -> DocumentStore {
        let document =
            DocumentSession::from_error(std::path::PathBuf::from("test.amx"));
        let editor =
            EditorBuffer::new(&std::path::PathBuf::from("test.amx"), document.source_text.clone());
        DocumentStore::new(document, editor)
    }

    fn make_preview_store(duration_s: f64) -> PreviewStore {
        let preview = PreviewPaneState::new(duration_s, SceneDimensions::default());
        PreviewStore::new(preview)
    }

    fn make_ui_store() -> UiStore {
        let tree = default_tree();
        UiStore::new(tree)
    }

    fn make_workspace_store() -> WorkspaceStore {
        WorkspaceStore::new(
            std::path::PathBuf::from("/tmp/test"),
            HashSet::new(),
            vec![],
            std::path::PathBuf::from(".test_persistence.ron"),
            None,
        )
    }

    // ── TogglePlayback ───────────────────────────────────────────────

    #[test]
    fn toggle_playback_returns_repaint_effect() {
        let mut preview_store = make_preview_store(5.0);
        let effects = handle_toggle_playback(&mut preview_store);
        assert_eq!(effects.len(), 1, "expected exactly 1 effect");
        assert!(
            matches!(&effects[0], Effect::Repaint),
            "expected Repaint effect"
        );
    }

    #[test]
    fn toggle_playback_toggles_playing_flag() {
        let mut preview_store = make_preview_store(5.0);
        assert!(!preview_store.preview.playback.is_playing);
        handle_toggle_playback(&mut preview_store);
        assert!(preview_store.preview.playback.is_playing);
        handle_toggle_playback(&mut preview_store);
        assert!(!preview_store.preview.playback.is_playing);
    }

    #[test]
    fn toggle_playback_resets_time_when_at_end() {
        let mut preview_store = make_preview_store(5.0);
        preview_store.preview.playback.current_time_s =
            preview_store.preview.playback.duration_s;
        handle_toggle_playback(&mut preview_store);
        assert_eq!(preview_store.preview.playback.current_time_s, 0.0);
        assert!(preview_store.preview.playback.is_playing);
    }

    // ── ScrubTo ──────────────────────────────────────────────────────

    #[test]
    fn scrub_to_updates_current_time_and_stops_playback() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let ui_store = make_ui_store();
        let target = preview_store.preview.playback.duration_s * 0.5;
        let clamped = target.max(0.0).min(preview_store.preview.playback.duration_s.max(0.1));
        preview_store.preview.playback.is_playing = true;

        let _effects = handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, target);

        assert_eq!(preview_store.preview.playback.current_time_s, clamped);
        assert!(!preview_store.preview.playback.is_playing);
        assert!(preview_store.preview_dirty);
    }

    #[test]
    fn scrub_to_clamps_negative_time() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let ui_store = make_ui_store();
        handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, -5.0);
        assert_eq!(preview_store.preview.playback.current_time_s, 0.0);
    }

    #[test]
    fn scrub_to_clamps_overshoot_time() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let ui_store = make_ui_store();
        handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, 999.0);
        let max = preview_store.preview.playback.duration_s.max(0.1);
        assert_eq!(preview_store.preview.playback.current_time_s, max);
    }

    // ── ToggleEditorSync ─────────────────────────────────────────────

    #[test]
    fn toggle_editor_sync_turns_off_when_on() {
        let mut ui_store = make_ui_store();
        ui_store.editor_sync_enabled = true;
        let effects = handle_toggle_editor_sync(&mut ui_store);
        assert!(!ui_store.editor_sync_enabled);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg == "Editor sync OFF"),
            "expected Status('Editor sync OFF'), got {:?}",
            effects[0]
        );
    }

    #[test]
    fn toggle_editor_sync_turns_on_when_off() {
        let mut ui_store = make_ui_store();
        ui_store.editor_sync_enabled = false;
        let effects = handle_toggle_editor_sync(&mut ui_store);
        assert!(ui_store.editor_sync_enabled);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg == "Editor sync ON"),
            "expected Status('Editor sync ON'), got {:?}",
            effects[0]
        );
    }

    // ── RequestRepaint ───────────────────────────────────────────────

    #[test]
    fn request_repaint_returns_repaint_effect() {
        let mut preview_store = make_preview_store(5.0);
        let effects = handle_request_repaint(&mut preview_store);
        assert_eq!(effects.len(), 1, "expected exactly 1 effect");
        assert!(
            matches!(&effects[0], Effect::Repaint),
            "expected Repaint effect"
        );
    }

    #[test]
    fn request_repaint_sets_preview_dirty() {
        let mut preview_store = make_preview_store(5.0);
        preview_store.preview_dirty = false;
        handle_request_repaint(&mut preview_store);
        assert!(preview_store.preview_dirty);
    }

    // ── ToggleKeyframeMode ───────────────────────────────────────────

    #[test]
    fn toggle_keyframe_mode_returns_empty() {
        let effects = handle_toggle_keyframe_mode();
        assert!(effects.is_empty());
    }

    // ── ShowInspector ────────────────────────────────────────────────

    #[test]
    fn show_inspector_returns_empty() {
        let mut ui_store = make_ui_store();
        let effects = handle_show_inspector(&mut ui_store);
        assert!(effects.is_empty());
    }

    // ── ToggleDiagnosticsPanel ───────────────────────────────────────

    #[test]
    fn toggle_diagnostics_toggles_visibility_flag() {
        let mut ui_store = make_ui_store();
        ui_store.view.diagnostics_panel_visible = false;
        let effects = handle_toggle_diagnostics_panel(&mut ui_store);
        assert!(ui_store.view.diagnostics_panel_visible);
        assert!(effects.is_empty());
    }

    // ── DragEnded ────────────────────────────────────────────────────

    #[test]
    fn drag_ended_resets_drag_state() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.drag_snapshot_taken = true;

        let effects = handle_drag_ended(&mut ui_store);

        assert!(
            matches!(
                ui_store.interaction.drag_state,
                DragState::None
            ),
            "expected DragState::None after DragEnded"
        );
        assert!(!ui_store.interaction.drag_snapshot_taken);
        assert!(effects.is_empty());
    }

    // ── Undo / Redo ──────────────────────────────────────────────────

    #[test]
    fn undo_with_empty_stack_does_nothing() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let mut ui_store = make_ui_store();
        let effects = handle_undo(&mut document_store, &mut preview_store, &mut ui_store);
        assert!(effects.is_empty());
        assert!(document_store.undo_stack.is_empty());
        assert!(document_store.redo_stack.is_empty());
    }

    #[test]
    fn redo_with_empty_stack_does_nothing() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let mut ui_store = make_ui_store();
        let effects = handle_redo(&mut document_store, &mut preview_store, &mut ui_store);
        assert!(effects.is_empty());
        assert!(document_store.undo_stack.is_empty());
        assert!(document_store.redo_stack.is_empty());
    }

    // ── InspectorInputDragStarted / Ended ────────────────────────────

    #[test]
    fn inspector_input_drag_started_sets_flag() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.inspector_input_drag_active = false;
        let effects = handle_inspector_input_drag_started(&mut ui_store);
        assert!(ui_store.interaction.inspector_input_drag_active);
        assert!(effects.is_empty());
    }

    #[test]
    fn inspector_input_drag_ended_resets_flags() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.inspector_input_drag_active = true;
        ui_store.interaction.drag_snapshot_taken = true;
        let effects = handle_inspector_input_drag_ended(&mut ui_store);
        assert!(!ui_store.interaction.inspector_input_drag_active);
        assert!(!ui_store.interaction.drag_snapshot_taken);
        assert!(effects.is_empty());
    }

    // ── ToggleExpandDir ──────────────────────────────────────────────

    #[test]
    fn toggle_expand_dir_toggles_path_in_set() {
        let workspace_store = &mut make_workspace_store();
        let document_store = make_document_store();
        let path = std::path::PathBuf::from("/tmp/test/subdir");

        assert!(!workspace_store.expanded_dirs.contains(&path));
        handle_toggle_expand_dir(workspace_store, &document_store, path.clone());
        assert!(workspace_store.expanded_dirs.contains(&path));
        handle_toggle_expand_dir(workspace_store, &document_store, path.clone());
        assert!(!workspace_store.expanded_dirs.contains(&path));
    }

    // ── ScrollToLine ─────────────────────────────────────────────────

    #[test]
    fn scroll_to_line_focuses_diagnostic() {
        let mut document_store = make_document_store();
        let effects = handle_scroll_to_line(&mut document_store, 5, 0);
        assert!(effects.is_empty());
        // focus_diagnostic requires cell_index coverage; with from_error()
        // the test document has no parsed cells so pending_scroll_to_line
        // stays None.  The important thing is that the handler doesn't panic.
    }
}
