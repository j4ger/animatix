use std::collections::{BTreeSet, HashMap};

use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::image::SceneImage;

/// Centralized cache for loaded assets and the actors that reference them.
///
/// Assets are keyed by their source identifier (file path). Usage tracking maps
/// each asset to the actor labels that loaded it, so GUI tooling can show asset
/// references and future hot reload can invalidate only changed files.
#[derive(Clone, Default)]
pub struct AssetCache {
    svg_paths: HashMap<String, Vec<VelloPath>>,
    images: HashMap<String, SceneImage>,
    text_glyphs: HashMap<String, Vec<TextPath>>,
    usage: HashMap<String, BTreeSet<String>>,
}

impl AssetCache {
    /// Create a new empty asset cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load an SVG file and record that the actor named by `actor_or_subject`
    /// references it. A property subject like `icon.url` is normalized to `icon`.
    ///
    /// Returns cached paths when the same file was loaded before.
    pub fn load_svg_for(
        &mut self,
        path: &str,
        actor_or_subject: &str,
    ) -> Result<Vec<VelloPath>, String> {
        let paths = if let Some(paths) = self.svg_paths.get(path) {
            paths.clone()
        } else {
            let parsed = crate::timeline::svg::parse_svg_file(path)?;
            self.svg_paths.insert(path.to_string(), parsed.clone());
            parsed
        };
        self.record_usage_for_subject(path, actor_or_subject);
        Ok(paths)
    }

    /// Load an image file and record that the actor named by `actor_or_subject`
    /// references it. A property subject like `icon.url` is normalized to `icon`.
    ///
    /// Returns cached image data when the same file was loaded before.
    pub fn load_image_for(
        &mut self,
        path: &str,
        actor_or_subject: &str,
    ) -> Result<SceneImage, String> {
        let image = if let Some(image) = self.images.get(path) {
            image.clone()
        } else {
            let loaded = crate::timeline::image::load_image_file(path)?;
            self.images.insert(path.to_string(), loaded.clone());
            loaded
        };
        self.record_usage_for_subject(path, actor_or_subject);
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

    fn record_usage_for_subject(&mut self, path: &str, actor_or_subject: &str) {
        let actor = actor_or_subject
            .rsplit_once('.')
            .map(|(actor, _)| actor)
            .unwrap_or(actor_or_subject);
        self.record_usage(path, actor);
    }

    /// Get or load SVG paths from a file path.
    /// Returns cached result if the same path was previously loaded.
    pub fn get_or_load_svg(&mut self, path: &str) -> Option<&Vec<VelloPath>> {
        if !self.svg_paths.contains_key(path) {
            let parsed = crate::timeline::svg::parse_svg_file(path).ok()?;
            self.svg_paths.insert(path.to_string(), parsed);
        }
        self.svg_paths.get(path)
    }

    /// Get or load an image from a file path.
    pub fn get_or_load_image(&mut self, path: &str) -> Option<&SceneImage> {
        if !self.images.contains_key(path) {
            let loaded = crate::timeline::image::load_image_file(path).ok()?;
            self.images.insert(path.to_string(), loaded);
        }
        self.images.get(path)
    }

    /// Get or compile text/math/code paths.
    /// The key should be unique per content + style combination.
    pub fn get_or_compile_text(
        &mut self,
        key: &str,
        content: &str,
        compile_fn: impl FnOnce(&str) -> Vec<TextPath>,
    ) -> &Vec<TextPath> {
        use std::collections::hash_map::Entry;
        match self.text_glyphs.entry(key.to_string()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(compile_fn(content)),
        }
    }

    /// Drop cached payloads for one asset path.
    ///
    /// Usage references are retained so tooling can still report that the actor
    /// depends on the path after a file changes.
    pub fn invalidate_asset(&mut self, path: &str) {
        self.svg_paths.remove(path);
        self.images.remove(path);
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

    /// Remove all cached entries and usage references.
    pub fn clear(&mut self) {
        self.svg_paths.clear();
        self.images.clear();
        self.text_glyphs.clear();
        self.usage.clear();
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
    fn invalidation_drops_cached_payload_but_keeps_usage() {
        let mut cache = AssetCache::new();
        let path = test_svg_path("test_basic.svg");
        let path_str = path.display().to_string();

        cache.load_svg_for(&path_str, "icon").expect("load svg");
        cache.invalidate_asset(&path_str);
        assert!(cache.svg_paths().next().is_none());
        assert_eq!(cache.assets_for("icon").count(), 1);
    }

    #[test]
    fn property_subject_usage_is_normalized_to_actor() {
        let mut cache = AssetCache::new();
        let path = test_svg_path("test_basic.svg");
        let path_str = path.display().to_string();

        cache.load_svg_for(&path_str, "icon.url").expect("load svg");
        assert_eq!(cache.assets_for("icon").count(), 1);
        assert_eq!(cache.assets_for("icon.url").count(), 0);
    }
}
