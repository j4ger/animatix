use crate::hot_reload::HotReloader;
use egui_tiles::{Tile, Tree};
use std::collections::HashSet;
use std::path::PathBuf;

/// Owns workspace-level state: file tree, persisted layout, recent files,
/// hot-reloader, and global settings.
pub struct WorkspaceStore {
    pub workspace_root: PathBuf,
    pub expanded_dirs: HashSet<PathBuf>,
    pub file_tree: Vec<crate::app::file_tree::FileTreeEntry>,
    pub tree: Tree<crate::app::WorkspaceTab>,
    pub persistence_path: PathBuf,
    pub recent_files: Vec<PathBuf>,
    pub hot_reloader: Option<HotReloader>,
    pub theme: crate::app::design_tokens::Theme,
}
