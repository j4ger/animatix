use std::collections::HashMap;

use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::image::SceneImage;

/// Centralized cache for loaded assets to avoid redundant parsing.
/// Assets are keyed by their source identifier (file path or content hash).
#[derive(Clone, Default)]
pub struct AssetCache {
    svg_paths: HashMap<String, Vec<VelloPath>>,
    images: HashMap<String, SceneImage>,
    text_glyphs: HashMap<String, Vec<TextPath>>,
}

impl AssetCache {
    pub fn new() -> Self {
        Self::default()
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
        if !self.text_glyphs.contains_key(key) {
            let glyphs = compile_fn(content);
            self.text_glyphs.insert(key.to_string(), glyphs);
        }
        self.text_glyphs.get(key).expect("just inserted")
    }

    /// Remove all cached entries.
    pub fn clear(&mut self) {
        self.svg_paths.clear();
        self.images.clear();
        self.text_glyphs.clear();
    }
}