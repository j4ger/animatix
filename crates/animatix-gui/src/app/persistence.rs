use super::*;
use egui_tiles::{Tile, Tiles, Tree};

pub(super) fn default_tree() -> Tree<WorkspaceTab> {
    let mut tiles = Tiles::default();

    let explorer = tiles.insert_pane(WorkspaceTab::Explorer);
    let layers = tiles.insert_pane(WorkspaceTab::Layers);
    let editor = tiles.insert_pane(WorkspaceTab::Editor);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);

    let left_tabs = tiles.insert_tab_tile(vec![explorer, layers]);
    let right_col = tiles.insert_vertical_tile(vec![preview, inspector]);
    let root = tiles.insert_horizontal_tile(vec![left_tabs, editor, right_col]);

    Tree::new("workspace", root, tiles)
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
