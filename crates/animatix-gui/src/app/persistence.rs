use super::*;
use egui_tiles::{Linear, LinearDir, Tiles, Tree};

/// Default workspace layout.
///
/// ```text
/// ┌──────────────────┬──────────────────────────┐
/// │                  │      Preview (~45%)      │
/// │                  │   (aspect-ratio sized)   │
/// │    Editor        ├──────────────────────────┤
/// │   (45%)          │ Sidebar │  Inspector     │
/// │                  │  (35%)  │   (65%)        │
/// │                  │       (~55%)             │
/// └──────────────────┴──────────────────────────┘
///          (55%)
/// ```
///
/// The preview maintains its aspect ratio via `fit_preview()` and does not
/// expand to fill excess height. To avoid wasted space, the preview is given
/// a modest slice (~45 %) and the sidebar + inspector strip below absorbs
/// the remaining height. Editor is a tall column on the left.
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
        0.35, // sidebar gets 35 %, inspector 65 %
    ));

    // Right column: preview on top, sidebar/inspector strip below.
    // The preview maintains aspect ratio and doesn't expand to fill its
    // allocation, so we give it just enough space (~45 %) and let the
    // bottom row absorb the remaining usable height.
    let right_col = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [preview, bottom_row],
        0.45, // preview gets 45 %; bottom row gets the rest
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
