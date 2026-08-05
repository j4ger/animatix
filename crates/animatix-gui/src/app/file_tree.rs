use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{FileTreeEntry, MAX_TREE_DEPTH, MAX_TREE_ENTRIES};

pub(super) fn workspace_root_for(file_path: &Path) -> PathBuf {
    for ancestor in file_path.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    file_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}

pub(super) fn build_file_tree(
    workspace_root: &Path,
    current_file: &Path,
    expanded_dirs: &HashSet<PathBuf>,
) -> Vec<FileTreeEntry> {
    let mut entries = Vec::new();
    let mut remaining = MAX_TREE_ENTRIES;
    collect_tree_entries(
        workspace_root,
        current_file,
        expanded_dirs,
        0,
        &mut remaining,
        &mut entries,
    );
    entries
}

fn collect_tree_entries(
    dir: &Path,
    current_file: &Path,
    expanded_dirs: &HashSet<PathBuf>,
    depth: usize,
    remaining: &mut usize,
    entries: &mut Vec<FileTreeEntry>,
) {
    if depth > MAX_TREE_DEPTH || *remaining == 0 {
        return;
    }

    let read_dir = match fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(err) => {
            tracing::debug!("failed to read directory '{}': {}", dir.display(), err);
            return;
        },
    };

    let mut children: Vec<_> = read_dir
        .filter_map(Result::ok)
        .map(|entry| {
            let is_dir = entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or_else(|_| entry.path().is_dir());
            (entry, is_dir)
        })
        .collect();
    children.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.0.file_name().cmp(&b.0.file_name()),
    });

    for (child, is_dir) in children {
        if *remaining == 0 {
            return;
        }

        let path = child.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            let is_ancestor_of_current = current_file.ancestors().any(|ancestor| ancestor == path);
            if !is_ancestor_of_current {
                continue;
            }
        }

        entries.push(FileTreeEntry {
            path: path.clone(),
            name: name.to_string(),
            depth,
            is_dir,
        });
        *remaining = remaining.saturating_sub(1);

        if is_dir && expanded_dirs.contains(&path) {
            collect_tree_entries(&path, current_file, expanded_dirs, depth + 1, remaining, entries);
        }
    }
}
