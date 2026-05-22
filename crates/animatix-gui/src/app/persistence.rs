use super::*;
use egui_tiles::{Linear, LinearDir, Tiles, Tree};

/// Default workspace layout — canvas-centric.
///
/// ```text
/// ┌──────────────────┬──────────────────────────┐
/// │                  │        Preview (65%)     │
/// │                  │    (aspect-ratio sized)  │
/// │    Editor        ├──────────────────────────┤
/// │   (30%)          │ Sidebar │  Inspector     │
/// │                  │  (40%)  │   (60%)        │
/// │                  │       (35%)              │
/// └──────────────────┴──────────────────────────┘
///          (70%)
/// ```
///
/// Canvas dominates (~45 % of total area). Editor is a narrow column on the
/// left. The bottom strip below the preview holds sidebar + inspector.
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
        0.65, // preview gets 65 %; bottom row gets 35 %
    ));

    // Root: editor on the left, right column on the right.
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Horizontal,
        [editor, right_col],
        0.30, // editor gets 30 %, right column 70 %
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
