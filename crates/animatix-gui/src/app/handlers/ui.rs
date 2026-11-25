use std::path::PathBuf;

use crate::app::commands::{Effect, UndoEntry};
use crate::app::components::toast::Toast;
use crate::app::stores::{DocumentStore, ExportStore, PreviewStore, UiStore};

/// Toggle the on-demand Inspector. An unpinned Inspector overlays the preview;
/// a pinned Inspector remains in the dock until explicitly closed.
pub fn handle_show_inspector(ui_store: &mut UiStore) -> Vec<Effect> {
    use crate::app::{panels::CompactDrawer, persistence};
    let view = &mut ui_store.view;
    if !view.compact && persistence::detail_visible(&view.tree) {
        persistence::set_detail_visible(&mut view.tree, false);
    } else if view.compact_drawer == Some(CompactDrawer::Detail) {
        view.compact_drawer = None;
        if let Some(restore) = view.compact_restore.as_mut() {
            restore.detail_visible = false;
        }
    } else {
        view.compact_drawer = Some(CompactDrawer::Detail);
    }
    vec![]
}

pub fn handle_show_code(ui_store: &mut UiStore) -> Vec<Effect> {
    use crate::app::LayoutPreset;
    let next = match ui_store.view.layout_preset {
        LayoutPreset::Code => LayoutPreset::Animate,
        LayoutPreset::Animate => LayoutPreset::Code,
    };
    handle_apply_layout(ui_store, next)
}

pub fn handle_show_curves(ui_store: &mut UiStore) -> Vec<Effect> {
    crate::app::persistence::toggle_curve_comparison(&mut ui_store.view.tree);
    vec![]
}

pub fn handle_show_timeline(ui_store: &mut UiStore) -> Vec<Effect> {
    crate::app::persistence::show_timeline(&mut ui_store.view.tree);
    vec![]
}

/// Apply a named layout preset to the current tree.
pub fn handle_apply_layout(
    ui_store: &mut UiStore,
    preset: crate::app::LayoutPreset,
) -> Vec<Effect> {
    let (w, h) = ui_store.view.layout_size;
    if !crate::app::persistence::apply_layout_preset(&mut ui_store.view.tree, preset, w, h) {
        ui_store.view.tree = crate::app::persistence::build_tree_for(preset, w, h);
    }
    // Re-shape for compact: the preset must not un-hide the detail dock, which
    // renders as an overlay drawer in that mode. Refresh the captured restore
    // state first so widening later reflects *this* preset's intent, not the
    // layout that was on screen when compact mode began.
    if ui_store.view.compact {
        let mut restore = crate::app::persistence::capture_compact_restore(&ui_store.view.tree);
        if let Some(prior) = ui_store.view.compact_restore {
            restore.detail_visible = prior.detail_visible;
        }
        ui_store.view.compact_restore = Some(restore);
        crate::app::persistence::apply_compact_entry(&mut ui_store.view.tree);
    }
    ui_store.view.layout_preset = preset;
    ui_store.view.preview_fullscreen = false;
    vec![]
}

/// Rebuild the default Animate layout.
pub fn handle_reset_layout(ui_store: &mut UiStore) -> Vec<Effect> {
    let (w, h) = ui_store.view.layout_size;
    ui_store.view.tree =
        crate::app::persistence::build_tree_for(crate::app::LayoutPreset::Animate, w, h);
    if ui_store.view.compact {
        // Refresh the restore target to the fresh Animate layout (see
        // `handle_apply_layout`).
        ui_store.view.compact_restore =
            Some(crate::app::persistence::capture_compact_restore(&ui_store.view.tree));
        crate::app::persistence::apply_compact_entry(&mut ui_store.view.tree);
    }
    ui_store.view.layout_preset = crate::app::LayoutPreset::Animate;
    ui_store.view.preview_fullscreen = false;
    ui_store.view.compact_drawer = None;
    vec![]
}

pub fn handle_open_export_dialog(
    export_store: &mut ExportStore,
    document_store: &DocumentStore,
) -> Vec<Effect> {
    export_store.export_dialog_open = true;
    let state = &mut export_store.export_state;
    if state.follow_canvas {
        let canvas = document_store.source.document.scene_dimensions;
        state.width = canvas.width;
        state.height = canvas.height;
    }
    if export_store.export_state.output_path.is_empty() {
        let path = suggest_export_filename(export_store, document_store);
        export_store.export_state.output_path = path.to_string_lossy().to_string();
    }
    vec![]
}

pub fn handle_undo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if document_store.history.undo_stack.is_empty() {
        ui_store.toasts.push(Toast::info(
            crate::app::i18n::StatusText::message("gui-status-no-undo", [])
                .display(ui_store.view.language)
                .into_owned(),
        ));
        return vec![];
    }
    if let Some(entry) = document_store.history.undo_stack.pop_back() {
        // Push current state onto redo stack with correctly ordered before/after.
        let ui_after = ui_store.snapshot_with_preview(preview_store);
        document_store.history.redo_stack.push_back(UndoEntry::new(
            entry.command.clone(),
            entry.source_before.clone(),
            document_store.source.text().to_string(),
            entry.ui_before.clone(),
            ui_after.clone(),
        ));
        // Restore source via SourceStore to update epoch and invalidate caches.
        document_store.replace_text_with_ui(entry.source_before.clone(), ui_after);
        // Restore UI state from the recorded before-snapshot.
        ui_store.restore_snapshot(entry.ui_before.clone());
        preview_store.preview.playback.restore_snapshot_state(
            entry.ui_before.playhead_time_s,
            entry.ui_before.loop_start_s,
            entry.ui_before.loop_end_s,
        );
        preview_store.preview.timeline_scroll_offset = entry.ui_before.timeline_scroll_offset;
        preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(ui_store.rebuild_debounce_ms),
        );
        preview_store.preview.status = crate::app::i18n::StatusText::message(
            "gui-status-undo",
            [("action", entry.label.to_string())],
        );
        ui_store.toasts.push(Toast::info(
            preview_store.preview.status.display(ui_store.view.language).into_owned(),
        ));
    }
    vec![]
}

pub fn handle_redo(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if document_store.history.redo_stack.is_empty() {
        ui_store.toasts.push(Toast::info(
            crate::app::i18n::StatusText::message("gui-status-no-redo", [])
                .display(ui_store.view.language)
                .into_owned(),
        ));
        return vec![];
    }
    if let Some(entry) = document_store.history.redo_stack.pop_back() {
        // Push current state onto undo stack with correctly ordered before/after.
        let ui_after = ui_store.snapshot_with_preview(preview_store);
        document_store.history.undo_stack.push_back(UndoEntry::new(
            entry.command.clone(),
            entry.source_before.clone(),
            document_store.source.text().to_string(),
            entry.ui_before.clone(),
            ui_after.clone(),
        ));
        // Restore source via SourceStore to update epoch and invalidate caches.
        document_store.replace_text_with_ui(entry.source_after.clone(), ui_after);
        // Restore UI state from the recorded after-snapshot.
        ui_store.restore_snapshot(entry.ui_after.clone());
        preview_store.preview.playback.restore_snapshot_state(
            entry.ui_after.playhead_time_s,
            entry.ui_after.loop_start_s,
            entry.ui_after.loop_end_s,
        );
        preview_store.preview.timeline_scroll_offset = entry.ui_after.timeline_scroll_offset;
        preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(ui_store.rebuild_debounce_ms),
        );
        preview_store.preview.status = crate::app::i18n::StatusText::message(
            "gui-status-redo",
            [("action", entry.label.to_string())],
        );
        ui_store.toasts.push(Toast::info(
            preview_store.preview.status.display(ui_store.view.language).into_owned(),
        ));
    }
    vec![]
}

pub fn handle_scroll_to_line(
    document_store: &mut DocumentStore,
    line: usize,
    column: usize,
) -> Vec<Effect> {
    document_store.source.editor.focus_diagnostic(line, column);
    document_store.source.editor.set_highlighted_line(Some(line));
    vec![]
}

pub fn handle_zoom_to_selection(
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    document_store: &DocumentStore,
) -> Vec<Effect> {
    if ui_store.selection.selected_actors.is_empty() {
        preview_store.preview.status =
            crate::app::i18n::StatusText::message("gui-status-zoom-no-selection", []);
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
        preview_store.preview.status =
            crate::app::i18n::StatusText::message("gui-status-zoom-no-spatial-data", []);
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
    preview_store.preview.status = crate::app::i18n::StatusText::message(
        "gui-status-zoom-selection",
        [("percent", format!("{:.0}", zoom * 100.0))],
    );
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
    preview_store.preview.status = crate::app::i18n::StatusText::message("gui-status-zoom-fit", []);
    vec![Effect::Repaint]
}

fn suggest_export_filename(export_store: &ExportStore, document_store: &DocumentStore) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{WorkspaceTab, test_support::ui_store};

    #[test]
    fn curve_comparison_does_not_replace_the_inline_timeline() {
        use crate::app::persistence::pane_visible;
        let mut ui_store = ui_store();
        assert!(ui_store.view.tree.tiles.find_pane(&WorkspaceTab::Curves).is_none());
        handle_show_curves(&mut ui_store);
        assert!(ui_store.view.tree.tiles.find_pane(&WorkspaceTab::Curves).is_some());
        handle_show_curves(&mut ui_store);
        assert!(ui_store.view.tree.tiles.find_pane(&WorkspaceTab::Curves).is_none());
        assert!(pane_visible(&ui_store.view.tree, WorkspaceTab::Timeline));
    }

    #[test]
    fn inspector_overlay_and_source_editor_are_independent() {
        use crate::app::{panels::CompactDrawer, persistence};
        let mut ui_store = ui_store();
        handle_show_inspector(&mut ui_store);
        assert_eq!(ui_store.view.compact_drawer, Some(CompactDrawer::Detail));
        assert!(!persistence::detail_visible(&ui_store.view.tree));
        handle_show_code(&mut ui_store);
        assert!(persistence::pane_visible(&ui_store.view.tree, WorkspaceTab::Code));
        assert_eq!(ui_store.view.compact_drawer, Some(CompactDrawer::Detail));
        handle_show_inspector(&mut ui_store);
        assert!(ui_store.view.compact_drawer.is_none());
    }
}
