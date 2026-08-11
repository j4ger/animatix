//! Unified source identity and in-memory source map for multi-file programs.
//!
//! All file keys in this module are normalized before they enter the map.
//! Disk-backed files are additionally canonicalized by [`SourceMap::path_key`],
//! so in-memory overrides and filesystem paths share one identity model.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// An error produced while resolving a file identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMapError {
    /// The requested file does not exist on disk and has no in-memory override.
    FileNotFound(PathBuf),
}

/// Normalize an absolute or relative path without touching the filesystem.
///
/// Current directory components are removed and parent components are applied
/// lexically, so equivalent paths share one identity. Prefix/root components
/// are preserved; a parent component cannot pop the filesystem root.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {},
            Component::ParentDir => {
                let can_pop = normalized
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, Component::Normal(_)));
                if can_pop {
                    normalized.pop();
                } else {
                    normalized.push(component.as_os_str());
                }
            },
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

/// Resolve an import string relative to a base file path.
pub fn resolve_import(base: &Path, import_path: &str) -> PathBuf {
    let trimmed = import_path.trim_matches('"');
    let joined = base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(trimmed);
    normalize_path(&joined)
}

/// Owned in-memory sources plus normalized path lookup.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    sources: HashMap<PathBuf, String>,
}

impl SourceMap {
    /// Create an empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace an in-memory source.
    pub fn add_source(&mut self, path: PathBuf, source: impl Into<String>) {
        let path = normalize_path(&path);
        self.sources.insert(path, source.into());
    }

    /// Remove an in-memory source.
    pub fn remove_source(&mut self, path: &Path) {
        let path = normalize_path(path);
        self.sources.remove(&path);
    }

    /// Return the in-memory source for a path, if any.
    pub fn get(&self, path: &Path) -> Option<&str> {
        self.sources.get(&normalize_path(path)).map(String::as_str)
    }

    /// Return true when an in-memory source is registered for the path.
    pub fn contains(&self, path: &Path) -> bool {
        self.sources.contains_key(&normalize_path(path))
    }

    /// Return the canonical identity for a path.
    ///
    /// In-memory sources use their normalized path directly; all other paths
    /// are canonicalized so equivalent filesystem paths map to one key.
    pub fn path_key(&self, path: &Path) -> Result<PathBuf, SourceMapError> {
        let path = normalize_path(path);
        if self.sources.contains_key(&path) {
            Ok(path)
        } else {
            std::fs::canonicalize(&path).map_err(|_| SourceMapError::FileNotFound(path))
        }
    }

    /// Resolve an import path relative to a base file path.
    pub fn resolve_import(&self, base: &Path, import_path: &str) -> PathBuf {
        resolve_import(base, import_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_map_prefers_in_memory_source_over_disk_key() {
        let path = PathBuf::from("/virtual/project/lib.amx");
        let mut map = SourceMap::new();
        assert!(map.path_key(&path).is_err());

        map.add_source(path.clone(), "pub let accent = red");
        assert_eq!(map.path_key(&path).unwrap(), path);
        assert_eq!(map.get(&path), Some("pub let accent = red"));
    }

    #[test]
    fn normalize_path_removes_cur_dir() {
        assert_eq!(normalize_path(Path::new("/a/./b.amx")), PathBuf::from("/a/b.amx"));
    }

    #[test]
    fn resolve_import_uses_normalized_identity() {
        let base = Path::new("/project/scenes/main.amx");
        assert_eq!(
            resolve_import(base, "./../lib/components.amx"),
            PathBuf::from("/project/lib/components.amx")
        );
    }

    #[test]
    fn source_map_round_trips_override_lifecycle() {
        let mut map = SourceMap::new();
        let path = PathBuf::from("/virtual/a.amx");
        map.add_source(path.clone(), "one");
        map.add_source(path.clone(), "two");
        assert_eq!(map.get(&path), Some("two"));

        map.remove_source(&path);
        assert!(!map.contains(&path));
    }
}
