use std::time::{SystemTime, UNIX_EPOCH};

use animatix::timeline::SceneDimensions;

use crate::app::commands::Effect;
use crate::app::document::plugins::DocumentPluginManager;
use crate::app::handlers::*;
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
fn parsed_stores(source: &str) -> (crate::app::test_support::TestTempDir, Stores) {
    let (fixture, document_store) = document_store_loaded(source);
    (
        fixture,
        (document_store, preview_store(5.0, SceneDimensions::default()), ui_store()),
    )
}

// ── TogglePlayback ───────────────────────────────────────────────

#[test]
fn toggle_playback_toggles_playing_flag() {
    let mut preview_store = preview_store(5.0, SceneDimensions::default());
    assert!(!preview_store.preview.playback.is_playing);
    let effects = playback::handle_toggle_playback(&mut preview_store);
    assert_eq!(effects.len(), 1, "expected exactly 1 effect");
    assert!(matches!(&effects[0], Effect::Repaint), "expected Repaint effect");
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
fn scrub_to_clamps_out_of_range_times() {
    for (case, requested, expected) in [("negative", -5.0, 0.0), ("overshoot", 999.0, 5.0)] {
        let (mut document_store, mut preview_store, ui_store) = stores();
        playback::handle_scrub_to(&mut document_store, &mut preview_store, &ui_store, requested);
        assert_eq!(preview_store.preview.playback.current_time_s, expected, "{case}");
    }
}

// ── ToggleEditorSync ─────────────────────────────────────────────

#[test]
fn toggle_editor_sync_updates_flag_and_status() {
    for (initial, expected, message) in [
        (true, false, "Editor sync OFF"),
        (false, true, "Editor sync ON"),
    ] {
        let mut ui_store = ui_store();
        ui_store.editor_sync_enabled = initial;
        let effects = playback::handle_toggle_editor_sync(&mut ui_store);
        assert_eq!(ui_store.editor_sync_enabled, expected, "initial sync={initial}");
        assert_eq!(effects.len(), 1);
        assert!(matches!(&effects[0], Effect::Status(msg) if msg == message), "{effects:?}");
    }
}

// ── ShowInspector ────────────────────────────────────────────────

#[test]
fn show_inspector_returns_empty() {
    let mut ui_store = ui_store();
    let effects = ui::handle_show_inspector(&mut ui_store);
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
    assert_eq!(document_store.source.editor.pending_scroll_to_line, Some(5));
}

const TEST_SOURCE: &str = r#"config { canvas_size: (1280, 720) }

box: Rect, at: (100, 100), size: (100, 100), color: blue
circle: Ellipse, at: (200, 200), radius_x: 50, radius_y: 50, color: red
"#;

// ── handle_save (filesystem) ───────────────────────────────────────
//
// NOTE: handle_save writes to the document's file_path on disk.
//       These tests use the temp files `document_store_loaded` writes.

#[test]
fn save_writes_editor_text_to_disk() {
    let (_fixture, (mut document_store, mut preview_store, _)) = parsed_stores(TEST_SOURCE);

    let original_text = document_store.source.editor.text().to_string();
    let modified_text = format!("{}\n// comment after save\n", original_text);
    document_store.source.editor.replace_text(modified_text.clone());
    // Sync source store with editor (as handle_editor_changed does)
    document_store.replace_text(modified_text.clone());

    file::handle_save(&mut document_store, &mut preview_store);

    let path = document_store.source.document.file_path.clone();
    let disk_content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(disk_content, modified_text, "disk content should match editor text");

    assert!(!document_store.source.document.is_dirty);

    assert_eq!(document_store.source.document.source_text, modified_text);
}

#[test]
fn save_unchanged_text_does_not_lose_data() {
    let (_fixture, (mut document_store, mut preview_store, _)) = parsed_stores(TEST_SOURCE);

    let original_text = document_store.source.editor.text().to_string();

    file::handle_save(&mut document_store, &mut preview_store);

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

    assert_eq!(document_store.source.document.file_path, path);

    assert!(!document_store.source.document.source_text.is_empty());
    assert!(document_store.source.document.source_text.contains("box:"));

    assert!(
        document_store.source.document.raw_statements.is_some(),
        "raw_statements should be parsed after open"
    );

    assert!(document_store.history.undo_stack.is_empty());
    assert!(document_store.history.redo_stack.is_empty());

    // Opening a document does not retain actor selection.
    assert!(ui_store.selection.selected_actors.is_empty());
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
    let (_fixture, (mut document_store, mut preview_store, mut ui_store)) =
        parsed_stores(MULTI_SCENE_SOURCE);

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

    assert_eq!(
        document_store.source.document.active_scene.as_deref(),
        Some("Diagram"),
        "active scene should be Diagram"
    );

    assert!(preview_store.preview_dirty);

    assert!(!preview_store.preview.playback.is_playing);

    assert!(
        effects.iter().any(|e| matches!(e, Effect::Status(_))),
        "expected Status effect, got {:?}",
        effects
    );
}

#[test]
fn select_scene_noop_for_invalid_scene() {
    let (_fixture, (mut document_store, mut preview_store, mut ui_store)) =
        parsed_stores(MULTI_SCENE_SOURCE);

    let effects = scene::handle_select_scene(
        &mut document_store,
        &mut preview_store,
        &mut ui_store,
        "NonExistent".into(),
    );

    assert_eq!(
        document_store.source.document.active_scene.as_deref(),
        Some("Intro"),
        "active scene should remain Intro"
    );

    assert!(effects.is_empty(), "expected no effects for invalid scene");
}

#[test]
fn select_scene_noop_without_composition() {
    let (_fixture, (mut document_store, mut preview_store, mut ui_store)) =
        parsed_stores(TEST_SOURCE);

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

    assert!(effects.is_empty(), "expected no effects without composition");
}
