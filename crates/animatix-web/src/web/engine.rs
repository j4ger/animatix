//! Shared WebGPU engine context: initialization, player creation, and GPU drains.

use std::cell::RefCell;
use std::sync::Arc;

use animatix::timeline::{BuildQuality, SceneDimensions};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use super::context::{CONTEXT, clear_if_generation, next_generation, with_engine};
use super::{AmxPlayer, EngineContext};

impl AmxPlayer {
    pub(super) fn new(
        canvas: HtmlCanvasElement,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
        Self {
            canvas,
            surface,
            config,
            device_generation: 0,
            webgl: None,
            compositor: None,
            fonts: Vec::new(),
            font_context: None,
            modules: Vec::new(),
            vars: Vec::new(),
            runtime_inputs: animatix::timeline::RuntimeInputs::default(),
            quality: BuildQuality::Draft,
            offscreen: None,
            offscreen_to: None,
            composite: None,
            dims: SceneDimensions::default(),
            duration_s: 0.1,
            target: None,
            videos: Default::default(),
            prepared_video_time: None,
            render_scale: 0.85,
            dedup: animatix::timeline::frame_signature::FrameDedup::new(),
            render_generation: 0,
            frames_drawn: 0,
            frames_deduped: 0,
        }
    }
}

/// Create the shared context if it does not exist yet. Idempotent; the JS
/// shell additionally funnels concurrent first calls through one promise.
async fn ensure_engine() -> Result<(), String> {
    let already = CONTEXT.with(|cell| cell.borrow().is_some());
    if already {
        return Ok(());
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
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
    let generation = next_generation();

    // Surface wgpu's internal validation errors on the JS console — the
    // default handler logs through `log`, which has no subscriber here.
    device.on_uncaptured_error(Arc::new(move |error| {
        web_sys::console::error_1(&format!("wgpu uncaptured error: {error}").into());
    }));
    device.set_device_lost_callback(move |reason, message| {
        web_sys::console::error_1(&format!("wgpu device lost: {reason:?} — {message}").into());
        if clear_if_generation(generation) {
            dispatch_backend_lost(generation, &format!("{reason:?}"), &message);
        }
    });

    let frame_gpu = animatix_render::frame_gpu::FrameGpu::new(&device, &queue);

    CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(EngineContext {
            _instance: instance,
            _adapter: adapter,
            device,
            queue,
            generation,
            frame_gpu: RefCell::new(frame_gpu),
        });
    });
    Ok(())
}

/// Notify all page players after the shared WebGPU device is invalidated.
fn dispatch_backend_lost(generation: u64, reason: &str, message: &str) {
    use wasm_bindgen::JsCast;

    let global = js_sys::global();
    let Ok(constructor) = js_sys::Reflect::get(&global, &"CustomEvent".into())
        .and_then(|value| value.dyn_into::<js_sys::Function>())
    else {
        return;
    };
    let detail = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&detail, &"backend".into(), &"webgpu".into());
    let _ =
        js_sys::Reflect::set(&detail, &"generation".into(), &JsValue::from_f64(generation as f64));
    let _ = js_sys::Reflect::set(&detail, &"reason".into(), &reason.into());
    let _ = js_sys::Reflect::set(&detail, &"message".into(), &message.into());
    let init = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&init, &"detail".into(), &detail);
    let args = js_sys::Array::new();
    args.push(&"animatixbackendlost".into());
    args.push(&init);
    let Ok(event) = js_sys::Reflect::construct(&constructor, &args) else {
        return;
    };
    if let Ok(dispatch) = js_sys::Reflect::get(&global, &"dispatchEvent".into())
        .and_then(|value| value.dyn_into::<js_sys::Function>())
    {
        let _ = dispatch.call1(&global, &event);
    }
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
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: canvas.width().max(1),
            height: canvas.height().max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&ctx.device, &config);

        let mut player = AmxPlayer::new(canvas, surface, config);
        player.device_generation = ctx.generation;
        Ok(player)
    })
}

/// Create a player using the requested canvas backend. The existing
/// `create_player(canvas)` export remains the WebGPU compatibility entry point.
#[wasm_bindgen]
pub async fn create_player_for_backend(
    canvas: HtmlCanvasElement,
    backend: &str,
) -> Result<AmxPlayer, JsError> {
    match backend {
        "webgpu" => create_player(canvas).await,
        "webgl2" => super::webgl::create_webgl_player(canvas).await,
        _ => Err(JsError::new("backend must be 'webgpu' or 'webgl2'")),
    }
}
