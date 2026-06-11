use super::*;
use egui_tiles::{Linear, LinearDir, Tiles, Tree};

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
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);

    // Top row: sidebar | preview (| inspector if visible).
    let (top_children, inspector_id) = if inspector_visible {
        let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
        (vec![sidebar, preview, inspector], Some(inspector))
    } else {
        (vec![sidebar, preview], None)
    };

    let mut top_row = Linear::new(LinearDir::Horizontal, top_children);
    if let Some(inspector_id) = inspector_id {
        top_row.shares[sidebar] = 0.22;
        top_row.shares[preview] = 0.53;
        top_row.shares[inspector_id] = 0.25;
    } else {
        top_row.shares[sidebar] = 0.30;
        top_row.shares[preview] = 0.70;
    }
    let top_row = tiles.insert_container(top_row);

    // Root: top row above, full-width timeline below.
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [top_row, timeline],
        0.65, // top row gets 65 %; timeline gets 35 %
    ));

    Tree::new("workspace", root, tiles)
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

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkspacePersistence {
    pub(crate) tree: Tree<WorkspaceTab>,
    #[serde(default)]
    pub(crate) window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) window_maximized: Option<bool>,
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
        fs::create_dir_all(parent).ok();
    }
    let state = AppState {
        recent_file: Some(recent_file.to_path_buf()),
    };
    if let Ok(serialized) = ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()) {
        fs::write(&path, serialized).ok();
    }
}

pub(super) fn clear_app_state() {
    let path = app_state_path();
    if path.exists() {
        fs::remove_file(&path).ok();
    }
}
