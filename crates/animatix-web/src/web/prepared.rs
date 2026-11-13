//! Immutable resource preparation followed by transactional document installation.

use std::{cell::RefCell, collections::HashMap, path::PathBuf, sync::Arc};

use animatix::renderer::text::FontContext;
use animatix::timeline::{BuildQuality, assets::AssetCache};
use js_sys::Uint8Array;
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

use super::AmxPlayer;
use crate::host;

thread_local! {
    // Contexts are immutable once registered. A bounded pool shares parsing and
    // FontSetIds across players, while custom font sets remain distinct.
    static FONT_CONTEXTS: RefCell<HashMap<[u8; 32], Arc<FontContext>>> = RefCell::new(HashMap::new());
}

/// Parse a custom font in an isolated database before JS shares its bytes.
#[wasm_bindgen]
pub fn validate_overlay_font(bytes: Vec<u8>) -> Result<(), JsError> {
    animatix::renderer::text::validate_supported_font_data(&bytes)
        .map_err(|error| JsError::new(error))
}

/// Snapshot the context pool without retaining any context references.
pub(super) fn font_context_pool_stats() -> (usize, usize) {
    FONT_CONTEXTS.with(|contexts| {
        let contexts = contexts.borrow();
        (
            contexts.len(),
            contexts.values().filter(|context| Arc::strong_count(context) > 1).count(),
        )
    })
}

#[wasm_bindgen]
pub struct WasmPreparedProgram {
    program: host::PreparedProgram,
    transcripts: HashMap<String, [u8; 32]>,
}

#[wasm_bindgen]
pub fn prepare_source(
    source: &str,
    modules: JsValue,
    vars: &JsValue,
    source_url: &str,
) -> Result<WasmPreparedProgram, JsError> {
    let modules: HashMap<String, String> = serde_wasm_bindgen::from_value(modules)
        .map_err(|error| JsError::new(&format!("module sources: {error}")))?;
    let mut modules: Vec<_> = modules
        .into_iter()
        .map(|(path, source)| (PathBuf::from(path), source))
        .collect();
    modules.sort_by(|a, b| a.0.cmp(&b.0));
    let vars = super::assets::js_var_defines(vars)?;
    Ok(WasmPreparedProgram {
        program: host::prepare_document(source, &modules, &vars, Some(source_url)),
        transcripts: HashMap::new(),
    })
}

#[wasm_bindgen]
impl WasmPreparedProgram {
    pub fn analysis(&self) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.program.analysis)
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Validate fetched text resources and complete font requirements before any
    /// fonts or timeline are built. A failed enrichment leaves the snapshot intact.
    pub fn prepare_transcripts(
        &mut self,
        asset_ids: Vec<JsValue>,
        asset_texts: Vec<JsValue>,
    ) -> Result<(), JsError> {
        if asset_ids.len() != asset_texts.len() {
            return Err(JsError::new("transcript id/text lengths differ"));
        }
        let sources = asset_ids
            .into_iter()
            .zip(asset_texts)
            .map(|(id, text)| {
                let id =
                    id.as_string().ok_or_else(|| JsError::new("transcript id must be a string"))?;
                let text = text
                    .as_string()
                    .ok_or_else(|| JsError::new("transcript payload must be text"))?;
                Ok((id, text))
            })
            .collect::<Result<Vec<_>, JsError>>()?;
        let prepared = self
            .program
            .with_transcript_sources(&sources)
            .map_err(|error| JsError::new(&error))?;
        let transcripts = sources
            .iter()
            .map(|(id, text)| (id.clone(), Sha256::digest(text.as_bytes()).into()))
            .collect();
        self.program = prepared;
        self.transcripts = transcripts;
        Ok(())
    }

    /// Decode supplied resources and build exactly this snapshot. Failure leaves
    /// the player, its previous document, and all previous inputs untouched.
    pub fn build(
        &self,
        player: &mut AmxPlayer,
        asset_ids: Vec<JsValue>,
        asset_payloads: Vec<JsValue>,
        fonts: Vec<JsValue>,
        quality: &str,
    ) -> Result<JsValue, JsError> {
        let quality = match quality {
            "draft" => BuildQuality::Draft,
            "preview" => BuildQuality::Preview,
            "production" => BuildQuality::Production,
            _ => return Err(JsError::new("quality must be draft, preview or production")),
        };
        if asset_ids.len() != asset_payloads.len() {
            return Err(JsError::new("asset id/payload lengths differ"));
        }
        let mut cache = AssetCache::new();
        let mut supplied = std::collections::HashSet::new();
        for (id, payload) in asset_ids.into_iter().zip(asset_payloads) {
            let id = id.as_string().ok_or_else(|| JsError::new("asset id must be a string"))?;
            if !supplied.insert(id.clone()) {
                return Err(JsError::new("duplicate prepared asset id"));
            }
            let requirement =
                self.program.analysis.assets.iter().find(|asset| asset.id == id).ok_or_else(
                    || JsError::new(&format!("asset is outside this prepared snapshot: {id}")),
                )?;
            match requirement.kind.as_str() {
                "svg" => insert_svg(&mut cache, &id, &payload)?,
                "image" => insert_image(&mut cache, &id, &payload)?,
                "text" => {
                    let text = payload
                        .as_string()
                        .ok_or_else(|| JsError::new("transcript payload must be text"))?;
                    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
                    if self.transcripts.get(&id) != Some(&digest) {
                        return Err(JsError::new(
                            "transcript differs from the font-prepared snapshot",
                        ));
                    }
                    cache.insert_transcript_text(&id, text);
                },
                _ => return Err(JsError::new("unknown resource kind")),
            }
        }
        if self.program.analysis.assets.iter().any(|asset| !supplied.contains(&asset.id)) {
            return Err(JsError::new("prepared resource is missing from the build snapshot"));
        }
        let context = font_context(fonts)?;
        let built = host::build_prepared(&self.program, context, quality, Some(Arc::new(cache)));
        player.install_built(built)
    }
}

fn font_context(fonts: Vec<JsValue>) -> Result<Arc<FontContext>, JsError> {
    let mut hasher = Sha256::new();
    let mut bytes = Vec::with_capacity(fonts.len());
    for font in fonts {
        let font = font
            .dyn_ref::<Uint8Array>()
            .ok_or_else(|| JsError::new("font must be Uint8Array"))?
            .to_vec();
        hasher.update((font.len() as u64).to_le_bytes());
        hasher.update(&font);
        bytes.push(font);
    }
    let id: [u8; 32] = hasher.finalize().into();
    FONT_CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        if let Some(context) = contexts.get(&id) {
            return Ok(context.clone());
        }
        let mut context = FontContext::new();
        context.load_fonts(bytes);
        let context = Arc::new(context);
        if contexts.len() >= 32 {
            contexts.retain(|_, value| Arc::strong_count(value) > 1);
        }
        // Active documents own their contexts; the pool never needs to retain
        // an additional unbounded set of inactive custom overlays.
        if contexts.len() < 32 {
            contexts.insert(id, context.clone());
        }
        Ok(context)
    })
}

fn insert_svg(cache: &mut AssetCache, id: &str, payload: &JsValue) -> Result<(), JsError> {
    #[cfg(feature = "svg")]
    {
        let text = payload.as_string().ok_or_else(|| JsError::new("SVG payload must be text"))?;
        cache
            .insert_svg_source(id, &text)
            .map_err(|error| JsError::new(&format!("{id}: {error}")))
    }
    #[cfg(not(feature = "svg"))]
    {
        let _ = (cache, id, payload);
        Err(JsError::new("SVG capability is unavailable"))
    }
}

fn insert_image(cache: &mut AssetCache, id: &str, payload: &JsValue) -> Result<(), JsError> {
    #[cfg(feature = "image-decode")]
    {
        let bytes = payload
            .dyn_ref::<Uint8Array>()
            .ok_or_else(|| JsError::new("image payload must be Uint8Array"))?;
        cache
            .insert_image_bytes(id, &bytes.to_vec())
            .map_err(|error| JsError::new(&format!("{id}: {error}")))
    }
    #[cfg(not(feature = "image-decode"))]
    {
        let _ = (cache, id, payload);
        Err(JsError::new("image capability is unavailable"))
    }
}
