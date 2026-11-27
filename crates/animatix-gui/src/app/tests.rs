use std::path::PathBuf;

use animatix::timeline::SceneDimensions;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use egui::Vec2;

use super::{
    GuiShell, LayoutPreset, WorkspaceTab, default_tree, diagnostics_banner_message,
    diagnostics_summary_color, fit_preview, has_source_load_failure, preview,
    primary_diagnostic_phase,
};
use crate::app::design_tokens::semantic::status::DIAGNOSTIC_ERROR;

#[test]
fn opening_a_relative_scene_reveals_its_deep_workspace_path() {
    let cwd = std::env::current_dir().unwrap();
    let workspace = tempfile::tempdir_in(&cwd).unwrap();
    std::fs::create_dir(workspace.path().join(".git")).unwrap();
    let dir = workspace.path().join("scenes/one/two/three/four/five");
    std::fs::create_dir_all(&dir).unwrap();
    let scene = dir.join("main.amx");
    std::fs::write(&scene, "#0s\nbox: Rect\n").unwrap();

    let mut shell = GuiShell::load(scene.strip_prefix(cwd).unwrap().to_path_buf(), false);
    assert_eq!(shell.workspace_store.workspace_root, workspace.path());
    shell.workspace_store.file_tree = shell.workspace_store.file_tree_loader.wait();
    assert!(
        shell
            .workspace_store
            .file_tree
            .iter()
            .any(|entry| { entry.path == scene && !entry.is_dir && entry.depth == 6 })
    );

    shell.handle_command(super::Command::ToggleExpandDir(dir));
    shell.workspace_store.file_tree = shell.workspace_store.file_tree_loader.wait();
    assert!(shell.workspace_store.file_tree.iter().all(|entry| entry.path != scene));
}

#[test]
fn switching_workspaces_updates_the_project_tree_on_a_frame() {
    let original = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    std::fs::create_dir(original.path().join(".git")).unwrap();
    let scene = original.path().join("main.amx");
    let other = destination.path().join("other.amx");
    for path in [&scene, &other] {
        std::fs::write(path, "#0s\nbox: Rect\n").unwrap();
    }

    let mut shell = GuiShell::load(scene.clone(), false);
    shell.workspace_store.file_tree = shell.workspace_store.file_tree_loader.wait();
    assert!(shell.workspace_store.file_tree.iter().any(|entry| entry.path == scene));

    let (notify, ready) = crossbeam_channel::bounded(1);
    shell.workspace_store.file_tree_loader.set_repaint(move || {
        let _ = notify.try_send(());
    });
    shell.handle_command(super::Command::SwitchWorkspace(destination.path().to_path_buf()));
    assert!(shell.workspace_store.file_tree.is_empty());
    assert!(shell.workspace_store.file_tree_loader.pending());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while shell.workspace_store.file_tree_loader.pending() {
        ready.recv_deadline(deadline).unwrap();
        shell.prepare_frame();
    }
    assert_eq!(shell.workspace_store.file_tree.len(), 1);
    assert_eq!(shell.workspace_store.file_tree[0].path, other);
}

#[test]
fn review_uses_three_visible_panes_and_write_adds_the_shared_editor() {
    use super::persistence::{apply_layout_preset, pane_visible, show_inspector};
    let mut tree = default_tree();
    for tab in [
        WorkspaceTab::Sidebar,
        WorkspaceTab::Preview,
        WorkspaceTab::Timeline,
    ] {
        assert!(pane_visible(&tree, tab));
    }
    assert!(!pane_visible(&tree, WorkspaceTab::Code));
    assert!(!pane_visible(&tree, WorkspaceTab::Inspector));
    assert!(tree.tiles.find_pane(&WorkspaceTab::Curves).is_none());
    let timeline = tree.tiles.find_pane(&WorkspaceTab::Timeline).unwrap();
    assert_eq!(tree.tiles.parent_of(timeline), tree.root);
    show_inspector(&mut tree);
    assert!(apply_layout_preset(&mut tree, LayoutPreset::Code, 1440.0, 960.0));
    assert!(pane_visible(&tree, WorkspaceTab::Code));
    assert!(pane_visible(&tree, WorkspaceTab::Inspector));
    assert!(apply_layout_preset(&mut tree, LayoutPreset::Animate, 1440.0, 960.0));
    assert!(!pane_visible(&tree, WorkspaceTab::Code));
    assert!(pane_visible(&tree, WorkspaceTab::Inspector), "keep the user's pin");
}

#[test]
fn preview_fit_uses_scene_dimensions_aspect_ratio() {
    let fitted = fit_preview(
        SceneDimensions {
            width: 1000,
            height: 1000,
        },
        Vec2::new(400.0, 200.0),
    );
    assert!((fitted.x - fitted.y).abs() < 0.001);
}

#[test]
fn timeline_fraction_clamps_to_bounds() {
    assert_eq!(preview::timeline_fraction(-1.0, 10.0), 0.0);
    assert_eq!(preview::timeline_fraction(5.0, 10.0), 0.5);
    assert_eq!(preview::timeline_fraction(20.0, 10.0), 1.0);
}

#[test]
fn pointer_position_maps_to_scrub_time() {
    let rect = egui::Rect::from_min_max(egui::pos2(10.0, 0.0), egui::pos2(210.0, 20.0));
    assert_eq!(preview::time_from_pointer_x(rect, 10.0, 8.0), 0.0);
    assert!((preview::time_from_pointer_x(rect, 110.0, 8.0) - 4.0).abs() < 0.001);
    assert_eq!(preview::time_from_pointer_x(rect, 210.0, 8.0), 8.0);
}

#[test]
fn diagnostics_summary_color_turns_red_when_errors_exist() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::SourceLoadFailure,
        DiagnosticPhase::Parse,
        "parse failed",
    )];

    assert_eq!(diagnostics_summary_color(&diagnostics), DIAGNOSTIC_ERROR);
}

#[test]
fn has_source_load_failure_detects_blocking_parse_entries() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::SourceLoadFailure,
        DiagnosticPhase::Parse,
        "parse failed",
    )];

    assert!(has_source_load_failure(&diagnostics));
}

#[test]
fn primary_diagnostic_phase_prefers_parse_then_build_then_render() {
    let diagnostics = vec![
        Diagnostic::warning(
            DiagnosticCode::MediaLoadFailure,
            DiagnosticPhase::Render,
            "render warning",
        ),
        Diagnostic::error(DiagnosticCode::UnknownAction, DiagnosticPhase::Build, "build error"),
        Diagnostic::error(DiagnosticCode::SourceLoadFailure, DiagnosticPhase::Parse, "parse error"),
    ];

    assert_eq!(primary_diagnostic_phase(&diagnostics), Some(DiagnosticPhase::Parse));
}

#[test]
fn diagnostics_banner_message_shows_first_parse_error() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::ParseError,
        DiagnosticPhase::Parse,
        "line 3, col 15: expected expression, found ','",
    )];

    assert_eq!(
        diagnostics_banner_message(&diagnostics, super::i18n::Language::English),
        Some("line 3, col 15: expected expression, found ','".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_build_error_message() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::UnknownAction,
        DiagnosticPhase::Build,
        "Unknown callable 'fade_ind'",
    )];

    assert_eq!(
        diagnostics_banner_message(&diagnostics, super::i18n::Language::English),
        Some("Unknown callable 'fade_ind'".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_warning_when_no_errors() {
    let diagnostics = vec![Diagnostic::warning(
        DiagnosticCode::InvalidConfigValue,
        DiagnosticPhase::Parse,
        "parse warning",
    )];

    assert_eq!(
        diagnostics_banner_message(&diagnostics, super::i18n::Language::English),
        Some("parse warning".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_first_error_over_warnings() {
    let diagnostics = vec![
        Diagnostic::warning(
            DiagnosticCode::InvalidConfigValue,
            DiagnosticPhase::Parse,
            "parse warning",
        ),
        Diagnostic::error(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            "Unknown callable 'fade_ind'",
        ),
    ];

    assert_eq!(
        diagnostics_banner_message(&diagnostics, super::i18n::Language::English),
        Some("Unknown callable 'fade_ind'".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_render_error_message() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::RenderFailure,
        DiagnosticPhase::Render,
        "render failed",
    )];

    assert_eq!(
        diagnostics_banner_message(&diagnostics, super::i18n::Language::English),
        Some("render failed".to_string())
    );
}

#[test]
fn clear_render_error_removes_render_failure_state() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.set_render_error("preview failed".to_string());

    shell.clear_render_error("Live preview restored".to_string());

    assert!(shell.document_store.history.render_diagnostics.is_empty());
    assert!(shell.preview_store.preview.error.is_none());
    assert_eq!(shell.preview_store.preview.status.canonical(), "Live preview restored");
}

#[test]
fn clear_render_error_preserves_non_render_preview_failures() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.set_status("Open failed • missing.amx".to_string(), Some("missing file".to_string()));

    shell.clear_render_error("Live preview restored".to_string());

    assert!(shell.document_store.history.render_diagnostics.is_empty());
    assert_eq!(shell.preview_store.preview.error.as_deref(), Some("missing file"));
    assert_eq!(shell.preview_store.preview.status.canonical(), "Open failed • missing.amx");
}

/// A 5s preview at `time_s` with the standard 1920×1080 stage.
fn preview_at(time_s: f64) -> super::PreviewPaneState {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = time_s;
    preview
}

#[test]
fn clamp_time_keeps_time_within_bounds() {
    let cases = [
        (0.0, 0.0),  // zero boundary
        (5.0, 5.0),  // max boundary
        (3.5, 3.5),  // valid time preserved
        (10.0, 5.0), // overshoot clamps to duration
        (-1.0, 0.0), // negative clamps to zero
    ];
    for (time_s, expected) in cases {
        let mut preview = preview_at(time_s);
        preview.playback.clamp_time();
        assert_eq!(preview.playback.current_time_s, expected, "clamp_time({time_s})");
    }

    // A zero duration clamps to the 0.1s minimum.
    let mut preview = super::PreviewPaneState::new(
        0.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = 5.0;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 0.1);
}

#[test]
fn clear_render_error_preserves_newer_non_render_failure_when_render_diagnostic_is_stale() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.set_render_error("preview failed".to_string());
    shell.set_status(
        "Rebuild blocked • parse/load error".to_string(),
        Some("duplicate export".to_string()),
    );

    shell.clear_render_error("Live preview restored".to_string());

    assert!(shell.document_store.history.render_diagnostics.is_empty());
    assert_eq!(shell.preview_store.preview.error.as_deref(), Some("duplicate export"));
    assert_eq!(
        shell.preview_store.preview.status.canonical(),
        "Rebuild blocked • parse/load error"
    );
}

#[test]
fn presentation_mode_clamps_at_pause_point_when_ticking() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.presentation_mode = true;
    preview.playback.pause_points_s = vec![1.0, 2.5];
    preview.playback.is_playing = true;
    preview.playback.current_time_s = 0.8;

    // Tick by 0.4s (overshooting 1.0s to 1.2s)
    preview.playback.tick(std::time::Duration::from_millis(400));

    assert_eq!(preview.playback.current_time_s, 1.0);
    assert!(!preview.playback.is_playing);
    assert!(preview.playback.paused_at_step);
}

#[test]
fn presentation_step_next_and_previous_navigation() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.presentation_mode = true;
    preview.playback.pause_points_s = vec![1.0, 2.5, 4.0];
    preview.playback.current_time_s = 1.0;
    preview.playback.is_playing = false;
    preview.playback.paused_at_step = true;

    // Resumes playback towards next pause point
    preview.playback.presentation_step_next();
    assert!(preview.playback.is_playing);
    assert!(!preview.playback.paused_at_step);

    // If step_next is pressed while playing, jumps to next pause point and pauses
    preview.playback.current_time_s = 1.8;
    preview.playback.presentation_step_next();
    assert_eq!(preview.playback.current_time_s, 2.5);
    assert!(!preview.playback.is_playing);
    assert!(preview.playback.paused_at_step);

    // Step previous jumps back to 1.0s
    preview.playback.presentation_step_prev();
    assert_eq!(preview.playback.current_time_s, 1.0);
    assert!(!preview.playback.is_playing);

    // Step previous from 1.0s jumps back to 0.0s
    preview.playback.presentation_step_prev();
    assert_eq!(preview.playback.current_time_s, 0.0);
}

#[test]
fn presentation_mode_shortcuts_registered() {
    use crate::app::interaction::keyboard::{KeyboardAction, ShortcutRegistry};
    let registry = ShortcutRegistry::new();
    assert!(registry.shortcut_for(&KeyboardAction::TogglePresentationMode).is_some());
    assert!(registry.shortcut_for(&KeyboardAction::StepNext).is_some());
    assert!(registry.shortcut_for(&KeyboardAction::StepPrevious).is_some());
}

#[test]
fn rebuild_command_submits_to_background_worker() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.preview_store.pending_rebuild_at = Some(std::time::Instant::now());

    let effects = shell.handle_command(crate::app::commands::Command::Rebuild);

    assert!(effects.is_empty());
    assert!(
        shell.preview_store.pending_rebuild_at.is_none(),
        "manual rebuild must consume the debounce timer"
    );
    assert!(
        shell.preview_store.in_flight_rebuild.is_some(),
        "manual rebuild must be submitted to the worker"
    );
    assert!(shell.preview_store.rebuild_in_progress);
    assert_eq!(shell.preview_store.preview.status.canonical(), "Building timeline…");
}

#[test]
fn frame_navigation_lands_on_the_export_grid_in_both_directions() {
    let mut preview = super::PreviewPaneState::new(
        10.0,
        SceneDimensions {
            width: 64,
            height: 64,
        },
    );
    let playback = &mut preview.playback;
    for _ in 0..201 {
        playback.frame_step_forward(25.0);
    }
    assert_eq!(playback.current_time_s(), 8.04);
    playback.frame_step_backward(25.0);
    assert_eq!(playback.current_time_s(), 8.0);
    playback.scrub_to(1.88_f64.next_down());
    playback.frame_step_forward(25.0);
    assert_eq!(playback.current_time_s(), 1.88);
    playback.scrub_to(1.88_f64.next_up());
    playback.frame_step_backward(25.0);
    assert_eq!(playback.current_time_s(), 1.88);
}
