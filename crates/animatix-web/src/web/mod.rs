//! wasm-bindgen entry points: WebGPU canvas presentation for built documents.
//!
//! Mirrors the GUI's `PreviewSurface` shape (evaluate → vello scene →
//! `RendererCore` offscreen, then blit to the presented view). All player
//! instances share one process-wide WebGPU context ([`EngineContext`]) so a
//! page with many `<amx-player>` embeds pays for adapter/device/renderer
//! initialization exactly once.

mod api;
mod assets;
mod capabilities;
mod context;
mod debug;
mod engine;
mod frame;
mod playback;
mod prepared;
mod readback;
mod resource_probe;
mod state;
mod text_probe;
#[cfg(feature = "vello-gpu")]
mod webgl;

pub use api::{build_id, engine_commit, engine_version};
pub use capabilities::{activate_capabilities, encode_rgba_png, runtime_build_id};
pub use engine::{create_player, create_player_for_backend, debug_gpu_drain, init_engine};
pub use prepared::{WasmPreparedProgram, prepare_source, validate_overlay_font};
pub use resource_probe::debug_resource_stats;
pub use state::AmxPlayer;
use state::{EngineContext, FrameTarget, OffscreenTarget};
pub use text_probe::debug_text_probe;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl AmxPlayer {
    /// Backend name for the player bound to this canvas.
    pub fn backend(&self) -> String {
        #[cfg(feature = "vello-gpu")]
        if self.webgl.is_some() {
            return "webgl2".to_string();
        }
        "webgpu".to_string()
    }

    /// Shared WebGPU device generation captured by this player.
    pub fn backend_generation(&self) -> u64 {
        self.device_generation
    }
}

/// Smallest raster scale [`AmxPlayer::set_render_scale`] accepts. Below this a
/// 1280-wide scene rasterizes at 320 px, which is already past the point where
/// text is legible.
pub const MIN_RENDER_SCALE: f32 = 0.25;

/// Largest raster scale [`AmxPlayer::set_render_scale`] accepts (supports up to
/// 2x Retina rendering for crisp vector display).
pub const MAX_RENDER_SCALE: f32 = 2.0;
