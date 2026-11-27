use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::app::FileTreeEntry;
use crate::app::file_tree::FileTreeLoader;
use crate::hot_reload::HotReloader;

/// Owns workspace-level state: file tree, persisted layout, recent files,
/// hot-reloader, and global settings.
pub struct WorkspaceStore {
    pub workspace_root: PathBuf,
    pub expanded_dirs: HashSet<PathBuf>,
    pub file_tree: Vec<FileTreeEntry>,
    pub(crate) file_tree_revision: u64,
    pub(crate) file_tree_loader: FileTreeLoader,
    pub persistence_path: PathBuf,
    pub hot_reloader: Option<HotReloader>,
    pub(crate) media_watch: crate::app::document::media_watch::MediaWatch,
    pub hot_reload_error: Option<String>,
    pub last_reload_time: Option<Instant>,
}

impl WorkspaceStore {
    pub fn new(
        workspace_root: PathBuf,
        expanded_dirs: HashSet<PathBuf>,
        persistence_path: PathBuf,
        hot_reloader: Option<HotReloader>,
        hot_reload_error: Option<String>,
    ) -> Self {
        Self {
            workspace_root,
            expanded_dirs,
            file_tree: Vec::new(),
            file_tree_revision: 0,
            file_tree_loader: FileTreeLoader::default(),
            persistence_path,
            hot_reloader,
            media_watch: Default::default(),
            hot_reload_error,
            last_reload_time: None,
        }
    }

    pub(crate) fn set_root(&mut self, path: PathBuf) {
        self.expanded_dirs = HashSet::from([path.clone()]);
        if self.workspace_root != path {
            self.workspace_root = path;
            self.replace_file_tree(Vec::new());
        }
    }

    pub(crate) fn request_file_tree(&mut self, file: &Path, refresh: bool) {
        self.file_tree_loader
            .request(&self.workspace_root, file, &self.expanded_dirs, refresh);
    }

    pub(crate) fn poll_file_tree(&mut self) {
        if let Some(tree) = self.file_tree_loader.poll() {
            self.replace_file_tree(tree);
        }
    }
    fn replace_file_tree(&mut self, tree: Vec<FileTreeEntry>) {
        self.file_tree_revision += 1;
        self.file_tree_loader.retire(std::mem::replace(&mut self.file_tree, tree));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_store_new_creates_valid_store() {
        let workspace_root = PathBuf::from("/tmp/test_workspace");
        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let persistence_path = PathBuf::from(".test_persistence.ron");

        let store = WorkspaceStore::new(
            workspace_root.clone(),
            expanded_dirs.clone(),
            persistence_path,
            None,
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
            PathBuf::from(".test_persistence.ron"),
            None,
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
