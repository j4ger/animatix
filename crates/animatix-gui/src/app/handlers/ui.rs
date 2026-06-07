use std::path::PathBuf;

use crate::app::commands::{Effect, UndoEntry};
use crate::app::components::toast::Toast;
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
    if document_store.history.undo_stack.is_empty() {
        ui_store.toasts.push(Toast::info("Nothing to undo"));
        return vec![];
    }
    if let Some(entry) = document_store.history.undo_stack.pop_back() {
        document_store.history.redo_stack.push_back(UndoEntry {
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
    if document_store.history.redo_stack.is_empty() {
        ui_store.toasts.push(Toast::info("Nothing to redo"));
        return vec![];
    }
    if let Some(entry) = document_store.history.redo_stack.pop_back() {
        document_store.history.undo_stack.push_back(UndoEntry {
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

pub fn handle_zoom_to_selection(
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    document_store: &DocumentStore,
) -> Vec<Effect> {
    if ui_store.selection.selected_actors.is_empty() {
        preview_store.preview.status = "No selection to zoom to".to_string();
        return vec![];
    }

    let bounds = &document_store.source.cached_actor_bounds;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has_any = false;

    for actor in &ui_store.selection.selected_actors {
        if let Some(rect) = bounds.get(actor) {
            min_x = min_x.min(rect.x0);
            min_y = min_y.min(rect.y0);
            max_x = max_x.max(rect.x1);
            max_y = max_y.max(rect.y1);
            has_any = true;
        }
    }

    if !has_any {
        preview_store.preview.status = "No spatial data for selection".to_string();
        return vec![];
    }

    let bounds_w = (max_x - min_x).max(1.0);
    let bounds_h = (max_y - min_y).max(1.0);
    let scene_w = document_store.source.document.scene_dimensions.width as f64;
    let scene_h = document_store.source.document.scene_dimensions.height as f64;

    // Compute zoom so the selection fills ~80% of the scene viewport.
    // zoom=1.0 means the full scene fits; zoom>1 means zoomed in.
    let zoom_x = scene_w / bounds_w * 0.8;
    let zoom_y = scene_h / bounds_h * 0.8;
    let zoom = (zoom_x.min(zoom_y)).clamp(0.5, 10.0) as f32;

    let center_x = ((min_x + max_x) / 2.0) as f32;
    let center_y = ((min_y + max_y) / 2.0) as f32;

    preview_store.preview.viewport.preview_zoom = zoom;
    preview_store.preview.viewport.preview_pan = egui::Vec2::new(center_x, center_y);
    preview_store.preview.status = format!("Zoomed to selection ({:.0}%)", zoom * 100.0);
    vec![Effect::Repaint]
}

pub fn handle_zoom_to_all(
    preview_store: &mut PreviewStore,
    document_store: &DocumentStore,
) -> Vec<Effect> {
    let scene_w = document_store.source.document.scene_dimensions.width as f32;
    let scene_h = document_store.source.document.scene_dimensions.height as f32;
    preview_store.preview.viewport.preview_zoom = 1.0;
    preview_store.preview.viewport.preview_pan = egui::Vec2::new(scene_w / 2.0, scene_h / 2.0);
    preview_store.preview.status = "Zoom to fit".to_string();
    vec![Effect::Repaint]
}

fn suggest_export_filename(
    export_store: &ExportStore,
    document_store: &DocumentStore,
) -> PathBuf {
    let ext = match export_store.export_state.format {
        crate::app::shell::export_dialog::ExportFormat::Image => "png",
        crate::app::shell::export_dialog::ExportFormat::Video => "mp4",
        crate::app::shell::export_dialog::ExportFormat::Gif => "gif",
        crate::app::shell::export_dialog::ExportFormat::WebM => "webm",
        crate::app::shell::export_dialog::ExportFormat::Mov => "mov",
        crate::app::shell::export_dialog::ExportFormat::WebP => "webp",
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
