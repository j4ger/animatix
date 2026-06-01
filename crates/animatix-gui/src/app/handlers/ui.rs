use std::path::PathBuf;

use crate::app::commands::{Effect, UndoEntry};
use crate::app::components::toast::Toast;
use crate::app::preview::DragState;
use crate::app::stores::{DocumentStore, ExportStore, PreviewStore, UiStore};

pub fn handle_show_inspector(ui_store: &mut UiStore) -> Vec<Effect> {
    let new_visible = !ui_store.view.inspector_visible;
    ui_store.view.inspector_visible = new_visible;
    ui_store.view.tree = crate::app::persistence::build_tree(new_visible);
    vec![]
}

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

pub fn handle_toggle_diagnostics_panel(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.view.diagnostics_panel_visible = !ui_store.view.diagnostics_panel_visible;
    vec![]
}

pub fn handle_drag_ended(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.reset_drag_state();
    vec![]
}

pub fn handle_inspector_input_drag_started(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.inspector_input_drag_active = true;
    vec![]
}

pub fn handle_inspector_input_drag_ended(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.interaction.reset_drag_state();
    vec![]
}

pub fn handle_undo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if let Some(entry) = document_store.history.undo_stack.pop() {
        document_store.history.redo_stack.push(UndoEntry {
            command: entry.command,
            source_before: document_store.source.document.source_text.clone(),
        });
        document_store.source.document.source_text = entry.source_before.clone();
        document_store.source.editor.replace_text(entry.source_before);
        document_store.source.document.is_dirty = true;
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(ui_store.rebuild_debounce_ms));
        preview_store.preview.status = "Undo".to_string();
        ui_store.toasts.push(Toast::info("Undo"));
    }
    vec![]
}

pub fn handle_redo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if let Some(entry) = document_store.history.redo_stack.pop() {
        document_store.history.undo_stack.push(UndoEntry {
            command: entry.command,
            source_before: document_store.source.document.source_text.clone(),
        });
        document_store.source.document.source_text = entry.source_before.clone();
        document_store.source.editor.replace_text(entry.source_before);
        document_store.source.document.is_dirty = true;
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(ui_store.rebuild_debounce_ms));
        preview_store.preview.status = "Redo".to_string();
        ui_store.toasts.push(Toast::info("Redo"));
    }
    vec![]
}

pub fn handle_scroll_to_line(
    document_store: &mut DocumentStore,
    line: usize,
    column: usize,
) -> Vec<Effect> {
    document_store.source.editor.focus_diagnostic(line, column);
    vec![]
}

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
        .source
        .document
        .file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("animatix");
    let workspace = document_store
        .source
        .document
        .file_path
        .parent()
        .unwrap_or(std::path::Path::new("."));
    workspace.join(format!("{}_export.{ext}", stem))
}
