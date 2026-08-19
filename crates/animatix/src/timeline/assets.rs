use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::renderer::types::VelloPath;
use crate::timeline::image::SceneImage;

/// File metadata captured when a decoded asset is cached.
#[derive(Debug, Clone, Copy)]
struct AssetFileMetadata {
    len: u64,
    modified: Option<SystemTime>,
}

/// Centralized cache for loaded assets and the actors that reference them.
///
/// Assets are keyed by their normalized source identifier. Relative URLs are
/// resolved against the document directory first and the workspace root second
/// when one is configured, so native plugins and built-in media agree on the
/// meaning of `"img/foo.png"`. Usage tracking maps each asset to the actor
/// labels that loaded it, so GUI tooling can show asset references. Rebuilds
/// can carry an existing `Arc<AssetCache>` forward; the cache records file
/// metadata on load so only paths that changed on disk need to be reloaded.
#[derive(Clone, Default)]
pub struct AssetCache {
    document_dir: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    svg_paths: HashMap<String, Vec<VelloPath>>,
    images: HashMap<String, SceneImage>,
    usage: HashMap<String, BTreeSet<String>>,
    metadata: HashMap<String, AssetFileMetadata>,
}

impl AssetCache {
    /// Create a new empty asset cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure base directories used to resolve relative asset URLs.
    ///
    /// Document directory wins over workspace root; absolute and remote URLs
    /// are never rewritten.
    pub fn set_base_dirs(&mut self, document_dir: Option<&Path>, workspace_root: Option<&Path>) {
        self.document_dir = document_dir.map(Path::to_path_buf);
        self.workspace_root = workspace_root.map(Path::to_path_buf);
    }

    /// Normalize an asset identifier to the cache key used by the engine.
    pub fn normalize_asset_url(&self, url: &str) -> String {
        if url.is_empty()
            || url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("data:")
        {
            return url.to_string();
        }
        let path = Path::new(url);
        if path.is_absolute() {
            return normalize_lexically(path.to_path_buf()).display().to_string();
        }
        if let Some(document_dir) = &self.document_dir {
            return normalize_lexically(document_dir.join(path)).display().to_string();
        }
        if let Some(workspace_root) = &self.workspace_root {
            return normalize_lexically(workspace_root.join(path)).display().to_string();
        }
        normalize_lexically(PathBuf::from(path)).display().to_string()
    }

    /// Load an SVG file and record that `actor_label` references it.
    ///
    /// Returns cached paths when the same file was loaded before.
    pub fn load_svg_for(
        &mut self,
        path: &str,
        actor_label: &str,
    ) -> Result<Vec<VelloPath>, String> {
        let key = self.normalize_asset_url(path);
        let paths = if let Some(paths) = self.svg_paths.get(&key) {
            paths.clone()
        } else {
            let parsed = crate::timeline::svg::parse_svg_file(&key)?;
            self.record_file_metadata(&key);
            self.svg_paths.insert(key.clone(), parsed.clone());
            parsed
        };
        self.record_usage(&key, actor_label);
        Ok(paths)
    }

    /// Load an image file and record that `actor_label` references it.
    ///
    /// Returns cached image data when the same file was loaded before.
    pub fn load_image_for(&mut self, path: &str, actor_label: &str) -> Result<SceneImage, String> {
        let key = self.normalize_asset_url(path);
        let image = if let Some(image) = self.images.get(&key) {
            image.clone()
        } else {
            let loaded = crate::timeline::image::load_image_file(&key)?;
            self.record_file_metadata(&key);
            self.images.insert(key.clone(), loaded.clone());
            loaded
        };
        self.record_usage(&key, actor_label);
        Ok(image)
    }

    /// Record that an actor uses an asset without caching its decoded payload.
    ///
    /// Audio is kept out of the visual cache because the GUI decodes it through
    /// a separate bounded LRU, but it should still participate in usage tracking.
    pub fn record_usage(&mut self, path: &str, actor: &str) {
        if !path.is_empty() {
            self.usage.entry(path.to_string()).or_default().insert(actor.to_string());
        }
    }

    /// Iterate over asset path → actor labels that reference it.
    pub fn asset_usage(&self) -> impl Iterator<Item = (&String, &BTreeSet<String>)> {
        self.usage.iter()
    }

    /// Iterate over the assets referenced by a single actor.
    pub fn assets_for(&self, actor: &str) -> impl Iterator<Item = &String> {
        self.usage.iter().filter_map(move |(path, actors)| {
            if actors.contains(actor) {
                Some(path)
            } else {
                None
            }
        })
    }

    /// Iterate over cached SVG paths.
    pub fn svg_paths(&self) -> impl Iterator<Item = (&String, &Vec<VelloPath>)> {
        self.svg_paths.iter()
    }

    /// Iterate over cached images.
    pub fn images(&self) -> impl Iterator<Item = (&String, &SceneImage)> {
        self.images.iter()
    }

    /// Read a cached image by source path, without loading or usage tracking.
    pub fn get_image(&self, path: &str) -> Option<SceneImage> {
        self.images.get(&self.normalize_asset_url(path)).cloned()
    }

    /// Drop decoded payloads whose file metadata changed since they were loaded.
    ///
    /// Usage entries are dropped with the payload, so a later `load_*_for` call
    /// reloads the file and re-records the actor reference.
    pub fn invalidate_changed_assets(&mut self) {
        let mut changed: Vec<String> = Vec::new();
        for path in self.svg_paths.keys().chain(self.images.keys()) {
            if !self.file_metadata_matches(path) {
                changed.push(path.clone());
            }
        }
        for path in changed {
            self.drop_asset(&path);
        }
    }

    /// Drop cached payloads and usage for paths not present in `referenced_paths`.
    pub fn prune_unreferenced(&mut self, referenced_paths: &HashSet<String>) {
        let stale: Vec<String> = self
            .usage
            .keys()
            .filter(|path| !referenced_paths.contains(*path))
            .cloned()
            .collect();
        for path in stale {
            self.drop_asset(&path);
        }
    }

    fn file_metadata_matches(&self, path: &str) -> bool {
        let Some(recorded) = self.metadata.get(path) else {
            return false;
        };
        match std::fs::metadata(path) {
            Ok(metadata) => {
                metadata.len() == recorded.len && metadata.modified().ok() == recorded.modified
            },
            Err(_) => false,
        }
    }

    fn record_file_metadata(&mut self, path: &str) {
        if let Ok(metadata) = std::fs::metadata(path) {
            self.metadata.insert(
                path.to_string(),
                AssetFileMetadata {
                    len: metadata.len(),
                    modified: metadata.modified().ok(),
                },
            );
        }
    }

    fn drop_asset(&mut self, path: &str) {
        self.svg_paths.remove(path);
        self.images.remove(path);
        self.usage.remove(path);
        self.metadata.remove(path);
    }
}

fn normalize_lexically(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => {
                if !normalized.pop() && !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            },
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_svg_path(name: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests/svg");
        path.push(name);
        path
    }

    fn test_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("animatix_asset_cache_tests_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_test_svg(name: &str, rect_width: u32) -> PathBuf {
        let path = test_dir().join(name);
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="20">
  <rect x="0" y="0" width="{rect_width}" height="10"/>
</svg>
"#
        );
        std::fs::write(&path, source).unwrap();
        path
    }

    #[test]
    fn svg_usage_is_recorded_and_cache_is_reused() {
        let mut cache = AssetCache::new();
        let path = test_svg_path("test_basic.svg");
        let path_str = path.display().to_string();

        let first = cache.load_svg_for(&path_str, "icon").expect("load svg");
        assert!(!first.is_empty());
        let second = cache.load_svg_for(&path_str, "icon").expect("load svg again");
        assert!(!second.is_empty());
        assert_eq!(cache.asset_usage().count(), 1);
        assert_eq!(cache.assets_for("icon").count(), 1);
        assert_eq!(cache.asset_usage().next().unwrap().0, &path_str);
    }

    #[test]
    fn dotted_actor_labels_are_preserved_in_usage() {
        let mut cache = AssetCache::new();
        let path = test_svg_path("test_basic.svg");
        let path_str = path.display().to_string();

        cache.load_svg_for(&path_str, "group.icon").expect("load svg");
        assert_eq!(cache.assets_for("group.icon").count(), 1);
        assert_eq!(cache.assets_for("group").count(), 0);
        assert_eq!(cache.assets_for("icon").count(), 0);
    }

    #[test]
    fn invalidate_changed_assets_reloads_only_changed_path() {
        let changed = write_test_svg("changed.svg", 10);
        let kept = write_test_svg("kept.svg", 20);
        let changed_str = changed.display().to_string();
        let kept_str = kept.display().to_string();

        let mut cache = AssetCache::new();
        cache.load_svg_for(&changed_str, "changed_actor").expect("load changed svg");
        cache.load_svg_for(&kept_str, "kept_actor").expect("load kept svg");
        assert_eq!(cache.svg_paths().count(), 2);
        assert_eq!(cache.asset_usage().count(), 2);

        // Rewrite only one file with a different payload length.
        std::fs::write(
            &changed,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="20">
  <rect x="0" y="0" width="50" height="10"/>
  <circle cx="5" cy="5" r="2" fill="red"/>
</svg>
"#,
        )
        .unwrap();

        cache.invalidate_changed_assets();
        assert_eq!(cache.svg_paths().count(), 1);
        assert_eq!(cache.asset_usage().count(), 1);
        assert!(cache.svg_paths().all(|(path, _)| path != &changed_str));

        cache.load_svg_for(&changed_str, "changed_actor").expect("reload changed svg");
        assert_eq!(cache.svg_paths().count(), 2);
        assert_eq!(cache.asset_usage().count(), 2);
    }

    #[test]
    fn prune_unreferenced_drops_only_unused_paths() {
        let used = write_test_svg("used.svg", 10);
        let unused = write_test_svg("unused.svg", 20);
        let used_str = used.display().to_string();
        let unused_str = unused.display().to_string();

        let mut cache = AssetCache::new();
        cache.load_svg_for(&used_str, "used_actor").expect("load used svg");
        cache.load_svg_for(&unused_str, "unused_actor").expect("load unused svg");

        cache.prune_unreferenced(&HashSet::from([used_str.clone()]));
        assert_eq!(cache.svg_paths().count(), 1);
        assert_eq!(cache.asset_usage().count(), 1);
        assert_eq!(cache.svg_paths().next().unwrap().0, &used_str);

        cache.load_svg_for(&unused_str, "unused_actor").expect("reload unused svg");
        assert_eq!(cache.svg_paths().count(), 2);
    }

    #[test]
    fn relative_urls_resolve_against_document_dir() {
        let dir = std::env::temp_dir().join("animatix_asset_base_dir_tests");
        let mut cache = AssetCache::new();
        cache.set_base_dirs(Some(&dir), None);
        let key = cache.normalize_asset_url("img/logo.png");
        assert_eq!(Path::new(&key), dir.join("img/logo.png").as_path());
        assert_eq!(
            cache.normalize_asset_url("https://example.com/logo.png"),
            "https://example.com/logo.png"
        );
    }

    #[test]
    fn rebuild_with_existing_cache_preserves_usage() {
        let path = write_test_svg("rebuild.svg", 10);
        let source = format!("#0s\nicon: Svg {{ url: \"{}\" }}\n", path.display());
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
        let ast = ast.expect("parsed AST");

        let first = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let old_cache = first.output.asset_cache.clone();
        assert_eq!(old_cache.asset_usage().count(), 1);

        let second = crate::timeline::Timeline::build_with_diagnostics_and_asset_cache(
            &ast,
            &std::collections::HashMap::new(),
            Some(old_cache.clone()),
        );
        assert_eq!(second.output.asset_cache().asset_usage().count(), 1);
        assert_eq!(second.output.asset_cache().assets_for("icon").count(), 1);
        assert_eq!(old_cache.asset_usage().count(), 1);
    }
}
