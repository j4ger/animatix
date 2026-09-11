use std::collections::BTreeMap;
use std::path::PathBuf;

use egui_tiles::{Container, Linear, LinearDir, Tile, Tiles, Tree};

use super::*;
use crate::app::interaction::keyboard::SavedShortcut;

/// Region proportions for the responsive default layout.
///
/// Every region is allocated as `clamp(ratio * available, min, max)`: a
/// proportion by default, with pixel bounds as the floor/ceiling so extreme
/// window sizes never produce unusable panes. `Behavior::min_size` is the final
/// per-tile floor. Shares are relative, so the proportions hold as the window
/// is resized.
mod metrics {
    pub const LEFT_RATIO: f32 = 0.16;
    pub const LEFT_MIN: f32 = 200.0;
    pub const LEFT_MAX: f32 = 360.0;

    pub const DETAIL_INSPECTOR_RATIO: f32 = 0.21;
    pub const DETAIL_INSPECTOR_MIN: f32 = 260.0;
    pub const DETAIL_INSPECTOR_MAX: f32 = 420.0;

    pub const DETAIL_CODE_RATIO: f32 = 0.38;
    pub const DETAIL_CODE_MIN: f32 = 420.0;
    pub const DETAIL_CODE_MAX: f32 = 720.0;

    pub const PREVIEW_MIN: f32 = 360.0;

    pub const BOTTOM_RATIO: f32 = 0.25;
    pub const BOTTOM_MIN: f32 = 180.0;
    pub const BOTTOM_MAX: f32 = 420.0;
}

/// Vertical chrome outside the dock area: toolbar + status bar.
const VERTICAL_CHROME: f32 = 28.0 + 22.0;

/// Narrowest the sidebar may be squeezed before the preview starts giving ground.
const RAIL_MIN: f32 = 48.0;

fn clamp_ratio(ratio: f32, available: f32, min: f32, max: f32) -> f32 {
    (ratio * available).clamp(min, max)
}

/// Per-preset region preferences (ratios, pixel bounds, detail tab).
struct PresetRegions {
    left: (f32, f32, f32),
    detail: (f32, f32, f32),
    bottom: (f32, f32, f32),
    tab: WorkspaceTab,
    hide_surroundings: bool,
}

fn preset_regions(preset: LayoutPreset) -> PresetRegions {
    use metrics::*;
    match preset {
        LayoutPreset::Animate => PresetRegions {
            left: (LEFT_RATIO, LEFT_MIN, LEFT_MAX),
            detail: (DETAIL_INSPECTOR_RATIO, DETAIL_INSPECTOR_MIN, DETAIL_INSPECTOR_MAX),
            bottom: (BOTTOM_RATIO, BOTTOM_MIN, BOTTOM_MAX),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: false,
        },
        LayoutPreset::Code => PresetRegions {
            left: (0.14, 180.0, 300.0),
            detail: (DETAIL_CODE_RATIO, DETAIL_CODE_MIN, DETAIL_CODE_MAX),
            bottom: (0.18, 160.0, 260.0),
            tab: WorkspaceTab::Code,
            hide_surroundings: false,
        },
        LayoutPreset::Inspect => PresetRegions {
            left: (0.12, 160.0, 260.0),
            detail: (0.30, 300.0, 460.0),
            bottom: (0.30, 220.0, 420.0),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: false,
        },
        LayoutPreset::Focus => PresetRegions {
            left: (LEFT_RATIO, LEFT_MIN, LEFT_MAX),
            detail: (DETAIL_INSPECTOR_RATIO, DETAIL_INSPECTOR_MIN, DETAIL_INSPECTOR_MAX),
            bottom: (BOTTOM_RATIO, BOTTOM_MIN, BOTTOM_MAX),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: true,
        },
    }
}

/// Build a workspace tree for a preset at a reference window size.
///
/// Layout:
/// ```text
/// ┌───────────┬───────────────────────────┬─────────────┐
/// │  Sidebar  │          Preview          │   Detail    │
/// │           │                           │ ┌─────────┐ │
/// │           │                           │ │Inspector│ │
/// │           │                           │ │  Code   │ │
/// │           │                           │ └─────────┘ │
/// ├───────────┴───────────────────────────┴─────────────┤
/// │                  Timeline (full width)               │
/// └──────────────────────────────────────────────────────┘
/// ```
///
/// The right "detail" region is a tab group holding the Inspector and the code
/// editor, so only one of them is ever on screen. The Timeline spans the full
/// width at the bottom.
pub(super) fn build_tree_for(preset: LayoutPreset, width: f32, height: f32) -> Tree<WorkspaceTab> {
    let regions = preset_regions(preset);
    let mut tiles = Tiles::default();

    let sidebar = tiles.insert_pane(WorkspaceTab::Sidebar);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
    let code = tiles.insert_pane(WorkspaceTab::Code);
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);

    // Inspector and Code share one tab group: mutually exclusive detail views.
    let detail = tiles.insert_tab_tile(vec![inspector, code]);

    let left_px = clamp_ratio(regions.left.0, width, regions.left.1, regions.left.2);
    let detail_px = clamp_ratio(regions.detail.0, width, regions.detail.1, regions.detail.2);
    let preview_px = (width - left_px - detail_px).max(metrics::PREVIEW_MIN);

    let mut top_row = Linear::new(LinearDir::Horizontal, vec![sidebar, preview, detail]);
    top_row.shares[sidebar] = left_px;
    top_row.shares[preview] = preview_px;
    top_row.shares[detail] = detail_px;
    let top_row = tiles.insert_container(top_row);

    let dock_h = (height - VERTICAL_CHROME).max(1.0);
    let bottom_px = clamp_ratio(regions.bottom.0, dock_h, regions.bottom.1, regions.bottom.2);
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [top_row, timeline],
        (1.0 - bottom_px / dock_h).clamp(0.1, 0.9),
    ));

    let mut tree = Tree::new("workspace", root, tiles);
    activate_detail_tab(&mut tree, regions.tab);
    if regions.hide_surroundings {
        set_pane_visible(&mut tree, WorkspaceTab::Sidebar, false);
        set_pane_visible(&mut tree, WorkspaceTab::Timeline, false);
        set_detail_visible(&mut tree, false);
    }
    tree
}

/// Default workspace layout: Animate, sized to the initial window.
pub(super) fn default_tree() -> Tree<WorkspaceTab> {
    build_tree_for(
        LayoutPreset::Animate,
        INITIAL_WINDOW_SIZE.0 as f32,
        INITIAL_WINDOW_SIZE.1 as f32,
    )
}

/// Apply a preset's proportions and detail tab to an existing tree.
///
/// Returns `false` when the tree lacks the expected panes (e.g. a layout
/// persisted before the detail region existed); callers rebuild in that case.
pub(super) fn apply_layout_preset(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) -> bool {
    let Some(sidebar) = tree.tiles.find_pane(&WorkspaceTab::Sidebar) else {
        return false;
    };
    let Some(preview) = tree.tiles.find_pane(&WorkspaceTab::Preview) else {
        return false;
    };
    let Some(inspector) = tree.tiles.find_pane(&WorkspaceTab::Inspector) else {
        return false;
    };
    let Some(timeline) = tree.tiles.find_pane(&WorkspaceTab::Timeline) else {
        return false;
    };
    let Some(detail) = tree.tiles.parent_of(inspector) else {
        return false;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return false;
    };

    let regions = preset_regions(preset);
    let left_px = clamp_ratio(regions.left.0, width, regions.left.1, regions.left.2);
    let detail_px = clamp_ratio(regions.detail.0, width, regions.detail.1, regions.detail.2);
    let preview_px = (width - left_px - detail_px).max(metrics::PREVIEW_MIN);
    if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(top_row) {
        linear.shares.set_share(sidebar, left_px);
        linear.shares.set_share(preview, preview_px);
        linear.shares.set_share(detail, detail_px);
    }

    let dock_h = (height - VERTICAL_CHROME).max(1.0);
    let bottom_px = clamp_ratio(regions.bottom.0, dock_h, regions.bottom.1, regions.bottom.2);
    let top_frac = (1.0 - bottom_px / dock_h).clamp(0.1, 0.9);
    if let Some(root) = tree.root {
        if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) {
            linear.shares.set_share(top_row, top_frac);
            linear.shares.set_share(timeline, 1.0 - top_frac);
        }
    }

    let visible = !regions.hide_surroundings;
    tree.set_visible(sidebar, visible);
    tree.set_visible(timeline, visible);
    set_pane_visible(tree, WorkspaceTab::Inspector, true);
    set_pane_visible(tree, WorkspaceTab::Code, true);
    set_detail_visible(tree, visible);
    if visible {
        activate_detail_tab(tree, regions.tab);
    }
    true
}

/// Enforce the preset's pixel bounds on the docked regions each frame.
///
/// Shares are relative, so a window smaller than the reference size used at
/// build time would scale every region down proportionally and drop them below
/// their pixel floors. This pass reads the current widths, clamps them into
/// `[min, max]`, and lets the preview absorb the difference — "proportion by
/// default, pixels as the floor/ceiling" applied continuously. Regions already
/// inside their bounds are left untouched, so user resizing is preserved.
pub(super) fn enforce_layout_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let regions = preset_regions(preset);

    let (Some(sidebar), Some(preview), Some(inspector), Some(timeline)) = (
        tree.tiles.find_pane(&WorkspaceTab::Sidebar),
        tree.tiles.find_pane(&WorkspaceTab::Preview),
        tree.tiles.find_pane(&WorkspaceTab::Inspector),
        tree.tiles.find_pane(&WorkspaceTab::Timeline),
    ) else {
        return;
    };
    let Some(detail) = tree.tiles.parent_of(inspector) else {
        return;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return;
    };

    // Horizontal: sidebar | preview (| detail).
    if tree.is_visible(sidebar) && tree.is_visible(preview) {
        let detail_visible = tree.is_visible(detail);
        let mut kids = vec![sidebar, preview];
        if detail_visible {
            kids.push(detail);
        }
        if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(top_row) {
            let px = linear.shares.split(&kids, width);
            let mut left = px[0].clamp(regions.left.1, regions.left.2);
            let mut detail_px = if detail_visible {
                px[2].clamp(regions.detail.1, regions.detail.2)
            } else {
                0.0
            };
            let mut preview_px = (width - left - detail_px).max(0.0);
            // Reclaim in reverse priority when the window is too small: the
            // detail column yields first, then the sidebar shrinks toward an
            // icon-rail width, and only then does the preview give ground. This
            // keeps every region present instead of collapsing one to nothing.
            if preview_px < metrics::PREVIEW_MIN {
                let need = metrics::PREVIEW_MIN - preview_px;
                let take = detail_px.min(need);
                detail_px -= take;
                preview_px += take;
            }
            if preview_px < metrics::PREVIEW_MIN {
                let need = metrics::PREVIEW_MIN - preview_px;
                let take = (left - RAIL_MIN).max(0.0).min(need);
                left -= take;
                preview_px += take;
            }
            linear.shares.set_share(sidebar, left);
            linear.shares.set_share(preview, preview_px);
            if detail_visible {
                linear.shares.set_share(detail, detail_px);
            }
        }
    }

    // Vertical: top row above, timeline below.
    if tree.is_visible(timeline) {
        if let Some(root) = tree.root {
            let kids = vec![top_row, timeline];
            if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) {
                let px = linear.shares.split(&kids, height);
                let bottom = px[1].clamp(regions.bottom.1, regions.bottom.2);
                linear.shares.set_share(top_row, (height - bottom).max(0.0));
                linear.shares.set_share(timeline, bottom);
            }
        }
    }
}

/// Show or hide the whole detail region (Inspector + Code tab group).
pub(super) fn set_detail_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    let Some(inspector) = tree.tiles.find_pane(&WorkspaceTab::Inspector) else {
        return false;
    };
    let Some(detail) = tree.tiles.parent_of(inspector) else {
        return false;
    };
    tree.set_visible(detail, visible);
    true
}

/// Activate a tab inside the detail region, making the region visible.
pub(super) fn activate_detail_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    if tree.tiles.find_pane(&tab).is_none() {
        return false;
    }
    set_detail_visible(tree, true);
    tree.make_active(|_id, tile| matches!(tile, Tile::Pane(pane) if *pane == tab))
}

/// Which detail tab is currently active.
pub(super) fn active_detail_tab(tree: &Tree<WorkspaceTab>) -> Option<WorkspaceTab> {
    let inspector = tree.tiles.find_pane(&WorkspaceTab::Inspector)?;
    let detail = tree.tiles.parent_of(inspector)?;
    let tabs = match tree.tiles.get_container(detail)? {
        Container::Tabs(tabs) => tabs,
        _ => return None,
    };
    for tab in [WorkspaceTab::Inspector, WorkspaceTab::Code] {
        if let Some(id) = tree.tiles.find_pane(&tab) {
            if tabs.is_active(id) {
                return Some(tab);
            }
        }
    }
    None
}

fn set_pane_visible(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab, visible: bool) -> bool {
    match tree.tiles.find_pane(&tab) {
        Some(id) => {
            tree.set_visible(id, visible);
            true
        },
        None => false,
    }
}

pub(super) fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

pub(super) fn load_workspace_persistence(path: &Path) -> Option<WorkspacePersistence> {
    let content = fs::read_to_string(path).ok()?;
    ron::from_str::<WorkspacePersistence>(&content).ok()
}

// ── Window geometry / workspace layout persistence ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SettingsPersistence {
    pub rebuild_debounce_ms: u64,
    pub scrub_step_s: f64,
    pub nudge_step_px: f32,
    pub nudge_step_shift_px: f32,
    pub rotation_snap_degrees: f32,
    pub snap_fps: f32,
    pub keyframe_merge_window_s: f64,
    pub undo_limit: usize,
    pub grid_size: f32,
    /// IDE appearance: "auto" | "light" | "dark". Defaults to "auto".
    #[serde(default = "default_app_theme")]
    pub app_theme: String,
    /// True when the user prefers reduced motion. Defaults to false.
    #[serde(default)]
    pub reduce_motion: bool,
    /// Density preference: "default" or "compact". Defaults to "default".
    #[serde(default = "default_density")]
    pub density: String,
    /// Optional directory containing eparts JSON theme files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_dir: Option<PathBuf>,
    /// Selected theme name inside `theme_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_name: Option<String>,
    /// Persisted shortcut overrides keyed by stable binding name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shortcuts: BTreeMap<String, SavedShortcut>,
    /// Explicit plugin manifest/library paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_paths: Vec<PathBuf>,
}

/// Load persisted shortcut overrides from the workspace persistence file.
fn default_density() -> String {
    "default".to_string()
}

fn default_app_theme() -> String {
    "auto".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkspacePersistence {
    pub(crate) tree: Tree<WorkspaceTab>,
    #[serde(default)]
    pub(crate) window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) window_maximized: Option<bool>,
    #[serde(default)]
    pub(crate) settings: Option<SettingsPersistence>,
}

// ── App state persistence (recent file, preferences) ─────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppState {
    #[serde(default)]
    recent_file: Option<PathBuf>,
    #[serde(default)]
    recent_files: Vec<PathBuf>,
}

/// Cap on the recent-files list.
const MAX_RECENT_FILES: usize = 12;

pub(super) fn app_state_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("app_state.ron");
    }
    PathBuf::from(".animatix-app-state.ron")
}

pub(super) fn load_app_state() -> Option<PathBuf> {
    let path = app_state_path();
    let content = fs::read_to_string(&path).ok()?;
    let state: AppState = ron::from_str(&content).ok()?;
    state.recent_file
}

/// Most-recently opened files, newest first.
pub(super) fn load_recent_files() -> Vec<PathBuf> {
    let Some(content) = fs::read_to_string(app_state_path()).ok() else {
        return Vec::new();
    };
    let state: AppState = ron::from_str(&content).unwrap_or_default();
    if state.recent_files.is_empty() {
        // Migrate a pre-list install that only stored a single recent file.
        state.recent_file.into_iter().collect()
    } else {
        state.recent_files
    }
}

pub(super) fn save_app_state(recent_file: &Path) {
    let path = app_state_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create persistence directory {}: {}", parent.display(), e);
        }
    }
    let mut state = fs::read_to_string(&path)
        .ok()
        .and_then(|content| ron::from_str::<AppState>(&content).ok())
        .unwrap_or_default();
    state.recent_file = Some(recent_file.to_path_buf());
    state.recent_files.retain(|existing| existing != recent_file);
    state.recent_files.insert(0, recent_file.to_path_buf());
    state.recent_files.truncate(MAX_RECENT_FILES);

    if let Ok(serialized) = ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()) {
        if let Err(e) = fs::write(&path, serialized) {
            tracing::warn!("Failed to write app state file {}: {}", path.display(), e);
        }
    }
}

pub(super) fn clear_app_state() {
    let path = app_state_path();
    if path.exists() {
        if let Err(e) = fs::remove_file(&path) {
            tracing::warn!("Failed to remove app state file {}: {}", path.display(), e);
        }
    }
}

#[cfg(test)]
mod tests {
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
        let parsed: SettingsPersistence =
            ron::from_str(&serialized).expect("parse settings roundtrip");
        assert_eq!(parsed.plugin_paths, settings.plugin_paths);
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
}
