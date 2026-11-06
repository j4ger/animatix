//! Evaluating one timeline frame and drawing it into a target view.

use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

use crate::core::RendererCore;
use crate::filter_backend::GpuFilterBackend;
use crate::offscreen::{RasterFit, rasterize_scene};

/// Evaluate one frame of `timeline` with `backend` (lazily created) and draw
/// the result — the encoded scene *plus* the scope's pending filter
/// composites — into `view`.
///
/// Free-standing so a call site can borrow the core and one backend slot off
/// `self` disjointly; the raster itself is [`rasterize_scene`], shared with the
/// web player and the export readback path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_timeline_into_view(
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

    rasterize_scene(
        core,
        device,
        queue,
        view,
        &scene,
        dimensions,
        RasterFit::OneToOne,
        backend.as_mut(),
        backdrop_target,
    )
}
