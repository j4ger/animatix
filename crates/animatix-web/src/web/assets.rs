//! Scene loading: source → built document, imported modules, fonts, and asset
//! resolution.

use std::sync::Arc;

use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::{BuildQuality, SceneDimensions};
use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::parse_source;
use wasm_bindgen::prelude::*;

use crate::host;
use crate::host::BuiltDocument;

use super::AmxPlayer;

#[wasm_bindgen]
impl AmxPlayer {
    /// Parse + build `source`, replacing the loaded document on success.
    ///
    /// Returns a [`LoadResultDto`] as a JS object; on parse/build failure the
    /// previous document stays loaded for last-known-good rendering.
    pub fn load_source(&mut self, source: &str) -> Result<JsValue, JsError> {
        self.load_source_with_cache(source, None)
    }

    /// Register an in-memory font (TTF/OTF) so later `load_source` calls can
    /// name it through `font_family` — the family comes from the font's own
    /// name table. Must run before `load_source`: scene text is compiled at
    /// build time, so changing the font set means re-loading the scene.
    ///
    /// A page that cannot ship a font can skip this entirely; the bundled
    /// Open Sans faces remain the default.
    pub fn add_font(&mut self, bytes: &[u8]) {
        self.fonts.push(bytes.to_vec());
    }

    /// Register an imported `.amx` module (the file's text) so later
    /// `load_source` calls can resolve the import. `path` is the *resolved*
    /// import path — exactly what `missing_imports` on a failed load reports
    /// (the import string joined onto the importing file's directory and
    /// normalized). Re-registering a path replaces its text.
    ///
    /// Like fonts, modules persist across loads; a scene that imports nothing
    /// outside the bundled library needs none.
    pub fn add_module(&mut self, path: String, source: String) {
        if let Some(existing) = self.modules.iter_mut().find(|(p, _)| *p == path) {
            existing.1 = source;
        } else {
            self.modules.push((path, source));
        }
    }

    /// Set the build quality for later `load_source` calls: "draft" (the
    /// default; the GUI's editing preview), "preview" (scrubbing fidelity), or
    /// "production" (what a desktop export renders). Quality is a build-time
    /// knob — plot-family actors sample with different tolerance — so the
    /// embed re-loads the scene after changing it.
    pub fn set_quality(&mut self, quality: &str) -> Result<(), JsError> {
        self.quality = match quality.to_ascii_lowercase().as_str() {
            "draft" => BuildQuality::Draft,
            "preview" => BuildQuality::Preview,
            "production" => BuildQuality::Production,
            other => {
                return Err(JsError::new(&format!(
                    "unknown quality '{other}': expected draft, preview, or production"
                )));
            },
        };
        Ok(())
    }

    /// Asset URLs referenced by `Image`/`Svg` actors through a literal `url`
    /// property, in declaration order and deduplicated. Dynamic assignments
    /// (`icon.url = expr`) are not listed.
    pub fn list_asset_urls(&mut self, source: &str) -> Result<JsValue, JsError> {
        // Parse errors are not reported here: `load_source` surfaces them with
        // full context, and a scene that does not parse has no asset urls.
        let (ast, _) = parse_source(source);
        let urls = match ast {
            Some(ast) => Self::collect_asset_urls(&ast),
            None => Vec::new(),
        };
        serde_wasm_bindgen::to_value(&urls)
            .map_err(|e| JsError::new(&format!("failed to serialize asset urls: {e}")))
    }

    /// Parse + build `source` with pre-fetched asset bytes, replacing the
    /// loaded document on success. `urls`/`payloads` are parallel arrays: an
    /// `.svg` payload is the file's text, anything else its encoded bytes.
    ///
    /// A missing or undecodable asset is reported when the scene is built —
    /// the same `MediaLoadFailure` a desktop build produces.
    pub fn load_source_with_assets(
        &mut self,
        source: &str,
        urls: Vec<JsValue>,
        payloads: Vec<JsValue>,
    ) -> Result<JsValue, JsError> {
        // `mut` is only needed under the `svg` feature — the slim profile
        // inserts nothing mutable — so the slim wasm build warns without this.
        // The x86 clippy job never compiles this crate's wasm path, which is how
        // it stayed unseen.
        #[allow(unused_mut)]
        let mut cache = AssetCache::new();
        for (url, payload) in urls.into_iter().zip(payloads) {
            let Some(url) = url.as_string() else { continue };
            if let Some(text) = payload.as_string() {
                #[cfg(feature = "svg")]
                cache
                    .insert_svg_source(&url, &text)
                    .map_err(|e| JsError::new(&format!("asset '{url}': {e}")))?;
                #[cfg(not(feature = "svg"))]
                {
                    let _ = (&url, &text);
                }
            } else if let Some(bytes) = payload.dyn_ref::<js_sys::Uint8Array>() {
                #[cfg(feature = "image-decode")]
                cache
                    .insert_image_bytes(&url, &bytes.to_vec())
                    .map_err(|e| JsError::new(&format!("asset '{url}': {e}")))?;
                #[cfg(not(feature = "image-decode"))]
                {
                    let _ = (&url, bytes);
                }
            }
        }
        self.load_source_with_cache(source, Some(Arc::new(cache)))
    }

    fn load_source_with_cache(
        &mut self,
        source: &str,
        assets: Option<Arc<AssetCache>>,
    ) -> Result<JsValue, JsError> {
        let mut font_context = FontContext::new();
        for bytes in &self.fonts {
            font_context.load_font_bytes(bytes.clone());
        }
        let modules: Vec<(std::path::PathBuf, String)> = self
            .modules
            .iter()
            .map(|(path, text)| (std::path::PathBuf::from(path), text.clone()))
            .collect();
        let built = host::build_document_with_modules(
            source,
            &modules,
            Arc::new(font_context),
            self.quality,
            assets,
        );
        let BuiltDocument { target, result } = built;

        if let Some(target) = target {
            self.dims = SceneDimensions {
                width: result.width,
                height: result.height,
            };
            self.duration_s = result.duration_s;
            // Filter targets are sized in scene space; rebuild for new dims.
            self.filter_backend = None;
            self.filter_backend_to = None;
            // A rebuilt document is different content at the same playhead time,
            // which the timeline epoch alone cannot see (both start at 0), so the
            // frame the canvas is holding is no longer describable by the old
            // signature.
            self.document_generation += 1;
            self.dedup.invalidate();
            self.target = Some(target);
        }

        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsError::new(&format!("failed to serialize build result: {e}")))
    }

    /// Asset URLs referenced by the given statements, in declaration order.
    fn collect_asset_urls(stmts: &[Stmt]) -> Vec<String> {
        let mut urls = Vec::new();
        walk_asset_urls(stmts, &mut urls);
        urls
    }
}

/// Walk statements (and keyframe bodies) collecting literal `url` properties
/// from `Image`/`Svg` actor declarations.
fn walk_asset_urls(stmts: &[Stmt], urls: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::ActorDecl { ty, props, .. } => {
                if matches!(ty.as_str(), "Image" | "Svg") {
                    if let Some(prop) = props.iter().find(|prop| prop.name == "url") {
                        if let animatix_syntax::ast::Expr::Str(url) = &prop.value {
                            if !url.is_empty() && !urls.contains(url) {
                                urls.push(url.clone());
                            }
                        }
                    }
                }
            },
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                walk_asset_urls(body, urls);
            },
            _ => {},
        }
    }
}
