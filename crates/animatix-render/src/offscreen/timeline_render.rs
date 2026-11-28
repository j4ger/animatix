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
    omit: &[&str],
) -> Result<(), String> {
    let mut recorder = crate::frame_gpu::SceneRecorder;
    let mut backend: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
        Some(&mut recorder);
    let program = timeline.evaluate_program_with_paint_omissions(
        time_s,
        dimensions,
        debug_options,
        &mut backend,
        suppressed_actors,
        omit,
    );
    check_video_diagnostics(&program)?;
    frame_gpu.render_frame_into(&program.scene, view, raster)
}

pub(super) fn check_video_diagnostics(
    program: &animatix::timeline::scene_program::SceneProgram,
) -> Result<(), String> {
    if let Some(diagnostic) = program.diagnostics.iter().find(|item| {
        item.is_error()
            && item.code == animatix_syntax::diagnostics::DiagnosticCode::VideoClipFailure
    }) {
        return Err(diagnostic.message.clone());
    }
    Ok(())
}
