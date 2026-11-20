//! GPU pixel readback diagnostics: text summaries and raw RGBA frames.

use std::sync::{Arc, Mutex};

use animatix::timeline::SceneDimensions;
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use super::context::with_engine;
use super::frame::{FrameCtx, FrameRenderer, Targets, render_document};
use super::{AmxPlayer, OffscreenTarget};

/// Everything the map_async callback needs to own: the mapped buffer plus
/// the geometry to interpret its bytes.
struct ReadbackSetup {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    /// Wgpu-core GL needs polling to process a pending map. BrowserWebGPU
    /// completes map callbacks through its native promise queue instead.
    poll_device: Option<wgpu::Device>,
}

#[derive(Clone, Copy)]
enum ReadbackKind {
    Summary,
    Rgba,
}

enum ReadbackValue {
    Summary(String),
    Rgba {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
}

const GL_MAP_POLL_INTERVAL_MS: i32 = 16;
const GL_MAP_MAX_POLL_ATTEMPTS: u32 = 300;

fn readback_impl(player: &mut AmxPlayer, time_s: f64) -> Result<ReadbackSetup, String> {
    let dims = player.dims;
    if player.target.is_none() {
        return Err("no document loaded".to_string());
    }
    if let Some(webgl) = player.webgl.clone() {
        let mut frame_gpu = webgl.frame_gpu.borrow_mut();
        return readback_with(
            player,
            &webgl.device,
            &webgl.queue,
            Some(webgl.device.clone()),
            &mut frame_gpu,
            time_s,
            dims,
        );
    }
    with_engine(|ctx| {
        let ctx = ctx?;
        let mut renderer = ctx.frame_gpu.borrow_mut();
        readback_with(player, &ctx.device, &ctx.queue, None, &mut renderer, time_s, dims)
    })
}

#[wasm_bindgen]
impl AmxPlayer {
    /// Diagnostic: render the current document into an offscreen texture and
    /// read the pixels back through a GPU buffer (no compositor involved).
    /// `on_result` receives a JSON string
    /// `{ avg: [r, g, b], distinct: N, samples: N, width, height }`.
    ///
    /// WebGPU completes mapping through its browser callback queue. The
    /// WebGL2 path polls cooperatively on timers and reports a bounded failure.
    pub fn debug_readback(&mut self, time_s: f64, on_result: js_sys::Function) {
        start_readback(self, time_s, ReadbackKind::Summary, on_result);
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
        start_readback(self, time_s, ReadbackKind::Rgba, on_result);
    }
}

fn start_readback(
    player: &mut AmxPlayer,
    time_s: f64,
    kind: ReadbackKind,
    on_result: js_sys::Function,
) {
    let setup = match readback_impl(player, time_s) {
        Ok(setup) => setup,
        Err(error) => {
            call_readback_error(&on_result, &error);
            return;
        },
    };
    let poll_device = setup.poll_device.clone();
    let buffer = setup.buffer.clone();
    let map_target = setup.buffer.clone();
    let callback = Arc::new(Mutex::new(Some(on_result)));
    let map_callback = callback.clone();
    let map_buffer = buffer.clone();

    map_target.slice(..).map_async(wgpu::MapMode::Read, move |mapped| {
        if map_callback.lock().unwrap().is_none() {
            return;
        }
        let value = match mapped {
            Ok(()) => {
                let slice = map_buffer.slice(..);
                match slice.get_mapped_range() {
                    Ok(data) => {
                        let value = match kind {
                            ReadbackKind::Summary => ReadbackValue::Summary(summarize_pixels(
                                &data,
                                setup.width,
                                setup.height,
                                setup.bytes_per_row,
                            )),
                            ReadbackKind::Rgba => ReadbackValue::Rgba {
                                width: setup.width,
                                height: setup.height,
                                bytes: compact_rgba(
                                    &data,
                                    setup.width,
                                    setup.height,
                                    setup.bytes_per_row,
                                ),
                            },
                        };
                        drop(data);
                        Ok(value)
                    },
                    Err(error) => Err(format!("map range failed: {error}")),
                }
            },
            Err(error) => Err(format!("map failed: {error}")),
        };
        map_buffer.unmap();
        match value {
            Ok(value) => call_readback_value(&map_callback, value),
            Err(error) => call_readback_error_once(&map_callback, &error),
        }
    });

    if let Some(device) = poll_device {
        if let Err(error) = schedule_gl_readback_poll(device, buffer.clone(), callback.clone(), 0) {
            fail_gl_readback(&callback, &buffer, &error);
        }
    }
}

/// WebGPU dispatches map callbacks through its JS promise. The GL backend is
/// wgpu-core, so keep polling it asynchronously until mapping completes or the
/// bounded diagnostic wait fails.
fn schedule_gl_readback_poll(
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    callback: Arc<Mutex<Option<js_sys::Function>>>,
    attempt: u32,
) -> Result<(), String> {
    if callback.lock().unwrap().is_none() {
        return Ok(());
    }
    if attempt >= GL_MAP_MAX_POLL_ATTEMPTS {
        return Err("WebGL2 readback mapping timed out".to_string());
    }
    let window = web_sys::window().ok_or_else(|| {
        "WebGL2 readback requires a Window timer to complete buffer mapping".to_string()
    })?;
    let next_device = device.clone();
    let next_buffer = buffer.clone();
    let next_callback = callback.clone();
    let timer = Closure::once_aborting(move || {
        if next_callback.lock().unwrap().is_none() {
            return;
        }
        if let Err(error) = next_device.poll(wgpu::PollType::Poll) {
            fail_gl_readback(
                &next_callback,
                &next_buffer,
                &format!("WebGL2 readback polling failed: {error}"),
            );
            return;
        }
        if let Err(error) = schedule_gl_readback_poll(
            next_device,
            next_buffer.clone(),
            next_callback.clone(),
            attempt + 1,
        ) {
            fail_gl_readback(&next_callback, &next_buffer, &error);
        }
    });
    let timer_value = timer.into_js_value();
    let timer_function = timer_value.unchecked_ref::<js_sys::Function>();
    if let Err(error) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        timer_function,
        GL_MAP_POLL_INTERVAL_MS,
    ) {
        let message = format!("failed to schedule WebGL2 readback polling: {error:?}");
        fail_gl_readback(&callback, &buffer, &message);
        // `once_aborting` is released when called. The completion flag makes
        // this cleanup invocation a no-op after scheduling has failed.
        let _ = timer_function.call0(&JsValue::NULL);
        return Err(message);
    }
    Ok(())
}

fn fail_gl_readback(
    callback: &Arc<Mutex<Option<js_sys::Function>>>,
    buffer: &wgpu::Buffer,
    message: &str,
) {
    let Some(callback) = callback.lock().unwrap().take() else {
        return;
    };
    buffer.unmap();
    call_readback_error(&callback, message);
}

fn call_readback_error_once(callback: &Arc<Mutex<Option<js_sys::Function>>>, error: &str) {
    if let Some(callback) = callback.lock().unwrap().take() {
        call_readback_error(&callback, error);
    }
}

fn call_readback_error(callback: &js_sys::Function, error: &str) {
    let message = js_sys::JSON::stringify(&JsValue::from_str(error))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| "\"readback failed\"".into());
    let payload = format!("{{\"error\":{message}}}");
    let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&payload));
}

fn call_readback_value(callback: &Arc<Mutex<Option<js_sys::Function>>>, value: ReadbackValue) {
    let Some(callback) = callback.lock().unwrap().take() else {
        return;
    };
    match value {
        ReadbackValue::Summary(summary) => {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&summary));
        },
        ReadbackValue::Rgba {
            width,
            height,
            bytes,
        } => {
            let out = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&out, &"width".into(), &width.into());
            let _ = js_sys::Reflect::set(&out, &"height".into(), &height.into());
            let _ = js_sys::Reflect::set(
                &out,
                &"bytes".into(),
                &js_sys::Uint8Array::from(bytes.as_slice()),
            );
            let _ = callback.call1(&JsValue::NULL, &out);
        },
    }
}

/// Read the current document back through an offscreen render — the diagnostic
/// twin of [`render_document`], so a readback taken mid-transition reports the
/// blended frame a viewer would see.
fn readback_with(
    player: &mut AmxPlayer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    poll_device: Option<wgpu::Device>,
    renderer: &mut FrameRenderer,
    time_s: f64,
    dims: SceneDimensions,
) -> Result<ReadbackSetup, String> {
    let bytes_per_row = (dims.width * 4 + 255) & !255;

    // Fresh targets: the readback must not disturb the player's own
    // presentation slots.
    let mut primary: Option<OffscreenTarget> = None;
    let mut secondary: Option<OffscreenTarget> = None;
    let mut composite: Option<OffscreenTarget> = None;
    if player.compositor.is_none() {
        player.compositor = Some(TransitionCompositor::new(device)?);
    }

    let AmxPlayer {
        target,
        runtime_inputs,
        ..
    } = player;
    // The probe gets its own filter backends for the same reason it gets its own
    // targets. `render_document` drains `take_pending_composites()` off whichever
    // backend it is handed and blits the result into its target, so probing
    // through the player's backend pulled the composited effect regions off a
    // frame that was still in flight — the probe's pixels came out right and the
    // next presented frame came out missing them.
    // Readbacks always rasterize at the scene's own resolution: the pixel
    // probes compare against backdrop baselines and each other, so they must
    // not inherit whatever scale the presentation path is currently using.
    let frame = render_document(
        &mut FrameCtx {
            renderer,
            device,
            queue,
            dims,
            scale: 1.0,
            compositor: player.compositor.as_ref(),
            target: target.as_ref(),
            runtime_inputs: &*runtime_inputs,
            time_s,
        },
        Targets {
            primary: &mut primary,
            secondary: &mut secondary,
            composite: &mut composite,
        },
    )?;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("animatix-web readback buffer"),
        size: bytes_per_row as u64 * dims.height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
    queue.submit(std::iter::once(encoder.finish()));

    Ok(ReadbackSetup {
        buffer,
        width: dims.width,
        height: dims.height,
        bytes_per_row,
        poll_device,
    })
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
