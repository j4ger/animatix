//! Cold code is activated before synchronous preparation/build/render calls.

use animatix_core::features::FeatureMask;
use wasm_bindgen::prelude::*;

/// Content identity supplied by the SDK build, independent of git dirtiness.
#[wasm_bindgen]
pub fn runtime_build_id() -> String {
    option_env!("ANIMATIX_RUNTIME_BUILD_ID").unwrap_or("development").to_string()
}

/// Named graph root required by the pinned relocation splitter.
#[unsafe(export_name = "main")]
pub extern "C" fn runtime_bootstrap(nonce: u32) -> u32 {
    crate::host::BUILD_ID ^ nonce.rotate_left(7)
}

/// Called by the generated loader only after all requested chunks instantiated.
#[wasm_bindgen]
pub async fn activate_capabilities(required: u32) -> Result<(), JsError> {
    let rich = FeatureMask::RICH_TEXT.bits();
    let media = (FeatureMask::IMAGE_DECODE | FeatureMask::SVG).bits();
    let encode = FeatureMask::RASTER_ENCODE.bits();
    if required & !(rich | media | encode) != 0 {
        return Err(JsError::new("unsupported capability bits"));
    }
    if required & rich != 0 {
        activate_rich().await.map_err(|error| JsError::new(&error))?;
    }
    if required & media != 0 {
        animatix::timeline::media_capability::activate()
            .await
            .map_err(|error| JsError::new(&error))?;
    }
    if required & encode != 0 {
        activate_encode().await.map_err(|error| JsError::new(&error))?;
    }
    Ok(())
}

async fn activate_rich() -> Result<(), String> {
    #[cfg(feature = "rich-text")]
    {
        animatix::renderer::text::activate_rich_capability().await
    }
    #[cfg(not(feature = "rich-text"))]
    {
        Err("rich text capability is not compiled in".to_string())
    }
}

async fn activate_encode() -> Result<(), String> {
    #[cfg(feature = "raster-encode")]
    {
        animatix_render::encode::raster::activate().await
    }
    #[cfg(not(feature = "raster-encode"))]
    {
        Err("raster encoding capability is not compiled in".to_string())
    }
}

/// Encode a readback without pulling an encoder into ordinary playback.
#[wasm_bindgen]
pub fn encode_rgba_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsError> {
    #[cfg(feature = "raster-encode")]
    {
        animatix_render::encode::raster::encode_png(rgba, width, height)
            .map_err(|error| JsError::new(&error))
    }
    #[cfg(not(feature = "raster-encode"))]
    {
        let _ = (rgba, width, height);
        Err(JsError::new("raster encoding capability is unavailable"))
    }
}
