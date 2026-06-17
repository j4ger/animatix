use crate::app::FileTreeEntry;
use crate::hot_reload::HotReloader;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

/// Owns workspace-level state: file tree, persisted layout, recent files,
/// hot-reloader, and global settings.
pub struct WorkspaceStore {
    pub workspace_root: PathBuf,
    pub expanded_dirs: HashSet<PathBuf>,
    pub file_tree: Vec<FileTreeEntry>,
    pub persistence_path: PathBuf,
    pub hot_reloader: Option<HotReloader>,
    pub last_reload_time: Option<Instant>,
}

impl WorkspaceStore {
    pub fn new(
        workspace_root: PathBuf,
        expanded_dirs: HashSet<PathBuf>,
        file_tree: Vec<FileTreeEntry>,
        persistence_path: PathBuf,
        hot_reloader: Option<HotReloader>,
    ) -> Self {
        Self {
            workspace_root,
            expanded_dirs,
            file_tree,
            persistence_path,
            hot_reloader,
            last_reload_time: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_store_new_creates_valid_store() {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = Vec::new();
        let persistence_path = PathBuf::from(".test_persistence.ron");

        let store = WorkspaceStore::new(
            workspace_root.clone(),
            expanded_dirs.clone(),
            file_tree,
            persistence_path,
            None,
        );

        assert_eq!(store.workspace_root, workspace_root);
        assert_eq!(store.expanded_dirs, expanded_dirs);
        assert!(store.hot_reloader.is_none());
        assert!(store.last_reload_time.is_none());
    }

    #[test]
    fn workspace_store_expanded_dirs_can_be_modified() {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let mut expanded_dirs = HashSet::new();
        expanded_dirs.insert(workspace_root.clone());

        let mut store = WorkspaceStore::new(
            workspace_root.clone(),
            expanded_dirs,
            Vec::new(),
            PathBuf::from(".test_persistence.ron"),
            None,
        );

        assert_eq!(store.expanded_dirs.len(), 1);

        // Add a new directory
        let new_dir = PathBuf::from("/tmp/test_workspace/subdir");
        store.expanded_dirs.insert(new_dir.clone());
        assert_eq!(store.expanded_dirs.len(), 2);
        assert!(store.expanded_dirs.contains(&new_dir));

        // Remove a directory
        store.expanded_dirs.remove(&workspace_root);
        assert_eq!(store.expanded_dirs.len(), 1);
        assert!(store.expanded_dirs.contains(&new_dir));
    }
}
