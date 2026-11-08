use std::time::{SystemTime, UNIX_EPOCH};

use animatix::timeline::SceneDimensions;
use animatix_syntax::ast::Stmt;

use crate::app::commands::Effect;
use crate::app::document::plugins::DocumentPluginManager;
use crate::app::handlers::*;
use crate::app::preview::DragState;
use crate::app::test_support::{
    document_store_empty, document_store_loaded, preview_store, ui_store, workspace_store,
};

// ── Test helpers ───────────────────────────────────────────────────

type Stores = (
    crate::app::stores::DocumentStore,
    crate::app::stores::PreviewStore,
    crate::app::stores::UiStore,
);

/// The standard empty `(document, preview, ui)` triple.
fn stores() -> Stores {
    (
        document_store_empty(),
        preview_store(5.0, SceneDimensions::default()),
        ui_store(),
    )
}

/// Same triple over a fully parsed document.
fn parsed_stores(source: &str) -> Stores {
    (
        document_store_loaded(source),
        preview_store(5.0, SceneDimensions::default()),
        ui_store(),
    )
}

// ── TogglePlayback ───────────────────────────────────────────────

#[test]
fn toggle_playback_returns_repaint_effect() {
    let mut preview_store = preview_store(5.0, SceneDimensions::default());
    let effects = playback::handle_toggle_playback(&mut preview_store);
    assert_eq!(effects.len(), 1, "expected exactly 1 effect");
    assert!(matches!(&effects[0], Effect::Repaint), "expected Repaint effect");
}

#[test]
fn toggle_playback_toggles_playing_flag() {
    let mut preview_store = preview_store(5.0, SceneDimensions::default());
    assert!(!preview_store.preview.playback.is_playing);
    playback::handle_toggle_playback(&mut preview_store);
    assert!(preview_store.preview.playback.is_playing);
    playback::handle_toggle_playback(&mut preview_store);
    assert!(!preview_store.preview.playback.is_playing);
}

#[test]
fn toggle_playback_resets_time_when_at_end() {
    let mut preview_store = preview_store(5.0, SceneDimensions::default());
    preview_store.preview.playback.current_time_s = preview_store.preview.playback.duration_s;
    playback::handle_toggle_playback(&mut preview_store);
    assert_eq!(preview_store.preview.playback.current_time_s, 0.0);
    assert!(preview_store.preview.playback.is_playing);
}

// ── ScrubTo ──────────────────────────────────────────────────────

#[test]
fn scrub_to_updates_current_time_and_stops_playback() {
    let (mut document_store, mut preview_store, ui_store) = stores();
    let target = preview_store.preview.playback.duration_s * 0.5;
    let clamped = target.max(0.0).min(preview_store.preview.playback.duration_s.max(0.1));
    preview_store.preview.playback.is_playing = true;

    let _effects =
        playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, target);

    assert_eq!(preview_store.preview.playback.current_time_s, clamped);
    assert!(!preview_store.preview.playback.is_playing);
    assert!(preview_store.preview_dirty);
}

#[test]
fn scrub_to_clamps_negative_time() {
    let (mut document_store, mut preview_store, ui_store) = stores();
    playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, -5.0);
    assert_eq!(preview_store.preview.playback.current_time_s, 0.0);
}

#[test]
fn scrub_to_clamps_overshoot_time() {
    let (mut document_store, mut preview_store, ui_store) = stores();
    playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, 999.0);
    let max = preview_store.preview.playback.duration_s.max(0.1);
    assert_eq!(preview_store.preview.playback.current_time_s, max);
}

// ── ToggleEditorSync ─────────────────────────────────────────────

#[test]
fn toggle_editor_sync_turns_off_when_on() {
    let mut ui_store = ui_store();
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
    let mut ui_store = ui_store();
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
    let mut ui_store = ui_store();
    let effects = ui::handle_show_inspector(&mut ui_store);
    assert!(effects.is_empty());
}

// ── DragEnded ────────────────────────────────────────────────────

#[test]
fn drag_ended_resets_drag_state() {
    let mut ui_store = ui_store();
    ui_store.interaction.drag_snapshot_taken = true;
    ui_store.interaction.inspector_input_drag_active = true;

    let effects = ui::handle_drag_ended(&mut ui_store);

    assert!(
        matches!(ui_store.interaction.drag_state, DragState::None),
        "expected DragState::None after DragEnded"
    );
    assert!(!ui_store.interaction.drag_snapshot_taken);
    assert!(!ui_store.interaction.inspector_input_drag_active);
    assert!(effects.is_empty());
}

// ── Undo / Redo ──────────────────────────────────────────────────

#[test]
fn undo_with_empty_stack_does_nothing() {
    let (mut document_store, mut preview_store, mut ui_store) = stores();
    let effects = ui::handle_undo(&mut document_store, &mut preview_store, &mut ui_store);
    assert!(effects.is_empty());
    assert!(document_store.history.undo_stack.is_empty());
    assert!(document_store.history.redo_stack.is_empty());
}

#[test]
fn redo_with_empty_stack_does_nothing() {
    let (mut document_store, mut preview_store, mut ui_store) = stores();
    let effects = ui::handle_redo(&mut document_store, &mut preview_store, &mut ui_store);
    assert!(effects.is_empty());
    assert!(document_store.history.undo_stack.is_empty());
    assert!(document_store.history.redo_stack.is_empty());
}

// ── InspectorInputDragStarted / Ended ────────────────────────────

#[test]
fn inspector_input_drag_started_sets_flag() {
    let mut ui_store = ui_store();
    ui_store.interaction.inspector_input_drag_active = false;
    let effects = ui::handle_inspector_input_drag_started(&mut ui_store);
    assert!(ui_store.interaction.inspector_input_drag_active);
    assert!(effects.is_empty());
}

#[test]
fn inspector_input_drag_ended_resets_flags() {
    let mut ui_store = ui_store();
    ui_store.interaction.inspector_input_drag_active = true;
    ui_store.interaction.drag_snapshot_taken = true;
    let effects = ui::handle_inspector_input_drag_ended(&mut ui_store);
    assert!(!ui_store.interaction.inspector_input_drag_active);
    assert!(!ui_store.interaction.drag_snapshot_taken);
    assert!(
        matches!(ui_store.interaction.drag_state, DragState::None),
        "expected DragState::None after InspectorInputDragEnded"
    );
    assert!(effects.is_empty());
}

// ── ToggleExpandDir ──────────────────────────────────────────────

#[test]
fn toggle_expand_dir_toggles_path_in_set() {
    let workspace_store = &mut workspace_store();
    let document_store = document_store_empty();
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
    let mut document_store = document_store_empty();
    let effects = ui::handle_scroll_to_line(&mut document_store, 5, 0);
    assert!(effects.is_empty());
    // focus_diagnostic requires cell_index coverage; with from_error()
    // the test document has no parsed cells so pending_scroll_to_line
    // stays None.  The important thing is that the handler doesn't panic.
}

// ── Domain handler test helpers ─────────────────────────────────────

const TEST_SOURCE: &str = r#"config { resolution: (1280, 720) }

box: Rect, at: (100, 100), size: (100, 100), color: blue
circle: Ellipse, at: (200, 200), radius: 50, color: red
"#;

#[test]
fn set_transition_without_ast_aborts_pending_snapshot() {
    let (mut document_store, mut preview_store, mut ui_store) = stores();

    property::handle_set_transition(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "Intro".to_string(),
        animatix_syntax::ast::Transition {
            id: "fade".to_string(),
            duration_ms: 300,
            easing: animatix_syntax::easing::Easing::Linear,
        },
    );

    assert!(
        document_store.history.undo_stack.is_empty(),
        "failed source edit should not leave a pending undo snapshot"
    );
    assert!(document_store.pending_snapshot_is_none());
}

// ── handle_create_actor ────────────────────────────────────────────

#[test]
fn create_actor_finalizes_undo_snapshot_and_undo_restores_source() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);
    let original_source = document_store.source.document.source_text.clone();
    preview_store.preview.playback.scrub_to(2.0);

    actor::handle_create_actor(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "Rect".into(),
        "undo_box".into(),
        [300.0, 300.0],
        vec![],
    );

    assert_eq!(
        document_store.history.undo_stack.len(),
        1,
        "create actor should finalize one undo snapshot"
    );
    assert!(
        document_store.source.document.source_text.contains("undo_box"),
        "source should contain the new actor"
    );

    ui::handle_undo(&mut document_store, &mut preview_store, &mut ui_store);
    assert_eq!(
        document_store.source.document.source_text, original_source,
        "undo should restore the original source"
    );
    assert!(
        !ui_store.selection.selected_actors.contains("undo_box"),
        "undo should clear the actor selection"
    );
    assert!((preview_store.preview.playback.current_time_s() - 2.0).abs() < 1e-9);
}

#[test]
fn create_actor_adds_to_ast_and_updates_source() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

    actor::handle_create_actor(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "Rect".into(),
        "new_box".into(),
        [300.0, 300.0],
        vec![],
    );

    // Actor should be in raw_statements
    let stmts = document_store
        .source
        .document
        .raw_statements
        .as_ref()
        .expect("raw_statements should exist");
    assert!(
        stmts
            .iter()
            .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "new_box")),
        "expected 'new_box' in raw_statements"
    );

    // Source text should contain the new actor
    assert!(
        document_store.source.document.source_text.contains("new_box"),
        "source_text should contain 'new_box'"
    );

    // Document should be dirty
    assert!(document_store.source.document.is_dirty);

    // New actor should be selected
    assert!(
        ui_store.selection.selected_actors.contains("new_box"),
        "'new_box' should be selected"
    );

    // Status should mention creation
    assert!(
        preview_store.preview.status.contains("Created"),
        "status should contain 'Created', got: {}",
        preview_store.preview.status
    );
}

// ── handle_rename_actor ────────────────────────────────────────────

#[test]
fn rename_actor_successfully_changes_label() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

    ui_store.selection.selected_actors.insert("box".to_string());

    actor::handle_rename_actor(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "box".into(),
        "big_box".into(),
    );

    // Old label should be gone, new label should exist
    let stmts = document_store
        .source
        .document
        .raw_statements
        .as_ref()
        .expect("raw_statements should exist");
    assert!(
        !stmts
            .iter()
            .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "box")),
        "old label 'box' should be removed"
    );
    assert!(
        stmts
            .iter()
            .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "big_box")),
        "new label 'big_box' should exist"
    );

    // Status should mention rename
    assert!(
        preview_store.preview.status.contains("Renamed"),
        "status should contain 'Renamed', got: {}",
        preview_store.preview.status
    );

    // Selection should be updated
    assert!(
        ui_store.selection.selected_actors.contains("big_box"),
        "selection should contain 'big_box'"
    );
    assert!(
        !ui_store.selection.selected_actors.contains("box"),
        "selection should not contain 'box'"
    );
}

#[test]
fn rename_actor_noop_when_old_equals_new() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

    let source_before = document_store.source.document.source_text.clone();

    actor::handle_rename_actor(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "box".into(),
        "box".into(),
    );

    // Source should be unchanged
    assert_eq!(
        document_store.source.document.source_text, source_before,
        "source should not change for no-op rename"
    );
}

#[test]
fn rename_actor_fails_with_invalid_label() {
    // Cases:
    // 1) empty label rejected with 'empty' status
    // 2) duplicate existing label rejected with 'already exists' status
    for (new_name, expected_status) in [("", "empty"), ("circle", "already exists")] {
        let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);
        let source_before = document_store.source.document.source_text.clone();

        actor::handle_rename_actor(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
            "box".into(),
            new_name.into(),
        );

        // Status should mention failure reason
        assert!(
            preview_store.preview.status.contains(expected_status),
            "status should mention '{expected_status}', got: {}",
            preview_store.preview.status
        );

        // Source should be unchanged
        assert_eq!(
            document_store.source.document.source_text, source_before,
            "source should not change on failure"
        );
    }
}

// ── handle_delete_selected_actors ──────────────────────────────────

#[test]
fn delete_selected_actors_removes_from_ast() {
    // Cases:
    // 1) single actor selection removes that actor and keeps others
    // 2) multiple actor selection removes all selected actors
    for (to_delete, should_remain) in [
        (vec!["box"], vec!["circle"]),
        (vec!["box", "circle"], vec![]),
    ] {
        let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

        for label in &to_delete {
            ui_store.selection.selected_actors.insert(label.to_string());
        }

        actor::handle_delete_selected_actors(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
        );

        // Actors should be removed from raw_statements
        let stmts = document_store
            .source
            .document
            .raw_statements
            .as_ref()
            .expect("raw_statements should exist");
        for deleted in &to_delete {
            assert!(
                !stmts
                    .iter()
                    .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == *deleted)),
                "'{deleted}' should be removed from raw_statements"
            );
        }

        // Other actors should remain
        for remaining in &should_remain {
            assert!(
                stmts
                    .iter()
                    .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == *remaining)),
                "'{remaining}' should still be present"
            );
        }

        // Selection should be cleared
        assert!(
            ui_store.selection.selected_actors.is_empty(),
            "selection should be empty after deletion"
        );

        // Source should be dirty
        assert!(document_store.source.document.is_dirty);

        // Status should mention deletion
        assert!(
            preview_store.preview.status.contains("Deleted"),
            "status should contain 'Deleted', got: {}",
            preview_store.preview.status
        );
    }
}

#[test]
fn delete_selected_actors_with_empty_selection_is_noop() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

    let source_before = document_store.source.document.source_text.clone();

    actor::handle_delete_selected_actors(&mut document_store, &mut preview_store, &mut ui_store);

    // Source should be unchanged since nothing was selected
    assert_eq!(
        document_store.source.document.source_text, source_before,
        "source should not change with empty selection"
    );
}

// ── handle_save (filesystem) ───────────────────────────────────────
//
// NOTE: handle_save writes to the document's file_path on disk.
//       These tests use the temp files `document_store_loaded` writes.

#[test]
fn save_writes_editor_text_to_disk() {
    let (mut document_store, mut preview_store, _) = parsed_stores(TEST_SOURCE);

    let original_text = document_store.source.editor.text().to_string();
    let modified_text = format!("{}\n// comment after save\n", original_text);
    document_store.source.editor.replace_text(modified_text.clone());
    // Sync source store with editor (as handle_editor_changed does)
    document_store.replace_text(modified_text.clone());

    file::handle_save(&mut document_store, &mut preview_store);

    // Verify the file on disk matches editor text
    let path = document_store.source.document.file_path.clone();
    let disk_content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(disk_content, modified_text, "disk content should match editor text");

    // Document should not be dirty after save
    assert!(!document_store.source.document.is_dirty);

    // Source text should be updated
    assert_eq!(document_store.source.document.source_text, modified_text);
}

#[test]
fn save_unchanged_text_does_not_lose_data() {
    let (mut document_store, mut preview_store, _) = parsed_stores(TEST_SOURCE);

    let original_text = document_store.source.editor.text().to_string();

    file::handle_save(&mut document_store, &mut preview_store);

    // File content should still match original
    let path = document_store.source.document.file_path.clone();
    let disk_content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(disk_content, original_text, "disk content should match original");
    assert!(!document_store.source.document.is_dirty);
}

// ── handle_open_file (filesystem) ──────────────────────────────────
//
// NOTE: handle_open_file reads from disk.  We test by opening a known
//       temp file and verifying the document state.

#[test]
fn open_file_populates_document_state() {
    let (mut document_store, mut preview_store, mut ui_store) = stores();
    let mut workspace_store = workspace_store();

    // Create a temp file with known content
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("animatix_test_open_{}_{}", std::process::id(), unique));
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    let path = dir.join("open_test.amx");
    std::fs::write(&path, TEST_SOURCE).expect("write test source");
    let mut plugin_manager = DocumentPluginManager::new(path.clone(), dir.clone());

    file::handle_open_file(
        &mut document_store,
        &mut workspace_store,
        &mut preview_store,
        &mut ui_store,
        &mut plugin_manager,
        path.clone(),
    );

    // Document should have the correct file path
    assert_eq!(document_store.source.document.file_path, path);

    // Source text should be populated
    assert!(!document_store.source.document.source_text.is_empty());
    assert!(document_store.source.document.source_text.contains("box:"));

    // Raw statements should be parsed
    assert!(
        document_store.source.document.raw_statements.is_some(),
        "raw_statements should be parsed after open"
    );

    // History should be cleared
    assert!(document_store.history.undo_stack.is_empty());
    assert!(document_store.history.redo_stack.is_empty());

    // UI state should be reset
    assert!(
        matches!(ui_store.interaction.drag_state, DragState::None),
        "drag state should be None after open"
    );
}

// ── Scene selection ───────────────────────────────────────────────

const MULTI_SCENE_SOURCE: &str = r#"
# Intro
#0s
title: Text, text: "Welcome"

# Diagram
#0s
graph: Rect, size: (400, 400)
"#;

#[test]
fn select_scene_switches_active_scene_and_seeks() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(MULTI_SCENE_SOURCE);

    // Should be a composition with two scenes
    assert!(
        document_store.source.document.composition.is_some(),
        "document should be a composition"
    );
    assert_eq!(
        document_store.source.document.active_scene.as_deref(),
        Some("Intro"),
        "default active scene should be Intro"
    );

    let effects = scene::handle_select_scene(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "Diagram".into(),
    );

    // Active scene should switch
    assert_eq!(
        document_store.source.document.active_scene.as_deref(),
        Some("Diagram"),
        "active scene should be Diagram"
    );

    // Preview should be dirty
    assert!(preview_store.preview_dirty);

    // Playback should not be playing
    assert!(!preview_store.preview.playback.is_playing);

    // Should return a status effect
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Status(_))),
        "expected Status effect, got {:?}",
        effects
    );
}

#[test]
fn select_scene_noop_for_invalid_scene() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(MULTI_SCENE_SOURCE);

    let effects = scene::handle_select_scene(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "NonExistent".into(),
    );

    // Active scene should remain unchanged
    assert_eq!(
        document_store.source.document.active_scene.as_deref(),
        Some("Intro"),
        "active scene should remain Intro"
    );

    // Should return empty effects for invalid scene
    assert!(effects.is_empty(), "expected no effects for invalid scene");
}

#[test]
fn select_scene_noop_without_composition() {
    let (mut document_store, mut preview_store, mut ui_store) = parsed_stores(TEST_SOURCE);

    // Single-scene document has no composition
    assert!(
        document_store.source.document.composition.is_none(),
        "TEST_SOURCE should not be a composition"
    );

    let effects = scene::handle_select_scene(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "Intro".into(),
    );

    // Should return empty effects
    assert!(effects.is_empty(), "expected no effects without composition");
}
