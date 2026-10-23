//! wasm-bindgen entry points: WebGPU canvas presentation for built documents.
//!
//! Mirrors the GUI's `PreviewSurface` shape (evaluate → vello scene →
//! `RendererCore` offscreen, then blit to the presented view). All player
//! instances share one process-wide WebGPU context ([`EngineContext`]) so a
//! page with many `<amx-player>` embeds pays for adapter/device/renderer
//! initialization exactly once.

mod api;
mod assets;
mod debug;
mod engine;
mod frame;
mod playback;
mod readback;

pub use api::{build_id, engine_commit, engine_version};
pub use engine::{create_player, debug_gpu_drain, init_engine};

use std::cell::RefCell;

use animatix::composition::BuildTarget;
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

thread_local! {
    /// wasm runs single-threaded, so a `thread_local` slot is the safe home
    /// for the non-`Sync` vello renderer; every player method reaches the
    /// context through [`with_engine`].
    static CONTEXT: RefCell<Option<EngineContext>> = const { RefCell::new(None) };
}

/// Run `f` with the shared context (or the reason it is missing).
fn with_engine<R>(f: impl FnOnce(Result<&EngineContext, String>) -> R) -> R {
    CONTEXT.with(|cell| {
        let ctx = cell.borrow();
        f(ctx.as_ref().ok_or_else(|| "engine not initialized".to_string()))
    })
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
    /// Offscreen vello target at scene resolution (scene is rendered here,
    /// then blitted to the canvas surface). During a transition this holds the
    /// *outgoing* scene, [`Self::offscreen_to`] the incoming one.
    offscreen: Option<OffscreenTarget>,
    offscreen_to: Option<OffscreenTarget>,
    /// The transition compositor's blended output; the blit source for that
    /// frame. Needs `TEXTURE_BINDING` for exactly that reason.
    composite: Option<OffscreenTarget>,
    /// Fonts registered through [`AmxPlayer::add_font`], applied to the text
    /// compiler the next time [`AmxPlayer::load_source`] builds the scene.
    fonts: Vec<Vec<u8>>,
    /// Imported `.amx` modules registered through [`AmxPlayer::add_module`],
    /// joined into the build's source map (keyed by resolved import path, the
    /// keys [`crate::dto::LoadResultDto::missing_imports`] reports).
    modules: Vec<(String, String)>,
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
    /// Reusable re-encode buffer for [`frame::scaled_scene`]; owned here so a scaled
    /// frame does not allocate a scene per render.
    scaled: vello::Scene,
    /// Guards against re-rasterizing a frame the canvas already shows. A looping
    /// embed spends its whole `hold` window drawing the same finished frame —
    /// 13% of ticks on the tour, and tens of milliseconds each for a filtered
    /// figure — and the raster, unlike evaluation, is the expensive part.
    dedup: animatix::timeline::frame_signature::FrameDedup,
    /// Bumped on every document build. The timeline's own epoch cannot tell a
    /// rebuilt scene from an untouched one (both start at 0), and the editor path
    /// rebuilds at the same playhead time.
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

/// Raster target size for `scale`, rounded to whole pixels and clamped to
/// `MIN_RENDER_SCALE..=MAX_RENDER_SCALE`.
fn raster_dims(dims: SceneDimensions, scale: f32) -> (u32, u32) {
    let s = scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    let w = ((dims.width as f32 * s).round() as u32).max(1);
    let h = ((dims.height as f32 * s).round() as u32).max(1);
    (w, h)
}

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
