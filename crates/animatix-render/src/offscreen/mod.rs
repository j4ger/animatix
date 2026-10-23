use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;

#[cfg(test)]
mod tests;
/// A single frame rendered to CPU-accessible RGBA memory.
///
/// The pixel buffer is shared (`Arc`): the renderer keeps its own handle to
/// the previous frame's allocation and reuses it in place on the next
/// readback (`Arc::make_mut` is a no-op clone once the encoder drops its
/// reference), so a steady-state video export performs zero per-frame
/// allocations for the multi-megabyte readback (PF-6 round 9: dhat measured
/// 3.7 MB/frame on a 720p export — 93% of the export path's churn).
mod transitions;

#[derive(Debug, Clone)]
pub struct RenderedFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Raw RGBA8 pixel data, row-major order.
    pub rgba: std::sync::Arc<Vec<u8>>,
}

/// GPU-backed offscreen renderer that evaluates a [`Timeline`] and produces
/// [`RenderedFrame`] buffers or intermediate GPU textures.
pub struct OffscreenRenderer {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) core: RendererCore,
    pub(super) output_texture: Option<wgpu::Texture>,
    pub(super) output_view: Option<wgpu::TextureView>,
    /// Two rotating MAP_READ buffers for the pipelined readback (PF-7): while
    /// frame N's copy is mapped/encoded, frame N+1's copy lands in the other
    /// buffer — one-frame GPU/CPU overlap without corrupting a mapped buffer.
    pub(super) readback_buffers: [Option<wgpu::Buffer>; 2],
    pub(super) texture_a: Option<wgpu::Texture>,
    pub(super) view_a: Option<wgpu::TextureView>,
    pub(super) texture_b: Option<wgpu::Texture>,
    pub(super) view_b: Option<wgpu::TextureView>,
    pub(super) compositor: Option<TransitionCompositor>,
    /// Cached GPU filter backend — recreated only when dimensions change.
    /// Serves the single-scene path and a transition's outgoing scene; the
    /// incoming scene gets [`Self::filter_backend_b`] so the two evaluations
    /// cannot stomp each other's pass state.
    pub(super) filter_backend: Option<GpuFilterBackend>,
    /// Second backend for a transition's incoming scene. Lazily created like
    /// [`Self::filter_backend`]; both reset together on a dimension change.
    pub(super) filter_backend_b: Option<GpuFilterBackend>,
    pub(super) filter_backend_dimensions: Option<SceneDimensions>,
    pub(super) dimensions: SceneDimensions,
    pub(super) bytes_per_row: u32,
    /// Recycled CPU readback buffer (PF-6 round 9): parked between frames and
    /// handed out again once the encoder drops its reference — the 720p RGBA
    /// readback was 3.7 MB allocated per frame (93% of export-path churn).
    pub(super) readback_buffer: Option<std::sync::Arc<Vec<u8>>>,
    /// Next `readback_buffers` slot for `queue_readback`.
    pub(super) readback_slot: usize,
}

/// A frame whose GPU readback copy has been queued but not yet waited on.
///
/// Plain data — the renderer stays free between `begin_frame` and
/// `wait_frame`, which is what makes the one-frame overlap possible
/// (`begin(t1)` before `wait(t0)` keeps the GPU busy while the CPU handles
/// frame t0's pixels).
#[derive(Debug)]
pub struct PendingFrame {
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    dims: SceneDimensions,
    bytes_per_row: u32,
}

/// Evaluate one frame of `timeline` with `backend` (lazily created) and draw
/// the result — the encoded scene *plus* the scope's pending filter
/// composites — into `view`.
///
/// This is the per-target tail of the single-scene path
/// (`render_to_output_texture_inner`), shared with the transition path so
/// both scenes of a blend keep their `Filter` scopes. Free-standing so a
/// call site can borrow the core and one backend slot off `self` disjointly.
/// Composite every queued layer onto `view`, in queue order.
///
/// A `Backdrop` layer is served here rather than during evaluation: its input is
/// the frame that has just been rendered into `target`, and that frame did not
/// exist while the scene was being built. Order is what keeps a `Glass` panel's
/// children above its blur — the queue is append-only, so the depth the walk
/// produced survives the split into separate renders.
pub fn drain_pending_layers(
    core: &RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backend: &mut dyn animatix::timeline::effects::FilterBackend,
    layers: Vec<animatix::timeline::effects::PendingLayer>,
    target: Option<&wgpu::Texture>,
    view: &wgpu::TextureView,
    scale: f32,
) {
    use animatix::timeline::effects::{EffectRegion, PendingComposite, PendingLayer};
    let scale_region = |r: EffectRegion, s: f32| EffectRegion {
        origin: [r.origin[0] * s, r.origin[1] * s],
        size: SceneDimensions {
            width: ((r.size.width as f32 * s).round() as u32).max(1),
            height: ((r.size.height as f32 * s).round() as u32).max(1),
        },
    };

    struct PreparedComposite {
        composite: PendingComposite,
        origin: [f32; 2],
        size: [u32; 2],
        corner_radius: Option<f32>,
        src_rect: Option<[f32; 4]>,
    }

    let mut batch: Vec<PreparedComposite> = Vec::new();
    let flush_batch =
        |batch: &mut Vec<PreparedComposite>,
         backend: &mut dyn animatix::timeline::effects::FilterBackend| {
            if batch.is_empty() {
                return;
            }
            let items: Vec<crate::fullscreen_blit::BatchedBlitItem<'_>> = batch
                .iter()
                .map(|p| crate::fullscreen_blit::BatchedBlitItem {
                    src_view: &p.composite.view,
                    dst_origin: p.origin,
                    dst_size: p.size,
                    alpha: p.composite.alpha,
                    corner_radius: p.corner_radius,
                    src_rect: p.src_rect,
                })
                .collect();
            core.blit_texture_rect_masked_batch(device, queue, view, &items);
            for item in batch.drain(..) {
                backend.recycle_pending_composite(item.composite);
            }
        };

    for layer in layers {
        // `s` is the factor still to apply when the composite is blitted. A
        // backdrop is scaled *before* its pass runs — the region is read out of
        // the target at the target's own resolution — so its result already
        // speaks target pixels and must not be scaled twice.
        let (composite, layer_scale) = match layer {
            PendingLayer::Composite(composite) => (composite, scale),
            PendingLayer::Backdrop(backdrop) => {
                // Flush prior composites so the target is up-to-date before the backdrop reads it.
                flush_batch(&mut batch, backend);

                let Some(target) = target else {
                    tracing::warn!("backdrop queued with no render target to read");
                    continue;
                };
                let backdrop = if (scale - 1.0).abs() > f32::EPSILON {
                    animatix::timeline::effects::PendingBackdrop {
                        region: scale_region(backdrop.region, scale),
                        clip: scale_region(backdrop.clip, scale),
                        corner_radius: backdrop.corner_radius * scale,
                        ..backdrop
                    }
                } else {
                    backdrop
                };
                match backend.run_backdrop(target, &backdrop) {
                    Ok(mut composite) => {
                        // The rects handed to `run_backdrop` were already put in
                        // target pixels above, so whatever it produced is in
                        // target space and needs no factor — including the
                        // `texel_scale` the backend stamped from its own raster.
                        composite.texel_scale = 1.0;
                        (composite, 1.0)
                    },
                    Err(e) => {
                        tracing::warn!("backdrop pass failed: {e}");
                        continue;
                    },
                }
            },
        };
        // `texel_scale` is how many texture pixels stand for one scene pixel. A
        // chain that was told the raster scale filtered at that scale, so its
        // result is already in target pixels and the frame scale must not be
        // applied a second time; `layer_scale` carries what the layer needs
        // otherwise.
        let s = layer_scale / composite.texel_scale.max(f32::EPSILON);
        let tex = composite.texture.size();
        let (origin, size) = match composite.clip_rect {
            Some([x0, y0, x1, y1]) => (
                [x0 * s, y0 * s],
                [
                    (((x1 - x0) * s).round() as u32).max(1),
                    (((y1 - y0) * s).round() as u32).max(1),
                ],
            ),
            None => (
                [composite.origin[0] * s, composite.origin[1] * s],
                [
                    ((tex.width as f32 * s).round() as u32).max(1),
                    ((tex.height as f32 * s).round() as u32).max(1),
                ],
            ),
        };
        let src_rect = composite.src_rect.map(|[x0, y0, x1, y1]| [x0 * s, y0 * s, x1 * s, y1 * s]);
        let radius = composite.corner_radius * s;
        batch.push(PreparedComposite {
            composite,
            origin,
            size,
            corner_radius: (radius > 0.0).then_some(radius),
            src_rect,
        });
    }

    flush_batch(&mut batch, backend);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_timeline_into_view(
    core: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    timeline: &Timeline,
    time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
    backend: &mut Option<GpuFilterBackend>,
    view: &wgpu::TextureView,
    backdrop_target: Option<&wgpu::Texture>,
    suppressed_actors: &[String],
) -> Result<(), String> {
    if backend.is_none() {
        *backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dimensions)?);
    }
    let mut fb: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
        backend.as_mut().map(|b| b as _);
    let scene = if suppressed_actors.is_empty() {
        timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
            .scene
    } else {
        timeline
            .evaluate_program_with_suppressed(
                time_s,
                dimensions,
                debug_options,
                &mut fb,
                suppressed_actors,
            )
            .scene
    };

    core.render_vello_scene(device, queue, view, dimensions.width, dimensions.height, &scene)
        .map_err(|e| e.to_string())?;

    let pending = backend.as_mut().map(|fb| fb.take_pending_composites()).unwrap_or_default();
    if let (Some(backend), Some(target)) = (backend.as_mut(), backdrop_target) {
        drain_pending_layers(core, device, queue, backend, pending, Some(target), view, 1.0);
    }
    Ok(())
}

mod renderer;
