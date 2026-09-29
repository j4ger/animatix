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
use animatix::timeline::{BuildQuality, DebugRenderOptions, SceneDimensions, Timeline};
use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::parse_source;

use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
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
    49
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
        let format = capabilities
            .formats
            .first()
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
            offscreen: None,
            offscreen_to: None,
            composite: None,
            dims: SceneDimensions::default(),
            duration_s: 0.1,
            target: None,
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
    /// Lazily-built GPU transition blender (compiles the WGSL blend
    /// pipeline). Per player rather than per context so the lazy init has a
    /// `&mut` to write into next to the targets it serves.
    compositor: Option<TransitionCompositor>,
    /// Scene-space dimensions of the loaded document (canvas pixels follow
    /// the aspect ratio at whatever scale the shell picks).
    dims: SceneDimensions,
    duration_s: f64,
    target: Option<BuildTarget>,
}

/// Everything the map_async callback needs to own: the mapped buffer plus
/// the geometry to interpret its bytes.
struct ReadbackSetup {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
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
        let mut cache = AssetCache::new();
        for (url, payload) in urls.into_iter().zip(payloads.into_iter()) {
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
        let built = host::build_document_with_assets(
            source,
            Arc::new(font_context),
            BuildQuality::Draft,
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

    pub fn scene_width(&self) -> u32 {
        self.dims.width
    }

    pub fn scene_height(&self) -> u32 {
        self.dims.height
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

        // Vello draws through a compute pipeline that needs STORAGE_BINDING
        // on its target, which browser canvas contexts don't reliably expose.
        // Mirror the GUI's PreviewSurface: render into an offscreen texture we
        // own at scene resolution (vello draws scene units 1:1 — no camera
        // scaling), then blit it scaled onto the surface view. During a
        // multi-scene transition the outgoing and incoming scenes render into
        // two of those targets and the compositor blends them into a third.
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
            // frame) — reconfigure and let the next rAF tick retry.
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&ctx.device, &self.config);
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
            self.compositor.as_ref(),
        );

        // Blit the rendered frame onto the swapchain view and present. The
        // blit scales from the offscreen resolution to canvas pixels; on a
        // render error the frame is dropped unpresented so the canvas keeps
        // its last known-good content.
        if let Ok(frame) = &frame_target {
            core.blit_texture(
                &ctx.device,
                &ctx.queue,
                &frame.view,
                &surface_view,
                width,
                height,
                1.0,
            );
        }

        drop(surface_view);
        frame.present();
        frame_target
            .map(|_| ())
            .map_err(|e| JsError::new(&format!("render failed: {e}")))?;
        Ok(())
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
    compositor: Option<&TransitionCompositor>,
) -> Result<FrameTarget, String> {
    let scene = (dims.width, dims.height);
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
                    timeline,
                    local_time_s,
                    dims,
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
                from_timeline,
                blend.from_local,
                dims,
            )?;
            render_timeline(
                core,
                secondary_backend,
                device,
                queue,
                &to.view,
                to_timeline,
                to_local,
                dims,
            )?;
            compositor.render(
                device,
                queue,
                &from.view,
                &to.view,
                &composite_target.view,
                dims.width,
                dims.height,
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
                timeline,
                time_s,
                dims,
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

    let AmxPlayer {
        target,
        filter_backend,
        filter_backend_to,
        ..
    } = player;
    let mut core = ctx.core.borrow_mut();
    let frame = render_document(
        &mut core,
        filter_backend,
        filter_backend_to,
        &ctx.device,
        &ctx.queue,
        &mut primary,
        &mut secondary,
        &mut composite,
        target.as_ref(),
        time_s,
        dims,
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
/// scopes render like the export path) and draw it into `view`.
fn render_timeline(
    core: &mut RendererCore,
    filter_backend: &mut Option<GpuFilterBackend>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    timeline: &Timeline,
    time_s: f64,
    dims: SceneDimensions,
) -> Result<(), String> {
    if filter_backend.is_none() {
        *filter_backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dims)?);
    }
    let mut fb: Option<&mut dyn FilterBackend> = filter_backend.as_mut().map(|b| b as _);
    let scene = timeline.evaluate_with_debug(time_s, dims, DebugRenderOptions::default(), &mut fb);
    core.render_vello_scene(device, queue, view, dims.width, dims.height, &scene)
        .map_err(|e| e.to_string())?;

    let pending = filter_backend
        .as_mut()
        .map(|fb| fb.take_pending_composites())
        .unwrap_or_default();
    for composite in pending {
        let size = composite.texture.size();
        core.blit_texture_rect(
            device,
            queue,
            &composite.view,
            view,
            composite.origin,
            [size.width, size.height],
            composite.alpha,
        );
    }
    Ok(())
}

/// Per-pixel-sample statistics over a mapped readback buffer, as JSON.
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
