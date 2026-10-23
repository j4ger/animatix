use super::*;

#[test]
fn settings_persistence_roundtrips_plugin_paths() {
    let settings = SettingsPersistence {
        rebuild_debounce_ms: 150,
        scrub_step_s: 0.05,
        nudge_step_px: 1.0,
        nudge_step_shift_px: 8.0,
        rotation_snap_degrees: 15.0,
        snap_fps: 60.0,
        keyframe_merge_window_s: 0.05,
        undo_limit: 100,
        grid_size: 40.0,
        app_theme: "dark".to_string(),
        reduce_motion: false,
        density: "default".to_string(),
        theme_dir: None,
        theme_name: None,
        shortcuts: BTreeMap::new(),
        plugin_paths: vec![PathBuf::from("/tmp/plugins")],
    };
    let serialized = ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default())
        .expect("serialize settings");
    let parsed: SettingsPersistence = ron::from_str(&serialized).expect("parse settings roundtrip");
    assert_eq!(parsed.plugin_paths, settings.plugin_paths);
}

#[test]
fn app_state_roundtrips_autosave_prefs() {
    let state = AppState {
        recent_file: Some(PathBuf::from("/tmp/scene.amx")),
        recent_files: vec![PathBuf::from("/tmp/scene.amx")],
        autosave: AutosavePrefs {
            enabled: false,
            interval_s: 45.0,
        },
    };
    let serialized = ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()).unwrap();
    let parsed: AppState = ron::from_str(&serialized).unwrap();

    assert!(!parsed.autosave.enabled);
    assert_eq!(parsed.autosave.interval_s, 45.0);
    assert_eq!(parsed.recent_file, state.recent_file);
}

#[test]
fn app_state_without_autosave_field_uses_defaults() {
    // Shape written before autosave existed: no `autosave` key at all.
    let legacy = "(recent_file: Some(\"/tmp/scene.amx\"), recent_files: [\"/tmp/scene.amx\"])";
    let parsed: AppState = ron::from_str(legacy).expect("legacy app state must still load");

    assert!(parsed.autosave.enabled, "autosave defaults on");
    assert_eq!(
        parsed.autosave.interval_s,
        crate::app::stores::ui_store::DEFAULT_AUTOSAVE_INTERVAL_S
    );
}

#[test]
fn autosave_prefs_defaults_are_stable() {
    let prefs = AutosavePrefs::default();
    assert!(prefs.enabled);
    assert_eq!(prefs.interval_s, 20.0);
}

#[test]
fn detail_region_toggles_without_rebuilding() {
    let mut tree = default_tree();
    assert!(tree.tiles.find_pane(&WorkspaceTab::Inspector).is_some());
    assert!(tree.tiles.find_pane(&WorkspaceTab::Code).is_some());
    assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Inspector));

    // Hiding the detail region keeps every pane in the tree.
    assert!(set_detail_visible(&mut tree, false));
    let inspector = tree.tiles.find_pane(&WorkspaceTab::Inspector).unwrap();
    let detail = tree.tiles.parent_of(inspector).unwrap();
    assert!(!tree.is_visible(detail));

    // Activating a tab shows the region again and switches the tab.
    assert!(activate_detail_tab(&mut tree, WorkspaceTab::Code));
    assert!(tree.is_visible(detail));
    assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Code));

    // Other panes survive untouched.
    assert!(tree.tiles.find_pane(&WorkspaceTab::Timeline).is_some());
    assert!(tree.tiles.find_pane(&WorkspaceTab::Preview).is_some());
    assert!(tree.tiles.find_pane(&WorkspaceTab::Sidebar).is_some());
}

// ── Compact (narrow-window) downgrade ─────────────────────────────────

#[test]
fn compact_decision_uses_the_breakpoint() {
    assert!(compact_for_width(COMPACT_BREAKPOINT - 1.0));
    assert!(!compact_for_width(COMPACT_BREAKPOINT));
    assert!(!compact_for_width(COMPACT_BREAKPOINT + 1.0));
    // A degenerate (unmeasured) width must not flip into compact mode.
    assert!(!compact_for_width(0.0));
    assert!(!compact_for_width(-100.0));
}

#[test]
fn compact_entry_hides_detail_and_restores_on_exit() {
    let mut tree = default_tree();
    let mut restore = None;
    assert!(detail_visible(&tree), "detail starts docked");
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar));

    let entered = reconcile_compact(&mut tree, false, true, &mut restore);
    assert_eq!(entered, CompactTransition::Entered);
    assert!(!detail_visible(&tree), "detail dock hidden while compact");
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar), "rail stays visible");
    assert!(restore.is_some(), "entry captures state to restore");
    // The dock is hidden, but the tab is still selectable (drawer reads it).
    assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Inspector));

    let exited = reconcile_compact(&mut tree, true, false, &mut restore);
    assert_eq!(exited, CompactTransition::Exited);
    assert!(detail_visible(&tree), "widening restores the detail dock");
    assert!(restore.is_none(), "restore state is consumed");

    // A no-op transition leaves the tree alone.
    assert_eq!(
        reconcile_compact(&mut tree, false, false, &mut restore),
        CompactTransition::Unchanged
    );
}

#[test]
fn compact_exit_restores_a_user_hidden_sidebar() {
    // The user collapsed the sidebar (or was in Focus mode); compact mode
    // must still show the rail, but widening must return to *their* state,
    // not force panes back open.
    let mut tree = default_tree();
    set_pane_visible(&mut tree, WorkspaceTab::Sidebar, false);
    let mut restore = None;

    reconcile_compact(&mut tree, false, true, &mut restore);
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar), "rail is reachable while compact");

    reconcile_compact(&mut tree, true, false, &mut restore);
    assert!(!pane_visible(&tree, WorkspaceTab::Sidebar), "user's hidden sidebar is restored");
}

#[test]
fn apply_compact_entry_reshapes_a_rebuilt_tree() {
    // A preset/reset while compact rebuilds the tree with the detail
    // visible; the migration path reshapes it without a transition.
    let mut tree = default_tree();
    assert!(detail_visible(&tree));
    apply_compact_entry(&mut tree);
    assert!(!detail_visible(&tree));
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar));
    // Idempotent.
    apply_compact_entry(&mut tree);
    assert!(!detail_visible(&tree));
}

#[test]
fn enforce_compact_rail_gives_the_sidebar_a_fixed_width() {
    let mut tree = default_tree();
    let width = 900.0;
    enforce_compact_layout(&mut tree, LayoutPreset::Animate, width, 700.0);

    let sidebar = tree.tiles.find_pane(&WorkspaceTab::Sidebar).unwrap();
    let preview = tree.tiles.find_pane(&WorkspaceTab::Preview).unwrap();
    let top_row = tree.tiles.parent_of(sidebar).unwrap();
    let egui_tiles::Tile::Container(Container::Linear(linear)) = tree.tiles.get(top_row).unwrap()
    else {
        panic!("expected a linear top row");
    };
    let px = linear.shares.split(&[sidebar, preview], width);
    assert!(
        (px[0] - crate::app::panels::COMPACT_RAIL_WIDTH).abs() < 0.01,
        "sidebar is the icon rail width, got {}",
        px[0]
    );
    assert!((px[1] - (width - crate::app::panels::COMPACT_RAIL_WIDTH)).abs() < 0.01);
}

#[test]
fn set_active_detail_tab_does_not_show_the_region() {
    let mut tree = default_tree();
    set_detail_visible(&mut tree, false);
    assert!(set_active_detail_tab(&mut tree, WorkspaceTab::Code));
    assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Code));
    assert!(!detail_visible(&tree), "selecting a tab must not unhide the dock");
    // A pane outside the detail group is rejected.
    assert!(!set_active_detail_tab(&mut tree, WorkspaceTab::Preview));
}
