//! wasm-bindgen entry points: WebGPU canvas presentation for built documents.
//!
//! Mirrors the GUI's `PreviewSurface` shape (evaluate → vello scene →
//! `RendererCore` offscreen, then blit to the presented view). All player
//! instances share one process-wide WebGPU context ([`EngineContext`]) so a
//! page with many `<amx-player>` embeds pays for adapter/device/renderer
//! initialization exactly once.

use std::cell::RefCell;
use std::sync::Arc;

use wasm_bindgen::prelude::*;

use web_sys::HtmlCanvasElement;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::assets::AssetCache;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::frame_signature::FrameSignature;
use animatix::timeline::{BuildQuality, DebugRenderOptions, SceneDimensions, Timeline};
use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::parse_source;

use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::offscreen::drain_pending_layers;
use animatix_render::transition::TransitionCompositor;

use crate::host;
use crate::host::BuiltDocument;
use wgpu::CurrentSurfaceTexture;

/// Process-wide WebGPU context: adapter, device, queue, and the one vello
/// renderer. Created once by [`ensure_engine`]; every [`AmxPlayer`] shares it.
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

/// Create the shared context if it does not exist yet. Idempotent; the JS
/// shell additionally funnels concurrent first calls through one promise.
async fn ensure_engine() -> Result<(), String> {
    let already = CONTEXT.with(|cell| cell.borrow().is_some());
    if already {
        return Ok(());
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("no WebGPU adapter available: {e}"))?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("animatix-web device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("failed to request WebGPU device: {e}"))?;

    // Surface wgpu's internal validation errors on the JS console — the
    // default handler logs through `log`, which has no subscriber here.
    device.on_uncaptured_error(Arc::new(move |error| {
        web_sys::console::error_1(&format!("wgpu uncaptured error: {error}").into());
    }));
    device.set_device_lost_callback(|reason, message| {
        web_sys::console::error_1(&format!("wgpu device lost: {reason:?} — {message}").into());
        // Drop the dead context. Leaving it in the slot means every later call
        // from every player in the page reaches the same lost device and fails
        // the same way, so one loss permanently breaks the tab; clearing it lets
        // the next `ensure_engine` negotiate a fresh adapter and lets a
        // re-created `<amx-player>` animate again.
        //
        // Known limit: players that already exist still hold targets and
        // sessions built on the dead device, so they keep failing until they are
        // re-created. Rebuilding their state in place needs a generation stamp
        // on `EngineContext` that each player can compare its own against —
        // tracked in `docs/roadmap.md`.
        CONTEXT.with(|cell| {
            *cell.borrow_mut() = None;
        });
    });

    let core =
        RendererCore::new(&device, &queue).map_err(|e| format!("renderer init failed: {e}"))?;

    CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(EngineContext {
            _instance: instance,
            _adapter: adapter,
            device,
            queue,
            core: RefCell::new(core),
        });
    });
    Ok(())
}

/// Identifies the running build from the JS side (stale-artifact checks).
#[wasm_bindgen]
pub fn build_id() -> u32 {
    54
}

/// Resolve once every command submitted so far has finished on the GPU.
///
/// The measurement half of [`AmxPlayer::debug_bench_frames`]: `queue.submit`
/// returns as soon as the work is *encoded*, so without this drain a frame
/// timer sees only the CPU's ~0.5 ms and never the GPU's few milliseconds.
/// Cheaper than [`AmxPlayer::debug_readback`] — it waits on submitted work
/// instead of copying a frame back to the CPU.
#[wasm_bindgen]
pub async fn debug_gpu_drain() -> Result<(), JsError> {
    await_gpu_drain().await.map_err(|e| JsError::new(&e))
}

/// Initialize the shared WebGPU context (adapter, device, vello renderer).
///
/// Idempotent and cheap after the first call; `create_player` also runs it,
/// so shells that skip this still work — calling it explicitly just lets a
/// page funnel N embeds' startup through one awaited promise.
#[wasm_bindgen]
pub async fn init_engine() -> Result<(), JsError> {
    console_error_panic_hook::set_once();
    ensure_engine().await.map_err(|e| JsError::new(&e))
}

/// Create a player bound to `canvas`, sharing the process-wide engine
/// context. Rejects with a readable message when WebGPU is unavailable — the
/// shell is expected to have feature-detected `navigator.gpu` first.
#[wasm_bindgen]
pub async fn create_player(canvas: HtmlCanvasElement) -> Result<AmxPlayer, JsError> {
    console_error_panic_hook::set_once();
    ensure_engine().await.map_err(|e| JsError::new(&e))?;

    with_engine(|ctx| {
        let ctx = ctx.map_err(|e| JsError::new(&e))?;

        let surface = ctx
            ._instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| JsError::new(&format!("failed to create canvas surface: {e}")))?;

        let capabilities = surface.get_capabilities(&ctx._adapter);
        // Prefer Rgba8Unorm — the format every internal target and the
        // default blit pipeline already use — when the surface supports it;
        // otherwise take the backend's first (and compile a matching blit
        // variant on first present).
        let format = capabilities
            .formats
            .iter()
            .find(|f| **f == wgpu::TextureFormat::Rgba8Unorm)
            .or_else(|| capabilities.formats.first())
            .copied()
            .ok_or_else(|| JsError::new("surface reports no supported formats"))?;
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::CompositeAlphaMode::Auto)
            .unwrap_or_else(|| {
                capabilities
                    .alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto)
            });

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: canvas.width().max(1),
            height: canvas.height().max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&ctx.device, &config);

        Ok(AmxPlayer {
            canvas,
            surface,
            config,
            filter_backend: None,
            filter_backend_to: None,
            compositor: None,
            fonts: Vec::new(),
            modules: Vec::new(),
            quality: BuildQuality::Draft,
            offscreen: None,
            offscreen_to: None,
            composite: None,
            dims: SceneDimensions::default(),
            duration_s: 0.1,
            target: None,
            render_scale: 1.0,
            scaled: vello::Scene::new(),
            dedup: animatix::timeline::frame_signature::FrameDedup::new(),
            document_generation: 0,
            frames_drawn: 0,
            frames_deduped: 0,
        })
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
    /// Raster scale for the offscreen targets, `0.25..=1.0` (see
    /// [`AmxPlayer::set_render_scale`]). The canvas blit scales whatever the
    /// offscreen holds up to the surface, so a scale below 1 trades detail for
    /// fill rate without touching layout.
    render_scale: f32,
    /// Reusable re-encode buffer for [`scaled_scene`]; owned here so a scaled
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

/// Raster target size for `scale`, rounded to whole pixels and clamped to the
/// scene's own resolution — upscaling beyond it spends pixels without adding
/// detail.
fn raster_dims(dims: SceneDimensions, scale: f32) -> (u32, u32) {
    let s = scale.clamp(MIN_RENDER_SCALE, 1.0);
    let w = ((dims.width as f32 * s).round() as u32).max(1);
    let h = ((dims.height as f32 * s).round() as u32).max(1);
    (w, h)
}

/// The scene to rasterize at `scale`.
///
/// Vello renders a scene 1:1 into its target — `RenderParams` carries no
/// transform in the pinned revision — so rasterizing below the scene's own
/// resolution means re-encoding the scene with an affine scale. `append` is
/// the only place that transform can be applied, and it costs one O(paths)
/// copy against a per-frame fill-rate cost that is several times larger (see
/// `docs/performance_evaluation.md`, "Web playback").
fn scaled_scene<'s>(
    scratch: &'s mut vello::Scene,
    scene: &'s vello::Scene,
    scale: f32,
) -> &'s vello::Scene {
    let s = scale.clamp(MIN_RENDER_SCALE, 1.0);
    if (s - 1.0).abs() < 1e-3 {
        return scene;
    }
    scratch.reset();
    scratch.append(scene, Some(vello::kurbo::Affine::scale(s as f64)));
    scratch
}

/// Monotonic milliseconds, the only clock wasm32-unknown-unknown has.
///
/// `std::time::Instant` is *not* implemented for this target — calling
/// `Instant::now()` traps with "time not implemented on this platform" — so the
/// frame timers read `performance.now()` through `web-sys`.
fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

/// Resolve once every command submitted so far has completed on the GPU.
///
/// `on_submitted_work_done` is the cheap drain — unlike [`AmxPlayer::debug_readback`]
/// it copies no pixels, so the benchmark can separate "how long the CPU spent
/// encoding" from "how long the GPU took to finish".
async fn await_gpu_drain() -> Result<(), String> {
    let promise = with_engine(|ctx| -> Result<js_sys::Promise, String> {
        let ctx = ctx?;
        Ok(js_sys::Promise::new(
            &mut |resolve: js_sys::Function, _reject: js_sys::Function| {
                ctx.queue.on_submitted_work_done(move || {
                    let _ = resolve.call0(&JsValue::NULL);
                });
            },
        ))
    })?;
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("gpu drain failed: {e:?}"))?;
    Ok(())
}

/// Everything the map_async callback needs to own: the mapped buffer plus
/// the geometry to interpret its bytes.
struct ReadbackSetup {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
}

/// One asset-cache entry of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgCacheEntry {
    url: String,
    paths: usize,
}

/// One track row of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgTrackEntry {
    label: String,
    paths: usize,
    /// Paths the track evaluates to at the probe time.
    paths_at_t: Option<usize>,
    /// The track's authored opacity at the probe time.
    opacity_at_t: f32,
}

/// The payload of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgStats {
    cache: Vec<SvgCacheEntry>,
    tracks: Vec<SvgTrackEntry>,
    vello: VelloStats,
}

/// Draw/path counts of the probed frame's vello encoding.
#[derive(serde::Serialize)]
struct VelloStats {
    draws: usize,
    paths: u32,
}

fn readback_impl(player: &mut AmxPlayer, time_s: f64) -> Result<ReadbackSetup, String> {
    let dims = player.dims;
    if player.target.is_none() {
        return Err("no document loaded".to_string());
    }
    with_engine(|ctx| readback_with(player, ctx, time_s, dims))
}

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

    /// Whether a document is currently loaded and renderable.
    pub fn has_document(&self) -> bool {
        self.target.is_some()
    }

    pub fn duration_s(&self) -> f64 {
        self.duration_s
    }

    /// Raster scale for the offscreen render targets, clamped to
    /// `MIN_RENDER_SCALE..=1.0`; returns the value in effect.
    ///
    /// A frame's GPU cost tracks the *raster* pixel count, not the scene's
    /// content — an empty 1280×720 scene and a full one measure the same — so
    /// this is the one knob that moves browser frame time materially. Layout is
    /// unaffected: the timeline is still evaluated against the scene's own
    /// dimensions, and only the rasterization is scaled (the canvas blit scales
    /// it back up). The shell's job is to pick a scale that fits the element's
    /// displayed pixels; `debug_bench` reports what a scale costs.
    pub fn set_render_scale(&mut self, scale: f64) -> f64 {
        let scale = if scale.is_finite() { scale as f32 } else { 1.0 };
        self.render_scale = scale.clamp(MIN_RENDER_SCALE, 1.0);
        self.render_scale as f64
    }

    pub fn render_scale(&self) -> f64 {
        self.render_scale as f64
    }

    /// The offscreen raster size the next frame will render at, in pixels.
    pub fn raster_width(&self) -> u32 {
        raster_dims(self.dims, self.render_scale).0
    }

    pub fn raster_height(&self) -> u32 {
        raster_dims(self.dims, self.render_scale).1
    }

    /// Drive `frames` frames of the loaded document and report what the *CPU*
    /// side of them cost, as a JSON object:
    ///
    /// ```text
    /// { frames, scene: [w, h], raster: [w, h], scale, cpu_ms,
    ///   cpu_p50_ms, cpu_p90_ms, cpu_max_ms }
    /// ```
    ///
    /// Pair it with [`debug_gpu_drain`] to get the frame cost a display would
    /// be paced at:
    ///
    /// ```js
    /// const t0 = performance.now();
    /// const cpu = JSON.parse(player.debug_bench_frames(60, 1 / 60));
    /// await debug_gpu_drain();
    /// const perFrameMs = (performance.now() - t0) / cpu.frames;
    /// ```
    ///
    /// `cpu_ms` is time spent inside `render_frame` (evaluate + vello encode +
    /// submit + blit encode); `perFrameMs - cpu_ms` is therefore the GPU's
    /// share, and `perFrameMs` is what has to fit in a frame budget.
    ///
    /// Why a dedicated driver rather than timing `requestAnimationFrame`: a
    /// backgrounded or headless tab delivers no rAF callbacks at all, and
    /// `queue.submit` returns long before the GPU has drawn anything, so a
    /// naive per-frame timer measures encoding only (it reports ~0.5 ms for a
    /// frame whose GPU work is ~4 ms). Driving the frames explicitly against an
    /// explicit sync point is the same shape as the native
    /// `perf_driver` / `export_perf_driver` harnesses.
    ///
    /// Two warm-up frames are rendered and drained first, so pipeline
    /// compilation and atlas growth are not billed to the mean.
    pub fn debug_bench_frames(&mut self, frames: u32, step_s: f64) -> Result<JsValue, JsError> {
        let frames = frames.clamp(1, 2000);
        let step = if step_s.is_finite() && step_s > 0.0 {
            step_s
        } else {
            1.0 / 60.0
        };
        if !self.has_document() {
            return Err(JsError::new("no document loaded"));
        }

        let mut cpu = Vec::with_capacity(frames as usize);
        for i in 0..frames {
            let t0 = now_ms();
            // A benchmark that dedups measures nothing: this loop asks "what does
            // drawing a frame cost", and two iterations at the same quantised
            // time would otherwise answer "free". Clearing the remembered frame
            // is a store of `None` — no GPU work — so it cannot bias the timing.
            self.dedup.invalidate();
            self.render_frame(i as f64 * step)?;
            cpu.push(now_ms() - t0);
        }
        let cpu_ms = cpu.iter().sum::<f64>() / frames as f64;
        cpu.sort_by(f64::total_cmp);
        let pick = |p: f64| -> f64 {
            let idx = ((cpu.len() - 1) as f64 * p).round() as usize;
            cpu[idx.min(cpu.len() - 1)]
        };
        let raster = raster_dims(self.dims, self.render_scale);
        Ok(JsValue::from_str(&format!(
            "{{\"frames\":{frames},\"scene\":[{},{}],\"raster\":[{},{}],\"scale\":{},\
             \"cpu_ms\":{cpu_ms:.4},\"cpu_p50_ms\":{:.4},\"cpu_p90_ms\":{:.4},\"cpu_max_ms\":{:.4}}}",
            self.dims.width,
            self.dims.height,
            raster.0,
            raster.1,
            self.render_scale,
            pick(0.5),
            pick(0.9),
            pick(1.0),
        )))
    }

    /// Render `frames` warm-up frames and leave them submitted. The caller
    /// awaits [`debug_gpu_drain`] before starting a timed run, so a benchmark
    /// does not bill first-frame pipeline compilation to the first sample.
    pub fn debug_bench_warmup(&mut self, frames: u32) -> Result<(), JsError> {
        if !self.has_document() {
            return Ok(());
        }
        for i in 0..frames.max(1) {
            self.render_frame(i as f64 / 60.0)?;
        }
        Ok(())
    }

    /// Frame-dedup counters: how many ticks drew and how many found the same
    /// frame already on the canvas. Read-only, so a page can prove the skip is
    /// happening rather than trust it (`web/demos/perf-probe.html`). Named in
    /// snake_case like every other export here; a `js_name` rename would make
    /// the probe's optional call read as zero forever.
    pub fn debug_dedup_stats(&self) -> Result<JsValue, JsError> {
        let stats = crate::dto::DedupStatsDto {
            drawn: self.frames_drawn,
            deduped: self.frames_deduped,
        };
        serde_wasm_bindgen::to_value(&stats)
            .map_err(|e| JsError::new(&format!("failed to serialize dedup stats: {e}")))
    }

    pub fn scene_width(&self) -> u32 {
        self.dims.width
    }

    pub fn scene_height(&self) -> u32 {
        self.dims.height
    }

    /// Diagnostic: report the loaded document's SVG state — the asset cache's
    /// parsed path counts per url, and each track's path count both statically
    /// and as evaluated at `time_s`, with the actor opacity at that moment.
    /// Bisects "the SVG never parsed / seeded" vs "the geometry exists but the
    /// frame does not contain it". Sample at or after the actor's entrance —
    /// a pre-reveal sample reads as empty for a hidden-by-default actor.
    /// Returns `null` when no document is loaded.
    pub fn debug_svg_stats(&self, time_ms: f64) -> Result<JsValue, JsError> {
        let time_ms = time_ms as u64;
        let Some(target) = &self.target else {
            return serde_wasm_bindgen::to_value(&Option::<()>::None)
                .map_err(|e| JsError::new(&format!("failed to serialize: {e}")));
        };
        let timeline = match target {
            BuildTarget::SingleScene(timeline) => timeline,
            BuildTarget::MultiScene(composition) => match composition.scenes.values().next() {
                Some(scene) => &scene.timeline,
                None => {
                    return Err(JsError::new("composition has no scenes"));
                },
            },
        };
        let mut cache: Vec<SvgCacheEntry> = timeline
            .asset_cache()
            .svg_paths()
            .map(|(url, paths)| SvgCacheEntry {
                url: url.clone(),
                paths: paths.len(),
            })
            .collect();
        cache.sort_by(|a, b| a.url.cmp(&b.url));
        let mut tracks: Vec<SvgTrackEntry> = timeline
            .tracks()
            .iter()
            .map(|(label, track)| SvgTrackEntry {
                label: label.clone(),
                paths: track.svg_paths.len(),
                paths_at_t: track.svg_paths_at(time_ms).map(|paths| paths.len()),
                opacity_at_t: animatix::timeline::TrackAccessor::get(
                    &track.style.opacity,
                    time_ms,
                    1.0,
                ),
            })
            .collect();
        tracks.sort_by(|a, b| a.label.cmp(&b.label));
        // Probe stage 3: evaluate the frame like the render loop would and
        // count what actually made it into the vello encoding. A missing draw
        // here points at the engine; a present draw that renders nothing
        // points at the GPU path.
        let mut no_filters: Option<&mut dyn animatix::timeline::effects::FilterBackend> = None;
        let scene = timeline.evaluate_with_debug(
            time_ms as f64 / 1000.0,
            self.dims,
            DebugRenderOptions::default(),
            &mut no_filters,
        );
        let encoding = scene.encoding();
        let vello = VelloStats {
            draws: encoding.draw_tags.len(),
            paths: encoding.n_paths,
        };
        serde_wasm_bindgen::to_value(&SvgStats {
            cache,
            tracks,
            vello,
        })
        .map_err(|e| JsError::new(&format!("failed to serialize svg stats: {e}")))
    }

    /// Diagnostic: clear the canvas with a solid color through raw wgpu,
    /// bypassing vello entirely. Bisects "scene content" vs "present chain".
    pub fn debug_fill(&mut self, r: f32, g: f32, b: f32) -> Result<(), JsError> {
        with_engine(|ctx| self.debug_fill_inner(ctx, r, g, b))
    }

    fn debug_fill_inner(
        &mut self,
        ctx: Result<&EngineContext, String>,
        r: f32,
        g: f32,
        b: f32,
    ) -> Result<(), JsError> {
        let ctx = ctx.map_err(|e| JsError::new(&e))?;
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            },
            _ => return Ok(()),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("animatix-web debug fill"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("animatix-web debug fill pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: r as f64,
                            g: g as f64,
                            b: b as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        // The screen now holds a flat colour that no frame signature describes,
        // so the next identical frame must draw rather than dedup.
        self.dedup.invalidate();
        Ok(())
    }

    /// Diagnostic: render the current document into an offscreen texture and
    /// read the pixels back through a GPU buffer (no compositor involved).
    /// `on_result` receives a JSON string
    /// `{ avg: [r, g, b], distinct: N, samples: N, width, height }`.
    ///
    /// On the web backend the `map_async` callback is driven by the browser's
    /// microtask queue, so no blocking poll is involved.
    pub fn debug_readback(&mut self, time_s: f64, on_result: js_sys::Function) {
        let result = readback_impl(self, time_s);
        match result {
            Ok(setup) => {
                let map_target = setup.buffer.clone();
                let buffer = setup.buffer.clone();
                map_target.slice(..).map_async(wgpu::MapMode::Read, move |mapped| {
                    let payload = match mapped {
                        Ok(()) => {
                            let slice = buffer.slice(..);
                            let data = slice.get_mapped_range();
                            let summary = summarize_pixels(
                                &data,
                                setup.width,
                                setup.height,
                                setup.bytes_per_row,
                            );
                            drop(data);
                            // Dropping the mapped range does not unmap the
                            // buffer; `filter_backend.rs` gets this right and
                            // these two paths did not, so every probe left a
                            // pinned mapping behind.
                            buffer.unmap();
                            summary
                        },
                        Err(err) => format!("{{\"error\":\"map failed: {err}\"}}"),
                    };
                    let _ = on_result.call1(&JsValue::NULL, &JsValue::from_str(&payload));
                });
            },
            Err(err) => {
                let _ = on_result
                    .call1(&JsValue::NULL, &JsValue::from_str(&format!("{{\"error\":\"{err}\"}}")));
            },
        }
    }

    /// Diagnostic: the same readback as [`Self::debug_readback`], handed back as
    /// pixels rather than statistics.
    ///
    /// `debug_readback` can prove *that* a scene rendered; it cannot show what
    /// it looked like — and a headless browser screenshot cannot either, because
    /// the WebGPU canvas comes out of one blank (see `docs/roadmap.md`, "Headless
    /// cannot see the site's primary surface"). This is the byte path that gap
    /// needs: `on_result` receives `{ width, height, bytes }`, with `bytes` a
    /// `Uint8Array` of tight RGBA rows ready for an `ImageData`.
    pub fn debug_readback_rgba(&mut self, time_s: f64, on_result: js_sys::Function) {
        let result = readback_impl(self, time_s);
        match result {
            Ok(setup) => {
                let map_target = setup.buffer.clone();
                let buffer = setup.buffer.clone();
                map_target.slice(..).map_async(wgpu::MapMode::Read, move |mapped| {
                    let pixels = match mapped {
                        Ok(()) => {
                            let slice = buffer.slice(..);
                            let data = slice.get_mapped_range();
                            let rgba =
                                compact_rgba(&data, setup.width, setup.height, setup.bytes_per_row);
                            drop(data);
                            buffer.unmap();
                            Ok(rgba)
                        },
                        Err(err) => Err(format!("map failed: {err}")),
                    };
                    match pixels {
                        Ok(rgba) => {
                            let out = js_sys::Object::new();
                            let _ =
                                js_sys::Reflect::set(&out, &"width".into(), &setup.width.into());
                            let _ =
                                js_sys::Reflect::set(&out, &"height".into(), &setup.height.into());
                            let _ = js_sys::Reflect::set(
                                &out,
                                &"bytes".into(),
                                &js_sys::Uint8Array::from(rgba.as_slice()),
                            );
                            let _ = on_result.call1(&JsValue::NULL, &out);
                        },
                        Err(err) => {
                            let _ = on_result.call1(
                                &JsValue::NULL,
                                &JsValue::from_str(&format!("{{\"error\":\"{err}\"}}")),
                            );
                        },
                    }
                });
            },
            Err(err) => {
                let _ = on_result
                    .call1(&JsValue::NULL, &JsValue::from_str(&format!("{{\"error\":\"{err}\"}}")));
            },
        }
    }

    /// Render the document at `time_s` into the canvas.
    ///
    /// No-ops when nothing is loaded, so the shell can keep rAF running
    /// through edits.
    pub fn render_frame(&mut self, time_s: f64) -> Result<(), JsError> {
        if !self.has_document() {
            return Ok(());
        }
        with_engine(|ctx| self.render_frame_inner(ctx, time_s))
    }

    /// What the frame about to be drawn at `time_s` depends on, in one
    /// comparable value. See `animatix::timeline::frame_signature` for why each
    /// member belongs there.
    fn frame_signature(
        &self,
        time_s: f64,
        surface_width: u32,
        surface_height: u32,
    ) -> FrameSignature {
        let (raster_width, raster_height) = raster_dims(self.dims, self.render_scale);
        FrameSignature {
            // Quantised exactly like the engine's own scene cache: a difference
            // the evaluator cannot see must not make the dedup redraw.
            time_ms: (time_s * 1000.0) as u64,
            scene_width: self.dims.width,
            scene_height: self.dims.height,
            raster_width,
            raster_height,
            surface_width,
            surface_height,
            document_generation: self.document_generation,
            content_epoch: self.target.as_ref().map_or(0, BuildTarget::content_epoch),
            // The web player has no debug overlays; a driver that can toggle
            // them names them here instead.
            debug_bits: 0,
        }
    }

    fn render_frame_inner(
        &mut self,
        ctx: Result<&EngineContext, String>,
        time_s: f64,
    ) -> Result<(), JsError> {
        let ctx = ctx.map_err(|e| JsError::new(&e))?;
        let width = self.canvas.width().max(1);
        let height = self.canvas.height().max(1);
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&ctx.device, &self.config);
        }

        // A loop rests on its finished frame for the whole `hold` window, and the
        // embed keeps asking for it 60 times a second. Evaluation is ~47 µs of
        // that; the raster is the milliseconds — so the check is worth doing
        // before anything is acquired, and a skipped tick does no GPU work at
        // all: the canvas holds the last presented image.
        let signature = self.frame_signature(time_s, width, height);
        if self.dedup.is_presented(&signature) {
            self.frames_deduped += 1;
            return Ok(());
        }

        // Vello draws its final pass through a compute pipeline that needs
        // STORAGE_BINDING on its target. A canvas configured the default way is
        // not given that usage (measured `GPUTextureUsage` = 16,
        // RENDER_ATTACHMENT only, for rgba8unorm and bgra8unorm, opaque and
        // premultiplied, at 64x64 and 1100x619 alike) — but *asking* works in
        // Chromium: passing a non-spec `usage` member to
        // `GPUCanvasContext.configure` yields a surface texture reporting 24, and
        // asking for more yields 30. So rendering straight into the canvas is
        // possible, and we still do not: the blit it would delete measures
        // 0.003 ms (`performance_evaluation.md` §3.7), and it is not overhead —
        // it *is* the upscaling step. Drawing into the surface means rasterizing
        // at canvas resolution every frame, which forfeits `set_render_scale`
        // and the adaptive quality controller, and Firefox/Safari are unverified
        // for the `usage` member (an ignored dictionary key reads as "accepts",
        // so a probe plus a blit fallback is mandatory, most sensibly as
        // "direct only when scale == 1").
        //
        // Mirror the GUI's PreviewSurface instead: render into an offscreen
        // texture we own at scene resolution (vello draws scene units 1:1 — no
        // camera scaling), then blit it scaled onto the surface view. During a
        // multi-scene transition the outgoing and incoming scenes render into two
        // of those targets and the compositor blends them into a third.
        let blending = matches!(self.target.as_ref(), Some(BuildTarget::MultiScene(_)));
        // The transition compositor compiles its WGSL pipeline on first use;
        // the player owns it so the lazy init has a home beside its targets.
        if blending && self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&ctx.device).map_err(|e| JsError::new(&e))?);
        }

        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            },
            // The surface changed under us (e.g. canvas resize raced the
            // frame) — reconfigure and let the next rAF tick retry. A
            // reconfigure can hand us a fresh swapchain, so forget what was on
            // screen: the image the dedup is remembering may no longer exist.
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&ctx.device, &self.config);
                self.dedup.invalidate();
                return Ok(());
            },
            // Timeout/Occluded/Lost/Validation: skip this frame; the shell's
            // render loop simply tries again next tick.
            _ => return Ok(()),
        };
        let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut core = ctx.core.borrow_mut();
        let frame_target = render_document(
            &mut core,
            &mut self.filter_backend,
            &mut self.filter_backend_to,
            &ctx.device,
            &ctx.queue,
            &mut self.offscreen,
            &mut self.offscreen_to,
            &mut self.composite,
            self.target.as_ref(),
            time_s,
            self.dims,
            self.render_scale,
            &mut self.scaled,
            self.compositor.as_ref(),
        );

        // Blit the rendered frame onto the swapchain view and present. The
        // blit scales from the offscreen resolution to canvas pixels. The
        // surface's format is the browser's choice (Firefox orders Bgra8Unorm
        // first), so the blit runs through the per-format pipeline rather than
        // the internal Rgba8Unorm one.
        //
        // Presenting is inside the success branch on purpose. A swapchain image
        // acquired but never drawn into is not "the previous frame" — presenting
        // it puts undefined content on screen, which with the dedup in place
        // would turn one bad frame into a permanent blank: the failed tick
        // records nothing, so every identical tick after it is skipped and the
        // canvas never gets another present.
        if let Ok(offscreen) = &frame_target {
            core.blit_texture_to_format(
                &ctx.device,
                &ctx.queue,
                &offscreen.view,
                &surface_view,
                width,
                height,
                1.0,
                self.config.format,
            );
            drop(surface_view);
            frame.present();
            self.frames_drawn += 1;
            // Only now: this is the frame the canvas is showing, and the
            // signature is the claim that redrawing it would change nothing.
            self.dedup.record(signature);
            Ok(())
        } else {
            // Never drawn into, never presented: the canvas keeps the last frame
            // that did make it. Release the view before the texture goes.
            drop(surface_view);
            frame_target
                .map(|_| ())
                .map_err(|e| JsError::new(&format!("render failed: {e}")))
        }
    }
}

/// Render the loaded document for `time_s`, returning the target that holds
/// the finished frame.
///
/// This is the one place that decides what a frame is. A single-scene document
/// renders into `primary`; a multi-scene document renders its scenes into
/// `primary`/`secondary` and, while a transition is active, blends the two into
/// `composite` with the GPU compositor — the same sequence the GUI preview and
/// the export path run. The canvas path blits the returned view to the surface
/// and the readback path copies the returned texture to a buffer: one frame
/// definition, two consumers.
///
/// `scale` sizes the offscreen targets relative to the scene's resolution
/// ([`raster_dims`]); the timeline is always evaluated against `dims`, so
/// layout is scale-independent.
#[allow(clippy::too_many_arguments)]
fn render_document(
    core: &mut RendererCore,
    primary_backend: &mut Option<GpuFilterBackend>,
    secondary_backend: &mut Option<GpuFilterBackend>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    primary: &mut Option<OffscreenTarget>,
    secondary: &mut Option<OffscreenTarget>,
    composite: &mut Option<OffscreenTarget>,
    target: Option<&BuildTarget>,
    time_s: f64,
    dims: SceneDimensions,
    scale: f32,
    scratch: &mut vello::Scene,
    compositor: Option<&TransitionCompositor>,
) -> Result<FrameTarget, String> {
    let scene = raster_dims(dims, scale);
    let from = ensure_offscreen(primary, device, "animatix-web offscreen target", scene);

    match target {
        Some(BuildTarget::MultiScene(composition)) => {
            let (scene_name, local_time_s, blend) = composition.evaluate(time_s);
            let Some(blend) = blend else {
                let timeline = composition
                    .scenes
                    .get(&scene_name)
                    .map(|scene| &scene.timeline)
                    .ok_or_else(|| format!("composition has no scene named '{scene_name}'"))?;
                render_timeline(
                    core,
                    primary_backend,
                    device,
                    queue,
                    &from.view,
                    &from.texture,
                    timeline,
                    local_time_s,
                    dims,
                    scale,
                    scratch,
                )?;
                return Ok(from.frame_target());
            };

            // Transition: both scenes render into their own targets (each with
            // its own filter backend so the two evaluations cannot stomp each
            // other's pass state) and the compositor blends them.
            let to_local = composition
                .scene_start_times
                .get(&blend.to_scene)
                .map(|start| time_s - start)
                .unwrap_or(local_time_s);
            let from_timeline = &composition
                .scenes
                .get(&blend.from_scene)
                .ok_or_else(|| format!("composition has no scene named '{}'", blend.from_scene))?
                .timeline;
            let to_timeline = &composition
                .scenes
                .get(&blend.to_scene)
                .ok_or_else(|| format!("composition has no scene named '{}'", blend.to_scene))?
                .timeline;
            let to =
                ensure_offscreen(secondary, device, "animatix-web offscreen target (to)", scene);
            let composite_target =
                ensure_offscreen(composite, device, "animatix-web transition target", scene);
            let compositor =
                compositor.ok_or_else(|| "transition compositor missing".to_string())?;

            render_timeline(
                core,
                primary_backend,
                device,
                queue,
                &from.view,
                &from.texture,
                from_timeline,
                blend.from_local,
                dims,
                scale,
                scratch,
            )?;
            render_timeline(
                core,
                secondary_backend,
                device,
                queue,
                &to.view,
                &to.texture,
                to_timeline,
                to_local,
                dims,
                scale,
                scratch,
            )?;
            compositor.render(
                device,
                queue,
                &from.view,
                &to.view,
                &composite_target.view,
                scene.0,
                scene.1,
                blend.progress as f32,
                &blend.id,
                blend.easing,
            )?;

            Ok(composite_target.frame_target())
        },
        Some(BuildTarget::SingleScene(timeline)) => {
            render_timeline(
                core,
                primary_backend,
                device,
                queue,
                &from.view,
                &from.texture,
                timeline,
                time_s,
                dims,
                scale,
                scratch,
            )?;
            Ok(from.frame_target())
        },
        None => Err("no document loaded".to_string()),
    }
}

/// Read the current document back through an offscreen render — the diagnostic
/// twin of [`render_document`], so a readback taken mid-transition reports the
/// blended frame a viewer would see.
fn readback_with(
    player: &mut AmxPlayer,
    ctx: Result<&EngineContext, String>,
    time_s: f64,
    dims: SceneDimensions,
) -> Result<ReadbackSetup, String> {
    let ctx = ctx?;
    let bytes_per_row = (dims.width * 4 + 255) & !255;

    // Fresh targets: the readback must not disturb the player's own
    // presentation slots.
    let mut primary: Option<OffscreenTarget> = None;
    let mut secondary: Option<OffscreenTarget> = None;
    let mut composite: Option<OffscreenTarget> = None;
    if player.compositor.is_none() {
        player.compositor = Some(TransitionCompositor::new(&ctx.device)?);
    }

    let AmxPlayer { target, .. } = player;
    // The probe gets its own filter backends for the same reason it gets its own
    // targets. `render_document` drains `take_pending_composites()` off whichever
    // backend it is handed and blits the result into its target, so probing
    // through the player's backend pulled the composited effect regions off a
    // frame that was still in flight — the probe's pixels came out right and the
    // next presented frame came out missing them.
    let mut filter_backend: Option<GpuFilterBackend> = None;
    let mut filter_backend_to: Option<GpuFilterBackend> = None;
    let mut core = ctx.core.borrow_mut();
    // Readbacks always rasterize at the scene's own resolution: the pixel
    // probes compare against backdrop baselines and each other, so they must
    // not inherit whatever scale the presentation path is currently using.
    let mut scratch = vello::Scene::new();
    let frame = render_document(
        &mut core,
        &mut filter_backend,
        &mut filter_backend_to,
        &ctx.device,
        &ctx.queue,
        &mut primary,
        &mut secondary,
        &mut composite,
        target.as_ref(),
        time_s,
        dims,
        1.0,
        &mut scratch,
        player.compositor.as_ref(),
    )?;

    let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("animatix-web readback buffer"),
        size: bytes_per_row as u64 * dims.height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("animatix-web readback copy"),
    });
    encoder.copy_texture_to_buffer(
        frame.texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(dims.height),
            },
        },
        wgpu::Extent3d {
            width: dims.width,
            height: dims.height,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue.submit(std::iter::once(encoder.finish()));

    Ok(ReadbackSetup {
        buffer,
        width: dims.width,
        height: dims.height,
        bytes_per_row,
    })
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

impl OffscreenTarget {
    /// A rendered frame keyed to this target's texture and view.
    fn frame_target(&self) -> FrameTarget {
        FrameTarget {
            texture: self.texture.clone(),
            view: self.view.clone(),
        }
    }
}

/// The rendered frame, ready to present or read back.
struct FrameTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Create an offscreen target usable by vello (STORAGE_BINDING), the GPU
/// filter passes (RENDER_ATTACHMENT), texture-based compositors
/// (TEXTURE_BINDING) and pixel readback (COPY_SRC).
fn create_offscreen(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create or resize an offscreen target and hand back the whole handle.
fn ensure_offscreen(
    slot: &mut Option<OffscreenTarget>,
    device: &wgpu::Device,
    label: &str,
    scene: (u32, u32),
) -> OffscreenTarget {
    if let Some(existing) = slot {
        if (existing.width, existing.height) == scene {
            return existing.clone();
        }
    }
    let (texture, view) = create_offscreen(device, label, scene.0, scene.1);
    let target = OffscreenTarget {
        texture: texture.clone(),
        view: view.clone(),
        width: scene.0,
        height: scene.1,
    };
    *slot = Some(target.clone());
    target
}

/// Evaluate one frame of `timeline` (with the GPU filter backend so `Filter`
/// scopes render like the export path) and draw it into `view`, including the
/// scope's pending zero-readback composites — the same tail the GUI preview
/// and the export path run.
///
/// `view` is sized by [`raster_dims`], not by `dims`: at a scale below 1 the
/// scene is re-encoded scaled down ([`scaled_scene`]) while the timeline still
/// evaluates against the scene's own dimensions. Filter scopes keep rendering
/// at scene resolution (the backend's targets are allocated that way), so their
/// composites are blitted into the smaller target at `origin * scale`.
#[allow(clippy::too_many_arguments)]
fn render_timeline(
    core: &mut RendererCore,
    filter_backend: &mut Option<GpuFilterBackend>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    target: &wgpu::Texture,
    timeline: &Timeline,
    time_s: f64,
    dims: SceneDimensions,
    scale: f32,
    scratch: &mut vello::Scene,
) -> Result<(), String> {
    if filter_backend.is_none() {
        *filter_backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dims)?);
    }
    let (raster_w, raster_h) = raster_dims(dims, scale);
    // Tell the chain what it is being drawn into. It used to allocate and
    // dispatch at scene resolution whatever the raster ended up as, so the page's
    // quality step bought nothing on a filtered figure — and the filtered figures
    // are the ones that need it. Derived from the integer raster rather than from
    // `scale` so the chain and the target cannot disagree by a rounding step.
    if let Some(fb) = filter_backend.as_mut() {
        let achieved = (raster_w as f32 / dims.width.max(1) as f32)
            .min(raster_h as f32 / dims.height.max(1) as f32);
        fb.set_raster_scale(achieved);
    }
    let mut fb: Option<&mut dyn FilterBackend> = filter_backend.as_mut().map(|b| b as _);
    let scene = timeline.evaluate_with_debug(time_s, dims, DebugRenderOptions::default(), &mut fb);
    let scene = scaled_scene(scratch, &scene, scale);
    core.render_vello_scene(device, queue, view, raster_w, raster_h, scene)
        .map_err(|e| e.to_string())?;

    let pending = filter_backend
        .as_mut()
        .map(|fb| fb.take_pending_composites())
        .unwrap_or_default();
    // The same tail the GUI and the export path run: filter composites, plus a
    // `Glass` scope's backdrop (read out of `target`) and its children above it.
    // The web target is rasterised at `dims * scale` while the timeline evaluated
    // against `dims`, so the drain is what applies the scale — including to the
    // backdrop region, which is why it receives `scale` rather than being
    // pre-scaled here.
    let s = scale.clamp(MIN_RENDER_SCALE, 1.0);
    if let Some(fb) = filter_backend.as_mut() {
        drain_pending_layers(core, device, queue, fb, pending, Some(target), view, s);
    }
    Ok(())
}

/// Per-pixel-sample statistics over a mapped readback buffer, as JSON.
/// Drop the row alignment a readback buffer carries (rows are padded to a
/// 256-byte boundary) so the caller gets `width * height * 4` tight RGBA bytes.
fn compact_rgba(data: &[u8], width: u32, height: u32, bytes_per_row: u32) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        let start = y * bytes_per_row as usize;
        out.extend_from_slice(&data[start..start + row_bytes]);
    }
    out
}

fn summarize_pixels(data: &[u8], width: u32, height: u32, bytes_per_row: u32) -> String {
    use std::collections::HashSet;
    let mut samples = 0u64;
    let mut sum = [0u64; 3];
    let mut distinct = HashSet::new();
    let stride = 8u32;
    for y in (0..height).step_by(stride as usize) {
        let row = &data[y as usize * bytes_per_row as usize..];
        for x in (0..width).step_by(stride as usize) {
            let px = &row[x as usize * 4..x as usize * 4 + 4];
            sum[0] += px[0] as u64;
            sum[1] += px[1] as u64;
            sum[2] += px[2] as u64;
            distinct.insert((px[0] >> 3, px[1] >> 3, px[2] >> 3));
            samples += 1;
        }
    }
    if samples == 0 {
        return "{\"error\":\"no samples\"}".to_string();
    }
    let avg = [sum[0] / samples, sum[1] / samples, sum[2] / samples];
    format!(
        "{{\"avg\":[{},{},{}],\"distinct\":{},\"samples\":{},\"width\":{},\"height\":{}}}",
        avg[0],
        avg[1],
        avg[2],
        distinct.len(),
        samples,
        width,
        height
    )
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
