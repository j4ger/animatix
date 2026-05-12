use super::*;
use egui_tiles::{Linear, LinearDir, Tiles, Tree};

/// Default workspace layout.
///
/// ```text
/// ┌──────────────────┬──────────────────────────┐
/// │                  │      Preview (70%)       │
/// │                  │                          │
/// │    Editor        ├──────────────────────────┤
/// │   (45%)          │ Sidebar │  Inspector     │
/// │                  │  (40%)  │   (60%)        │
/// │                  │  (30%)  │                │
/// └──────────────────┴──────────────────────────┘
///          (55%)
/// ```
///
/// The preview is the star of the show — it gets the full width of the
/// right half and the majority of its column height.  Editor is a tall
/// column on the left.  Sidebar + Inspector share a compact strip at the
/// bottom right, minimizing the dead space each had when they were full-height
/// panels.
pub(super) fn default_tree() -> Tree<WorkspaceTab> {
    let mut tiles = Tiles::default();

    let sidebar = tiles.insert_pane(WorkspaceTab::Sidebar);
    let editor = tiles.insert_pane(WorkspaceTab::Editor);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);

    // Bottom-right strip: sidebar + inspector side by side.
    let bottom_row = tiles.insert_container(Linear::new_binary(
        LinearDir::Horizontal,
        [sidebar, inspector],
        0.40, // sidebar gets 40 %, inspector 60 %
    ));

    // Right column: preview on top, sidebar/inspector strip below.
    let right_col = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [preview, bottom_row],
        0.70, // preview gets 70 % of the column height
    ));

    // Root: editor on the left, right column on the right.
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Horizontal,
        [editor, right_col],
        0.45, // editor gets 45 %, right column 55 %
    ));

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
