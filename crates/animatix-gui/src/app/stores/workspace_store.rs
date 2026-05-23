use crate::hot_reload::HotReloader;
use crate::app::FileTreeEntry;
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