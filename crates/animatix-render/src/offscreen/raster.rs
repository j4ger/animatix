//! Rasterising an evaluated scene into a target view, including filter-layer
//! compositing.

use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::FilterBackend;

use crate::core::RendererCore;
use crate::filter_backend::GpuFilterBackend;

use super::RasterFit;

impl RasterFit<'_> {
    /// The factor the scene is re-encoded at.
    fn scale(&self) -> f32 {
        match self {
            Self::OneToOne => 1.0,
            Self::Scaled { scale, .. } => *scale,
        }
    }
}

/// Rasterise one evaluated scene into `view` and composite the filter
/// backend's pending layers on top.
///
/// This is *the* per-target tail: the offscreen renderer, the transition
/// renderer and the web player's scaled raster all end here, so a `Glass`
/// panel's backdrop and its children are composited identically everywhere.
#[allow(clippy::too_many_arguments)]
pub fn rasterize_scene(
    core: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    scene: &vello::Scene,
    raster: SceneDimensions,
    fit: RasterFit<'_>,
    backend: Option<&mut GpuFilterBackend>,
    backdrop_target: Option<&wgpu::Texture>,
) -> Result<(), String> {
    let scale = fit.scale();
    let encoded = match fit {
        RasterFit::OneToOne => scene,
        RasterFit::Scaled { scratch, .. } => {
            scratch.reset();
            scratch.append(scene, Some(vello::kurbo::Affine::scale(scale as f64)));
            &*scratch
        },
    };
    core.render_vello_scene(device, queue, view, raster.width, raster.height, encoded)
        .map_err(|e| e.to_string())?;

    let Some(backend) = backend else {
        return Ok(());
    };
    let pending = backend.take_pending_composites();
    drain_pending_layers(core, device, queue, backend, pending, backdrop_target, view, scale);
    Ok(())
}

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
