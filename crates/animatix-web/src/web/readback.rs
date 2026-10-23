//! GPU pixel readback diagnostics: text summaries and raw RGBA frames.

use animatix::timeline::SceneDimensions;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;

use super::frame::render_document;
use super::{AmxPlayer, EngineContext, OffscreenTarget, with_engine};

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
