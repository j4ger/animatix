use std::collections::{BTreeSet, HashMap};

use crate::renderer::types::VelloPath;
use crate::timeline::image::SceneImage;

/// Centralized cache for loaded assets and the actors that reference them.
///
/// Assets are keyed by their source identifier (file path). Usage tracking maps
/// each asset to the actor labels that loaded it, so GUI tooling can show asset
/// references. The cache is rebuilt with each timeline build, so usage is
/// naturally re-derived from the current source instead of requiring
/// per-asset invalidation.
#[derive(Clone, Default)]
pub struct AssetCache {
    svg_paths: HashMap<String, Vec<VelloPath>>,
    images: HashMap<String, SceneImage>,
    usage: HashMap<String, BTreeSet<String>>,
}

impl AssetCache {
    /// Create a new empty asset cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load an SVG file and record that `actor_label` references it.
    ///
    /// Returns cached paths when the same file was loaded before.
    pub fn load_svg_for(
        &mut self,
        path: &str,
        actor_label: &str,
    ) -> Result<Vec<VelloPath>, String> {
        let paths = if let Some(paths) = self.svg_paths.get(path) {
            paths.clone()
        } else {
            let parsed = crate::timeline::svg::parse_svg_file(path)?;
            self.svg_paths.insert(path.to_string(), parsed.clone());
            parsed
        };
        self.record_usage(path, actor_label);
        Ok(paths)
    }

    /// Load an image file and record that `actor_label` references it.
    ///
    /// Returns cached image data when the same file was loaded before.
    pub fn load_image_for(&mut self, path: &str, actor_label: &str) -> Result<SceneImage, String> {
        let image = if let Some(image) = self.images.get(path) {
            image.clone()
        } else {
            let loaded = crate::timeline::image::load_image_file(path)?;
            self.images.insert(path.to_string(), loaded.clone());
            loaded
        };
        self.record_usage(path, actor_label);
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
}
