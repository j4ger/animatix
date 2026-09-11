use std::collections::BTreeMap;
use std::path::PathBuf;

use egui_tiles::{Linear, LinearDir, Tiles, Tree};

use super::*;
use crate::app::interaction::keyboard::SavedShortcut;

/// Build a workspace tree.
///
/// Layout:
/// ```text
/// Without inspector (default):
/// ┌─────────────────────────┬──────────────┐
/// │ Sidebar | Editor (tabs) │   Preview    │
/// │                         │              │
/// ├─────────────────────────┴──────────────┤
/// │          Timeline (full width)         │
/// └────────────────────────────────────────┘
///
/// With inspector:
/// ┌─────────────────────────┬──────────────┬──────────┐
/// │ Sidebar | Editor (tabs) │   Preview    │ Inspector│
/// │                         │              │          │
/// ├─────────────────────────┴──────────────┴──────────┤
/// │          Timeline (full width)                    │
/// └───────────────────────────────────────────────────┘
/// ```
///
/// Sidebar and Editor live as tabs in a single pane on the left.
/// Timeline always spans the full width at the bottom.
/// Inspector is hidden by default and toggled via a toolbar button.
pub(super) fn build_tree(inspector_visible: bool) -> Tree<WorkspaceTab> {
    let mut tiles = Tiles::default();

    let sidebar = tiles.insert_pane(WorkspaceTab::Sidebar);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);

    // The Inspector pane always exists in the tree; toggling it flips
    // visibility rather than rebuilding, so user rearrangement survives.
    // Container layout skips invisible children.
    let mut top_row = Linear::new(LinearDir::Horizontal, vec![sidebar, preview, inspector]);
    top_row.shares[sidebar] = 0.22;
    top_row.shares[preview] = 0.53;
    top_row.shares[inspector] = 0.25;
    let top_row = tiles.insert_container(top_row);

    // Root: top row above, full-width timeline below.
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [top_row, timeline],
        0.65, // top row gets 65 %; timeline gets 35 %
    ));

    let mut tree = Tree::new("workspace", root, tiles);
    tree.set_visible(inspector, inspector_visible);
    tree
}

/// Show or hide the Inspector pane without disturbing the rest of the layout.
///
/// Returns `false` when the tree has no Inspector pane (a layout persisted
/// before the pane existed); callers can fall back to a fresh tree.
pub(super) fn set_inspector_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    match tree.tiles.find_pane(&WorkspaceTab::Inspector) {
        Some(id) => {
            tree.set_visible(id, visible);
            true
        },
        None => false,
    }
}

/// Default workspace layout — inspector hidden.
pub(super) fn default_tree() -> Tree<WorkspaceTab> {
    build_tree(false)
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

#[derive(Debug, Serialize, Deserialize)]
struct AppState {
    recent_file: Option<PathBuf>,
}

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

pub(super) fn save_app_state(recent_file: &Path) {
    let path = app_state_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create persistence directory {}: {}", parent.display(), e);
        }
    }
    let state = AppState {
        recent_file: Some(recent_file.to_path_buf()),
    };
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
    fn inspector_pane_exists_and_toggles_without_rebuilding() {
        let mut tree = build_tree(false);
        let inspector = tree
            .tiles
            .find_pane(&WorkspaceTab::Inspector)
            .expect("inspector pane is always present in the tree");
        assert!(!tree.is_visible(inspector), "hidden by default");

        assert!(set_inspector_visible(&mut tree, true));
        assert!(tree.is_visible(inspector));
        assert!(set_inspector_visible(&mut tree, false));
        assert!(!tree.is_visible(inspector));

        // Other panes must survive the toggle untouched.
        assert!(tree.tiles.find_pane(&WorkspaceTab::Timeline).is_some());
        assert!(tree.tiles.find_pane(&WorkspaceTab::Preview).is_some());
        assert!(tree.tiles.find_pane(&WorkspaceTab::Sidebar).is_some());
    }
}
