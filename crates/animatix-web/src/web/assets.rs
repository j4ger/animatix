//! Scene loading: source → built document, imported modules, fonts, and asset
//! resolution.

use std::sync::Arc;

use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::{BuildQuality, SceneDimensions};
use wasm_bindgen::prelude::*;

use crate::host;
use crate::host::BuiltDocument;

use super::AmxPlayer;

#[wasm_bindgen]
impl AmxPlayer {
    /// Parse + build `source`, replacing the loaded document on success.
    ///
    /// Applies the variable overrides registered with [`AmxPlayer::set_vars`],
    /// the modules registered with [`AmxPlayer::add_module`], and the quality
    /// set with [`AmxPlayer::set_quality`].
    ///
    /// Returns a [`LoadResultDto`] as a JS object; on parse/build failure the
    /// previous document stays loaded for last-known-good rendering.
    pub fn load_source(&mut self, source: &str) -> Result<JsValue, JsError> {
        self.load_source_with_cache(source, None)
    }

    /// Set the build-time variable overrides every later `load_source`
    /// applies: the wasm half of the CLI's repeatable `--set NAME=VALUE`, and
    /// what the embed's `data-vars` attribute and `setVars()` method feed.
    ///
    /// `vars` is a plain JS object mapping a name to a number, string or
    /// boolean — `{ amp: 320, label: "Q3", mirrored: true }` — and the call
    /// *replaces* the whole set, so a page changing one value sends them all
    /// (the embed merges for you). `null`/`undefined` clears it. Each entry is
    /// rendered into Animatix expression text by [`crate::host::vars`], and the
    /// engine's own parser decides what that text means: a name that is not an
    /// identifier, or a value that does not parse, comes back as a build
    /// diagnostic on the next load instead of an error here. A JSON string
    /// becomes a string *literal*, not a number — write numbers as numbers.
    ///
    /// Returns how many entries were accepted. A value this boundary cannot
    /// render (an object, an array, `null`, `NaN`, a string the literal grammar
    /// cannot hold) is dropped with a `tracing::warn!` and left out of the
    /// count, so a caller comparing it with `Object.keys(vars).length` sees the
    /// drop rather than guessing. Never panics.
    ///
    /// Like fonts, modules and quality the set persists across loads, and
    /// because it is baked in at build time a change means a re-load.
    pub fn set_vars(&mut self, vars: &JsValue) -> Result<u32, JsError> {
        self.vars = js_var_defines(vars)?;
        Ok(u32::try_from(self.vars.len()).unwrap_or(u32::MAX))
    }

    /// The overrides [`AmxPlayer::set_vars`] rendered, as an array of
    /// `{ name, value }` ([`crate::dto::VarOverrideDto`]) where `value` is the
    /// expression text the engine parses. Read-back for a page confirming what
    /// a scalar became — the getter beside its setter, like
    /// [`AmxPlayer::render_scale`].
    pub fn vars(&self) -> Result<JsValue, JsError> {
        let rendered: Vec<crate::dto::VarOverrideDto> = self
            .vars
            .iter()
            .map(|(name, value)| crate::dto::VarOverrideDto {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        serde_wasm_bindgen::to_value(&rendered)
            .map_err(|e| JsError::new(&format!("failed to serialize var overrides: {e}")))
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
        self.font_context = None;
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
    /// property, at any statement or container depth, in declaration order and
    /// deduplicated. Dynamic assignments (`icon.url = expr`) are not listed.
    pub fn list_asset_urls(&mut self, source: &str) -> Result<JsValue, JsError> {
        let urls = crate::asset_urls::collect_asset_urls_from_source(source);
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
        let font_context = if let Some(context) = &self.font_context {
            context.clone()
        } else {
            let mut context = FontContext::new();
            context.load_fonts(self.fonts.iter().cloned());
            self.font_context = Some(context.clone());
            context
        };
        let modules: Vec<(std::path::PathBuf, String)> = self
            .modules
            .iter()
            .map(|(path, text)| (std::path::PathBuf::from(path), text.clone()))
            .collect();
        let built = host::build_document_with_vars(
            source,
            &modules,
            Arc::new(font_context),
            self.quality,
            assets,
            &self.vars,
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
}

/// Render a JS object of `name -> number|string|boolean` into the engine's
/// `(name, expression text)` pairs. `null`/`undefined` is the empty set; any
/// other non-object is a caller error rather than a value to drop.
fn js_var_defines(vars: &JsValue) -> Result<Vec<(String, String)>, JsError> {
    if vars.is_null() || vars.is_undefined() {
        return Ok(Vec::new());
    }
    let Some(object) = vars.dyn_ref::<js_sys::Object>() else {
        return Err(JsError::new(
            "set_vars expects an object of name -> number|string|boolean (or null to clear)",
        ));
    };
    let mut defines = Vec::new();
    for entry in js_sys::Object::entries(object).iter() {
        // `Object.entries` yields `[key, value]` pairs; anything else here is a
        // host exotic object, and an override we cannot name is one to skip.
        let Some(pair) = entry.dyn_ref::<js_sys::Array>() else {
            tracing::warn!("set_vars: skipping a non-pair entry from Object.entries");
            continue;
        };
        let value = pair.get(1);
        let Some(name) = pair.get(0).as_string() else {
            tracing::warn!("set_vars: skipping an entry whose key is not a string");
            continue;
        };
        if let Some(define) = var_define(&name, &value) {
            defines.push(define);
        }
    }
    Ok(defines)
}

/// One `(name, expression text)` pair, or `None` — logged — when the JS value
/// has no faithful Animatix literal form.
///
/// The *name* is passed through untouched: what a variable may be called is the
/// engine parser's rule, and duplicating it here would be a second parser that
/// can drift. Rendering the *value* is this boundary's job, because the page
/// sent a scalar and the engine reads source text.
fn var_define(name: &str, value: &JsValue) -> Option<(String, String)> {
    let text = if let Some(number) = value.as_f64() {
        match host::vars::number_expr(number) {
            Some(text) => text,
            None => {
                tracing::warn!(
                    "var '{name}': {number} has no Animatix number literal; override dropped"
                );
                return None;
            },
        }
    } else if let Some(text) = value.as_string() {
        match host::vars::string_expr(&text) {
            Some(literal) => literal,
            None => {
                tracing::warn!(
                    "var '{name}': {text:?} cannot be written as an Animatix string literal \
                     (a control character, a backslash, or both quote kinds); override dropped"
                );
                return None;
            },
        }
    } else if let Some(flag) = value.as_bool() {
        // `true`/`false` are the DSL's own boolean literals.
        flag.to_string()
    } else {
        tracing::warn!(
            "var '{name}': {} cannot override a build-time variable; expected a number, string \
             or boolean, so the override was dropped",
            js_kind(value)
        );
        return None;
    };
    Some((name.to_string(), text))
}

/// A readable JS type name for the dropped-value warning above. Deliberately
/// not `JSON::stringify`: that throws on a circular value, and the warning path
/// must be the one thing here that cannot fail.
fn js_kind(value: &JsValue) -> &'static str {
    if value.is_null() {
        "`null`"
    } else if value.is_undefined() {
        "`undefined`"
    } else if value.is_function() {
        "a function"
    } else if value.is_symbol() {
        "a symbol"
    } else if value.is_bigint() {
        "a BigInt"
    } else if value.is_object() {
        "an object or array"
    } else {
        "a value of an unsupported type"
    }
}
