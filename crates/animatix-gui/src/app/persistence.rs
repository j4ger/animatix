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

pub(super) fn load_workspace_persistence(path: &Path) -> Option<Tree<WorkspaceTab>> {
    let content = fs::read_to_string(path).ok()?;
    ron::from_str::<Tree<WorkspaceTab>>(&content).ok()
}
