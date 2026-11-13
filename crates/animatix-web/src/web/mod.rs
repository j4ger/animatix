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
mod text_probe;

pub use api::{build_id, engine_commit, engine_version};
pub use capabilities::{activate_capabilities, encode_rgba_png, runtime_build_id};
pub use engine::{create_player, debug_gpu_drain, init_engine};
pub use prepared::{WasmPreparedProgram, prepare_source, validate_overlay_font};
pub use resource_probe::debug_resource_stats;
pub use text_probe::debug_text_probe;

use std::cell::RefCell;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::{BuildQuality, SceneDimensions};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Process-wide WebGPU context: adapter, device, queue, and the one vello
/// renderer. Created once by [`engine::ensure_engine`]; every [`AmxPlayer`] shares it.
struct EngineContext {
    /// The adapter/device pairing lives here for the process lifetime —
    /// dropping the instance or adapter can invalidate derived state.
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Vello's `render_to_texture` encodes through `&mut self`. Frames are
    /// ticked sequentially on the single wasm thread, so a `RefCell` is
    /// enough; a second player rendering concurrently is impossible.
    core: RefCell<RendererCore>,
}

/// Player + editor backend bound to one canvas; shares the process-wide
/// [`EngineContext`] with every other player on the page.
#[wasm_bindgen]
pub struct AmxPlayer {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// Filter backend for the frame's primary target. During a multi-scene
    /// transition the incoming scene gets [`Self::filter_backend_to`] so the
    /// two scene renders cannot stomp each other's intermediate textures.
    filter_backend: Option<GpuFilterBackend>,
    filter_backend_to: Option<GpuFilterBackend>,
    /// Offscreen vello target at scene resolution; during a transition this
    /// holds the outgoing scene and [`Self::offscreen_to`] the incoming one.
    offscreen: Option<OffscreenTarget>,
    offscreen_to: Option<OffscreenTarget>,
    /// The transition compositor's blended output; needs `TEXTURE_BINDING`.
    composite: Option<OffscreenTarget>,
    /// Fonts registered through [`AmxPlayer::add_font`], applied on next load.
    fonts: Vec<Vec<u8>>,
    /// Cached context derived from `fonts`; invalidated by `add_font`.
    font_context: Option<FontContext>,
    /// Imported `.amx` modules registered through [`AmxPlayer::add_module`],
    /// joined into the build's source map (keyed by resolved import path, the
    /// keys [`crate::dto::LoadResultDto::missing_imports`] reports).
    modules: Vec<(String, String)>,
    /// Build-time variable overrides for every later load — the embed's
    /// `data-vars` / `setVars`. See [`AmxPlayer::set_vars`] for the rules.
    vars: Vec<(String, String)>,
    /// Build quality for the next [`AmxPlayer::load_source`] (the embed's
    /// `quality` attribute). Draft matches the GUI's editing preview;
    /// Production matches what a desktop export renders. Quality is baked in
    /// at build time (plot sampling tolerance), so a change means a rebuild.
    quality: BuildQuality,
    /// Lazily-built GPU transition blender (compiles the WGSL blend
    /// pipeline). Per player rather than per context so the lazy init has a
    /// `&mut` to write into next to the targets it serves.
    compositor: Option<TransitionCompositor>,
    /// Scene-space dimensions of the loaded document (canvas pixels follow the
    /// aspect ratio at whatever scale the shell picks).
    dims: SceneDimensions,
    duration_s: f64,
    target: Option<BuildTarget>,
    /// Raster scale for the offscreen targets, `0.25..=2.0` (see
    /// [`AmxPlayer::set_render_scale`]). The canvas blit scales whatever the
    /// offscreen holds up to the surface, so a scale below 1 trades detail for
    /// fill rate without touching layout.
    render_scale: f32,
    /// Reusable re-encode buffer for a scaled raster, so a scaled frame does
    /// not allocate a scene per render.
    scaled: vello::Scene,
    /// Guards against re-rasterizing a frame the canvas already shows; the
    /// raster, unlike evaluation, is the expensive part of a tick.
    dedup: animatix::timeline::frame_signature::FrameDedup,
    /// Bumped on every document build: the timeline's own epoch cannot tell a
    /// rebuilt scene from an untouched one (both start at 0).
    document_generation: u64,
    /// Diagnostics only: how many ticks the dedup absorbed and how many it drew,
    /// so `debug_dedup_stats` can prove the skip is happening rather than
    /// assuming it.
    frames_drawn: u64,
    /// See [`AmxPlayer::frames_drawn`].
    frames_deduped: u64,
}

/// Smallest raster scale [`AmxPlayer::set_render_scale`] accepts. Below this a
/// 1280-wide scene rasterizes at 320 px, which is already past the point where
/// text is legible.
pub const MIN_RENDER_SCALE: f32 = 0.25;

/// Largest raster scale [`AmxPlayer::set_render_scale`] accepts (supports up to
/// 2x Retina rendering for crisp vector display).
pub const MAX_RENDER_SCALE: f32 = 2.0;

/// One offscreen render target, kept as texture+view so the view never
/// outlives the texture.
#[derive(Clone)]
struct OffscreenTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// The rendered frame, ready to present or read back.
struct FrameTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
