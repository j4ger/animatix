//! Shared WebGPU engine context: initialization, player creation, and GPU drains.

use std::cell::RefCell;
use std::sync::Arc;

use animatix::timeline::{BuildQuality, SceneDimensions};
use animatix_render::core::RendererCore;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use super::{AmxPlayer, CONTEXT, EngineContext, with_engine};

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
