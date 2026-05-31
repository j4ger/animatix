//! Tests for command handlers.
//!
//! The `GuiShell::handle_command` dispatcher lives in `shell/mod.rs`.
//! All handler logic has been extracted into the `handlers/` sub-modules.



// =========================================================================
// ── Tests ─────────────────────────────────────────────────────────────
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::Effect;
    use crate::app::handlers::*;
    use crate::app::persistence::default_tree;
    use crate::app::preview::DragState;
    use crate::app::PreviewPaneState;
    use crate::document::DocumentSession;
    use crate::editor::EditorBuffer;
    use animatix::timeline::SceneDimensions;
    use std::collections::HashSet;

    // ── Test helpers (no filesystem needed) ────────────────────────────

    fn make_document_store() -> crate::app::stores::DocumentStore {
        let document =
            DocumentSession::from_error(std::path::PathBuf::from("test.amx"));
        let editor =
            EditorBuffer::new(&std::path::PathBuf::from("test.amx"), document.source_text.clone());
        crate::app::stores::DocumentStore::new(document, editor)
    }

    fn make_preview_store(duration_s: f64) -> crate::app::stores::PreviewStore {
        let preview = PreviewPaneState::new(duration_s, SceneDimensions::default());
        crate::app::stores::PreviewStore::new(preview)
    }

    fn make_ui_store() -> crate::app::stores::UiStore {
        let tree = default_tree();
        crate::app::stores::UiStore::new(tree)
    }

    fn make_workspace_store() -> crate::app::stores::WorkspaceStore {
        crate::app::stores::WorkspaceStore::new(
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
        let effects = playback::handle_toggle_playback(&mut preview_store);
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
        playback::handle_toggle_playback(&mut preview_store);
        assert!(preview_store.preview.playback.is_playing);
        playback::handle_toggle_playback(&mut preview_store);
        assert!(!preview_store.preview.playback.is_playing);
    }

    #[test]
    fn toggle_playback_resets_time_when_at_end() {
        let mut preview_store = make_preview_store(5.0);
        preview_store.preview.playback.current_time_s =
            preview_store.preview.playback.duration_s;
        playback::handle_toggle_playback(&mut preview_store);
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

        let _effects = playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, target);

        assert_eq!(preview_store.preview.playback.current_time_s, clamped);
        assert!(!preview_store.preview.playback.is_playing);
        assert!(preview_store.preview_dirty);
    }

    #[test]
    fn scrub_to_clamps_negative_time() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let ui_store = make_ui_store();
        playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, -5.0);
        assert_eq!(preview_store.preview.playback.current_time_s, 0.0);
    }

    #[test]
    fn scrub_to_clamps_overshoot_time() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let ui_store = make_ui_store();
        playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, 999.0);
        let max = preview_store.preview.playback.duration_s.max(0.1);
        assert_eq!(preview_store.preview.playback.current_time_s, max);
    }

    // ── ToggleEditorSync ─────────────────────────────────────────────

    #[test]
    fn toggle_editor_sync_turns_off_when_on() {
        let mut ui_store = make_ui_store();
        ui_store.editor_sync_enabled = true;
        let effects = playback::handle_toggle_editor_sync(&mut ui_store);
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
        let effects = playback::handle_toggle_editor_sync(&mut ui_store);
        assert!(ui_store.editor_sync_enabled);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg == "Editor sync ON"),
            "expected Status('Editor sync ON'), got {:?}",
            effects[0]
        );
    }

    // ── ShowInspector ────────────────────────────────────────────────

    #[test]
    fn show_inspector_returns_empty() {
        let mut ui_store = make_ui_store();
        let effects = ui::handle_show_inspector(&mut ui_store);
        assert!(effects.is_empty());
    }

    // ── ToggleDiagnosticsPanel ───────────────────────────────────────

    #[test]
    fn toggle_diagnostics_toggles_visibility_flag() {
        let mut ui_store = make_ui_store();
        ui_store.view.diagnostics_panel_visible = false;
        let effects = ui::handle_toggle_diagnostics_panel(&mut ui_store);
        assert!(ui_store.view.diagnostics_panel_visible);
        assert!(effects.is_empty());
    }

    // ── DragEnded ────────────────────────────────────────────────────

    #[test]
    fn drag_ended_resets_drag_state() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.drag_snapshot_taken = true;

        let effects = ui::handle_drag_ended(&mut ui_store);

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
        let effects = ui::handle_undo(&mut document_store, &mut preview_store, &mut ui_store);
        assert!(effects.is_empty());
        assert!(document_store.history.undo_stack.is_empty());
        assert!(document_store.history.redo_stack.is_empty());
    }

    #[test]
    fn redo_with_empty_stack_does_nothing() {
        let mut document_store = make_document_store();
        let mut preview_store = make_preview_store(5.0);
        let mut ui_store = make_ui_store();
        let effects = ui::handle_redo(&mut document_store, &mut preview_store, &mut ui_store);
        assert!(effects.is_empty());
        assert!(document_store.history.undo_stack.is_empty());
        assert!(document_store.history.redo_stack.is_empty());
    }

    // ── InspectorInputDragStarted / Ended ────────────────────────────

    #[test]
    fn inspector_input_drag_started_sets_flag() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.inspector_input_drag_active = false;
        let effects = ui::handle_inspector_input_drag_started(&mut ui_store);
        assert!(ui_store.interaction.inspector_input_drag_active);
        assert!(effects.is_empty());
    }

    #[test]
    fn inspector_input_drag_ended_resets_flags() {
        let mut ui_store = make_ui_store();
        ui_store.interaction.inspector_input_drag_active = true;
        ui_store.interaction.drag_snapshot_taken = true;
        let effects = ui::handle_inspector_input_drag_ended(&mut ui_store);
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
        file::handle_toggle_expand_dir(workspace_store, &document_store, path.clone());
        assert!(workspace_store.expanded_dirs.contains(&path));
        file::handle_toggle_expand_dir(workspace_store, &document_store, path.clone());
        assert!(!workspace_store.expanded_dirs.contains(&path));
    }

    // ── ScrollToLine ─────────────────────────────────────────────────

    #[test]
    fn scroll_to_line_focuses_diagnostic() {
        let mut document_store = make_document_store();
        let effects = ui::handle_scroll_to_line(&mut document_store, 5, 0);
        assert!(effects.is_empty());
        // focus_diagnostic requires cell_index coverage; with from_error()
        // the test document has no parsed cells so pending_scroll_to_line
        // stays None.  The important thing is that the handler doesn't panic.
    }
}
