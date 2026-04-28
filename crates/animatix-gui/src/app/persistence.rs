use super::*;

pub(super) fn default_dock_state() -> DockState<WorkspaceTab> {
    let mut dock_state = DockState::new(vec![WorkspaceTab::Editor]);
    let surface = dock_state.main_surface_mut();
    let [editor, _explorer] =
        surface.split_left(NodeIndex::root(), 0.15, vec![WorkspaceTab::Explorer]);
    let [_editor, _preview] = surface.split_right(editor, 0.30, vec![WorkspaceTab::Preview]);
    let [_preview, _inspector] = surface.split_right(_preview, 0.18, vec![WorkspaceTab::Inspector]);
    dock_state
}

pub(super) fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

pub(super) fn load_workspace_persistence(path: &Path) -> Option<DockState<WorkspaceTab>> {
    let content = fs::read_to_string(path).ok()?;
    let persistence = ron::from_str::<WorkspacePersistence>(&content).ok()?;
    Some(persistence.dock_state)
}
