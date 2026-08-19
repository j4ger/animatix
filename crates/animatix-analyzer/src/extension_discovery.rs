//! Shared filesystem discovery for `.amx-plugin.toml` manifests.
//!
//! The GUI and LSP both need the same directory order, deduplication, and
//! change fingerprinting. Keeping that here means runtime plugin loading and
//! analyzer-only language intelligence cannot drift apart.

use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::ExtensionManifest;

/// One discovered manifest plus a stable change fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestSource {
    /// Manifest file path.
    pub path: PathBuf,
    /// Parsed manifest metadata.
    pub manifest: ExtensionManifest,
    /// Content/library fingerprint used to detect reloads.
    pub fingerprint: u64,
}

/// A manifest or library file that could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestIssue {
    /// File path that produced the issue.
    pub path: PathBuf,
    /// Human-readable failure.
    pub message: String,
}

/// Discover manifests in explicit-path, document-directory, then workspace-root
/// priority order. Duplicate manifest paths are collapsed by normalized path.
pub fn discover_manifest_sources(
    document_dir: Option<&Path>,
    workspace_root: Option<&Path>,
    explicit_paths: &[PathBuf],
) -> Vec<ManifestSource> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for manifest_path in discover_manifest_paths(document_dir, workspace_root, explicit_paths) {
        if let Some(source) = load_source(&manifest_path, &mut seen) {
            sources.push(source);
        }
    }
    sources
}

/// Discover all manifest file paths without parsing them.
pub fn discover_manifest_paths(
    document_dir: Option<&Path>,
    workspace_root: Option<&Path>,
    explicit_paths: &[PathBuf],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for path in explicit_paths {
        for manifest_path in expand_manifest_target(path) {
            if seen.insert(normalized_key(&manifest_path)) {
                paths.push(manifest_path);
            }
        }
    }
    if let Some(dir) = document_dir {
        for manifest_path in manifests_in_dir(dir) {
            if seen.insert(normalized_key(&manifest_path)) {
                paths.push(manifest_path);
            }
        }
    }
    if let Some(dir) = workspace_root {
        for manifest_path in manifests_in_dir(dir) {
            if seen.insert(normalized_key(&manifest_path)) {
                paths.push(manifest_path);
            }
        }
    }
    paths
}

/// Parse one manifest path, returning either a source or a diagnostic issue.
pub fn load_manifest_source(path: &Path) -> Result<ManifestSource, ManifestIssue> {
    let source = fs::read_to_string(path).map_err(|err| ManifestIssue {
        path: path.to_path_buf(),
        message: err.to_string(),
    })?;
    let manifest = ExtensionManifest::from_toml(&source).map_err(|err| ManifestIssue {
        path: path.to_path_buf(),
        message: err,
    })?;
    Ok(ManifestSource {
        path: path.to_path_buf(),
        fingerprint: fingerprint_manifest(path, &manifest),
        manifest,
    })
}

/// Compute a combined fingerprint for a set of discovered manifests.
pub fn fingerprint_sources(sources: &[ManifestSource]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for source in sources {
        source.path.hash(&mut hasher);
        source.fingerprint.hash(&mut hasher);
    }
    hasher.finish()
}

fn expand_manifest_target(path: &Path) -> Vec<PathBuf> {
    if path.is_dir() {
        manifests_in_dir(path)
    } else if is_manifest_file(path) {
        vec![path.to_path_buf()]
    } else {
        Vec::new()
    }
}

fn manifests_in_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut manifests = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_manifest_file(path))
        .collect::<Vec<_>>();
    manifests.sort();
    manifests
}

fn is_manifest_file(path: &Path) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".amx-plugin.toml"))
}

fn load_source(path: &Path, seen: &mut HashSet<PathBuf>) -> Option<ManifestSource> {
    let key = normalized_key(path);
    if !seen.insert(key) {
        return None;
    }
    let source = fs::read_to_string(path).ok()?;
    let manifest = ExtensionManifest::from_toml(&source).ok()?;
    Some(ManifestSource {
        path: path.to_path_buf(),
        fingerprint: fingerprint_manifest(path, &manifest),
        manifest,
    })
}

fn fingerprint_manifest(path: &Path, manifest: &ExtensionManifest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Ok(source) = fs::read(path) {
        source.hash(&mut hasher);
    }
    if let Some(library) = manifest.library.as_deref() {
        let library_path = path.parent().unwrap_or_else(|| Path::new(".")).join(library);
        match fs::metadata(library_path) {
            Ok(metadata) => {
                true.hash(&mut hasher);
                metadata.len().hash(&mut hasher);
                if let Ok(modified) = metadata.modified() {
                    modified.hash(&mut hasher);
                }
            },
            Err(_) => {
                false.hash(&mut hasher);
            },
        }
    }
    hasher.finish()
}

fn normalized_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_manifest_discovery_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_manifest(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(
            &path,
            "[[primitives]]\ntype_name = \"Gauge\"\n[[properties]]\nactor_type = \"Gauge\"\nname = \"level\"\ntype = \"Num\"\n",
        )
        .expect("write manifest");
        path
    }

    #[test]
    fn discovery_uses_explicit_document_workspace_priority() {
        let workspace = temp_dir("workspace");
        let document = workspace.join("src");
        fs::create_dir_all(&document).expect("create document dir");
        let workspace_manifest = write_manifest(&workspace, "workspace.amx-plugin.toml");
        let document_manifest = write_manifest(&document, "document.amx-plugin.toml");
        let explicit_manifest = write_manifest(&workspace, "explicit.amx-plugin.toml");

        let sources = discover_manifest_sources(
            Some(&document),
            Some(&workspace),
            &[explicit_manifest.clone()],
        );
        let paths = sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths, vec![explicit_manifest, document_manifest, workspace_manifest]);
    }

    #[test]
    fn discovery_deduplicates_by_canonical_path() {
        let document = temp_dir("dedupe");
        let manifest = write_manifest(&document, "same.amx-plugin.toml");
        let sources =
            discover_manifest_sources(Some(&document), Some(&document), &[manifest.clone()]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, manifest);
    }

    #[test]
    fn fingerprint_changes_when_manifest_or_library_changes() {
        let document = temp_dir("fingerprint");
        let manifest_path = write_manifest(&document, "lib.amx-plugin.toml");
        let first = fingerprint_manifest(
            &manifest_path,
            &ExtensionManifest::from_toml(
                &fs::read_to_string(&manifest_path).expect("read manifest"),
            )
            .expect("parse manifest"),
        );
        fs::write(&manifest_path, "[[primitives]]\ntype_name = \"Dial\"\n").expect("rewrite");
        let second = fingerprint_manifest(
            &manifest_path,
            &ExtensionManifest::from_toml(
                &fs::read_to_string(&manifest_path).expect("read manifest"),
            )
            .expect("parse manifest"),
        );
        assert_ne!(first, second);
    }
}
