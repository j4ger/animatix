//! Evaluating one timeline frame and drawing it into a target view.

use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

/// Evaluate and draw one timeline using Vello GPU.
/// Its `SceneRecorder` preserves filter and glass entries in scene order; the
/// frame renderer applies them while rasterizing the neutral scene.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_timeline_into_frame_gpu(
    frame_gpu: &mut crate::frame_gpu::FrameGpu,
    timeline: &Timeline,
    time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
    view: &wgpu::TextureView,
    raster: kurbo::Affine,
    suppressed_actors: &[String],
) -> Result<(), String> {
    let mut recorder = crate::frame_gpu::SceneRecorder;
    let mut backend: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
        Some(&mut recorder);
    let scene = if suppressed_actors.is_empty() {
        timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut backend)
            .scene
    } else {
        timeline
            .evaluate_program_with_suppressed(
                time_s,
                dimensions,
                debug_options,
                &mut backend,
                suppressed_actors,
            )
            .scene
    };
    frame_gpu.render_frame_into(&scene, view, raster)
}
