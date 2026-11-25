use super::*;

#[test]
fn settings_persistence_roundtrips_plugin_paths() {
    let settings = SettingsPersistence {
        interface_language: "zh-CN".into(),
        rebuild_debounce_ms: 150,
        scrub_step_s: 0.05,
        snap_fps: 60.0,
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
    assert_eq!(parsed.interface_language, "zh-CN");
    let before_language = serialized
        .lines()
        .filter(|line| !line.contains("interface_language:"))
        .collect::<Vec<_>>()
        .join("\n");
    let old_settings: SettingsPersistence =
        ron::from_str(&before_language).expect("load settings without language");
    assert!(old_settings.interface_language.is_empty());

    let legacy = serialized.replacen(
        '(',
        "(nudge_step_px: 1.0, nudge_step_shift_px: 8.0, rotation_snap_degrees: 15.0, keyframe_merge_window_s: 0.05,",
        1,
    );
    let migrated: SettingsPersistence =
        ron::from_str(&legacy).expect("load legacy writer settings");
    assert_eq!(migrated.plugin_paths, settings.plugin_paths);
    assert_eq!(migrated.snap_fps, settings.snap_fps);
    assert_eq!(migrated.undo_limit, settings.undo_limit);
    assert_eq!(migrated.app_theme, settings.app_theme);
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
    show_inspector(&mut tree);
    assert!(detail_visible(&tree), "pinned inspector starts docked");
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar));

    let entered = reconcile_compact(&mut tree, false, true, &mut restore);
    assert_eq!(entered, CompactTransition::Entered);
    assert!(!detail_visible(&tree), "detail dock hidden while compact");
    assert!(pane_visible(&tree, WorkspaceTab::Sidebar), "rail stays visible");
    assert!(restore.is_some(), "entry captures state to restore");

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
    show_inspector(&mut tree);
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

fn legacy_code_tree() -> Tree<WorkspaceTab> {
    let mut tiles = Tiles::default();
    let editor = tiles.insert_pane(WorkspaceTab::Code);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);
    let curves = tiles.insert_pane(WorkspaceTab::Curves);
    let detail = tiles.insert_tab_tile(vec![editor, inspector]);
    let bottom = tiles.insert_tab_tile(vec![timeline, curves]);
    let root = tiles.insert_container(Linear::new(LinearDir::Vertical, vec![detail, bottom]));
    let mut tree = Tree::new(egui::Id::unique("legacy-workspace"), root, tiles);
    tree.make_active(|id, _| id == editor);
    tree
}

#[test]
fn legacy_code_tab_migrates_to_write_without_the_curves_tab() {
    let mut saved = WorkspacePersistence {
        tree: legacy_code_tree(),
        layout_version: 0,
        layout_preset: Some(LayoutPreset::Animate),
        window_size: Some([1120.0, 820.0]),
        window_maximized: Some(true),
        settings: None,
    };
    let legacy = ron::to_string(&saved).unwrap().replace("Some(Animate)", "Some(Focus)");
    saved = ron::from_str(&legacy).expect("old named preset should not drop the RON file");
    migrate_workspace(&mut saved);
    assert_eq!(saved.layout_preset, Some(LayoutPreset::Code));
    assert_eq!(saved.window_size, Some([1120.0, 820.0]));
    assert_eq!(saved.window_maximized, Some(true));
    assert!(pane_visible(&saved.tree, WorkspaceTab::Code));
    assert!(!detail_visible(&saved.tree));
    assert!(saved.tree.tiles.find_pane(&WorkspaceTab::Curves).is_none());
    assert!(pane_visible(&saved.tree, WorkspaceTab::Timeline));
}

#[test]
fn legacy_write_layout_preserves_chinese_grid_and_shortcut_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace_layout.ron");
    let tree = ron::to_string(&legacy_code_tree()).unwrap();
    let input = format!(
        r#"(tree: {tree}, window_size: Some((1120.0, 820.0)), settings: Some((
            interface_language: "zh-CN", rebuild_debounce_ms: 150, scrub_step_s: 0.05,
            snap_fps: 60.0, undo_limit: 100, grid_size: 48.0,
            shortcuts: {{"Save": (command: true, ctrl: false, shift: true, alt: false, key: "S")}},
        )))"#
    );
    ron::from_str::<WorkspacePersistence>(&input).expect("decode the legacy RON fixture");
    fs::write(&path, input).unwrap();

    let first = load_workspace_persistence(&path).expect("load a pre-versioned dock");
    fs::write(&path, ron::to_string(&first).unwrap()).unwrap();
    let reopened = load_workspace_persistence(&path).expect("reload the migrated dock");

    for saved in [first, reopened] {
        assert_eq!(saved.layout_version, WORKSPACE_LAYOUT_VERSION);
        assert_eq!(saved.layout_preset, Some(LayoutPreset::Code));
        assert!(pane_visible(&saved.tree, WorkspaceTab::Code));
        assert!(!detail_visible(&saved.tree));
        assert_eq!(saved.window_size, Some([1120.0, 820.0]));
        let settings = saved.settings.expect("preferences are preserved");
        assert_eq!(settings.interface_language, "zh-CN");
        assert_eq!(settings.grid_size, 48.0);
        assert_eq!(
            settings.shortcuts.get("Save"),
            Some(&SavedShortcut {
                command: true,
                ctrl: false,
                shift: true,
                alt: false,
                key: "S".into(),
            }),
        );
    }
}

#[test]
fn modern_saved_workspace_keeps_pin_and_editor_but_drops_optional_comparison() {
    let mut tree = build_tree_for(LayoutPreset::Code, 1440.0, 960.0);
    show_inspector(&mut tree);
    toggle_curve_comparison(&mut tree);
    let mut saved = WorkspacePersistence {
        tree,
        layout_version: WORKSPACE_LAYOUT_VERSION,
        layout_preset: Some(LayoutPreset::Code),
        window_size: None,
        window_maximized: None,
        settings: None,
    };
    migrate_workspace(&mut saved);
    assert_eq!(saved.layout_preset, Some(LayoutPreset::Code));
    assert!(detail_visible(&saved.tree));
    assert!(pane_visible(&saved.tree, WorkspaceTab::Code));
    assert!(saved.tree.tiles.find_pane(&WorkspaceTab::Curves).is_none());
}
