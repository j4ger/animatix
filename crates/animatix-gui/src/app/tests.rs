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
fn default_workspace_has_detail_tabs_visible() {
    let tree = default_tree();
    let tabs: Vec<_> = tree
        .tiles
        .iter()
        .filter_map(|(_, tile)| match tile {
            egui_tiles::Tile::Pane(tab) => Some(*tab),
            _ => None,
        })
        .collect();
    // Five panes: the Inspector and Code share one detail tab group, visible
    // by default with the Inspector active.
    assert_eq!(tabs.len(), 5);
    assert!(tabs.contains(&WorkspaceTab::Sidebar));
    assert!(tabs.contains(&WorkspaceTab::Preview));
    assert!(tabs.contains(&WorkspaceTab::Inspector));
    assert!(tabs.contains(&WorkspaceTab::Code));
    assert!(tabs.contains(&WorkspaceTab::Timeline));

    assert_eq!(
        super::persistence::active_detail_tab(&tree),
        Some(WorkspaceTab::Inspector),
        "Inspector is the default detail tab"
    );
    let inspector = tree.tiles.find_pane(&WorkspaceTab::Inspector).unwrap();
    assert!(tree.is_visible(inspector), "detail region is visible by default");
}

#[test]
fn layout_presets_switch_detail_tab_and_focus_hides_regions() {
    let mut tree = default_tree();

    assert!(super::persistence::apply_layout_preset(
        &mut tree,
        LayoutPreset::Code,
        1440.0,
        960.0
    ));
    assert_eq!(super::persistence::active_detail_tab(&tree), Some(WorkspaceTab::Code));

    assert!(super::persistence::apply_layout_preset(
        &mut tree,
        LayoutPreset::Focus,
        1440.0,
        960.0
    ));
    let sidebar = tree.tiles.find_pane(&WorkspaceTab::Sidebar).unwrap();
    let timeline = tree.tiles.find_pane(&WorkspaceTab::Timeline).unwrap();
    assert!(!tree.is_visible(sidebar), "Focus hides the sidebar");
    assert!(!tree.is_visible(timeline), "Focus hides the timeline");

    assert!(super::persistence::apply_layout_preset(
        &mut tree,
        LayoutPreset::Animate,
        1440.0,
        960.0
    ));
    assert!(tree.is_visible(sidebar));
    assert!(tree.is_visible(timeline));
    assert_eq!(super::persistence::active_detail_tab(&tree), Some(WorkspaceTab::Inspector));
}

#[test]
fn preview_fit_preserves_aspect_ratio() {
    let fitted = fit_preview(
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
        Vec2::new(400.0, 400.0),
    );
    assert!((fitted.x / fitted.y - 16.0 / 9.0).abs() < 0.001);
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
        diagnostics_banner_message(&diagnostics),
        Some("line 3, col 15: expected expression, found ','".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_build_error_message() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::UnknownAction,
        DiagnosticPhase::Build,
        "Unknown action 'fade-ind'",
    )];

    assert_eq!(
        diagnostics_banner_message(&diagnostics),
        Some("Unknown action 'fade-ind'".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_warning_when_no_errors() {
    let diagnostics = vec![Diagnostic::warning(
        DiagnosticCode::InvalidConfigValue,
        DiagnosticPhase::Parse,
        "parse warning",
    )];

    assert_eq!(diagnostics_banner_message(&diagnostics), Some("parse warning".to_string()));
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
            "Unknown action 'fade-ind'",
        ),
    ];

    assert_eq!(
        diagnostics_banner_message(&diagnostics),
        Some("Unknown action 'fade-ind'".to_string())
    );
}

#[test]
fn diagnostics_banner_message_shows_render_error_message() {
    let diagnostics = vec![Diagnostic::error(
        DiagnosticCode::RenderFailure,
        DiagnosticPhase::Render,
        "render failed",
    )];

    assert_eq!(diagnostics_banner_message(&diagnostics), Some("render failed".to_string()));
}

#[test]
fn clear_render_error_removes_render_failure_state() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.set_render_error("preview failed".to_string());

    shell.clear_render_error("Live preview restored".to_string());

    assert!(shell.document_store.history.render_diagnostics.is_empty());
    assert!(shell.preview_store.preview.error.is_none());
    assert_eq!(shell.preview_store.preview.status, "Live preview restored");
}

#[test]
fn clear_render_error_preserves_non_render_preview_failures() {
    let mut shell = GuiShell::load(PathBuf::from("test_dummy.amx"), false);
    shell.set_status("Open failed • missing.amx".to_string(), Some("missing file".to_string()));

    shell.clear_render_error("Live preview restored".to_string());

    assert!(shell.document_store.history.render_diagnostics.is_empty());
    assert_eq!(shell.preview_store.preview.error.as_deref(), Some("missing file"));
    assert_eq!(shell.preview_store.preview.status, "Open failed • missing.amx");
}

#[test]
fn clamp_time_clamps_negative_to_zero() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = -1.0;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 0.0);
}

#[test]
fn clamp_time_clamps_over_duration_to_max() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = 10.0;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 5.0);
}

#[test]
fn clamp_time_uses_minimum_duration_of_point_one() {
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
fn clamp_time_preserves_valid_time() {
    let mut preview = super::PreviewPaneState::new(
        10.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = 3.5;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 3.5);
}

#[test]
fn clamp_time_at_boundary_zero() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = 0.0;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 0.0);
}

#[test]
fn clamp_time_at_boundary_max() {
    let mut preview = super::PreviewPaneState::new(
        5.0,
        SceneDimensions {
            width: 1920,
            height: 1080,
        },
    );
    preview.playback.current_time_s = 5.0;
    preview.playback.clamp_time();
    assert_eq!(preview.playback.current_time_s, 5.0);
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
    assert_eq!(shell.preview_store.preview.status, "Rebuild blocked • parse/load error");
}
