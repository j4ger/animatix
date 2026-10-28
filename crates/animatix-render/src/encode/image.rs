//! Image (PNG) single-frame export.
//!
//! Single-timeline and multi-scene composition image rendering to PNG files
//! using the `image` crate.

use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use tracing::info;

use crate::encode::{ExportError, ExportOptions};
use crate::offscreen::OffscreenRenderer;

// ---------------------------------------------------------------------------
// Single-timeline image
// ---------------------------------------------------------------------------

/// Render a single timeline to a PNG image at the given time with default debug
/// options.
pub fn render_image_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_image_timeline_with_options(
        timeline,
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
        ExportOptions::default(),
    )
}

/// Render a single timeline to a PNG image at the given time.
pub fn render_image_timeline_with_options(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    // A single frame has no thread configuration to apply and no frames to
    // count; cancellation is still honoured before the GPU work starts.
    let ExportOptions { cancel, .. } = options;
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err(ExportError::Cancelled);
    }
    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let frame = renderer
        .render_timeline_with_debug(
            &timeline,
            time as f64,
            SceneDimensions { width, height },
            debug_options,
        )
        .map_err(|e| ExportError::FrameRender {
            frame: 0,
            message: e,
        })?;
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.to_vec())
        .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;
    img.save(output_file).map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    info!("Image saved to {}", output_file.display());
    // Headless workflows (CI, scripts) have no GUI diagnostics panel — drain
    // the frame's runtime diagnostics so frame-time failures (plot closure
    // errors, filter fallbacks) are visible on stderr too.
    for diagnostic in timeline.runtime_diagnostics() {
        let location = diagnostic
            .location
            .subject
            .clone()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        tracing::warn!("[{}] {}{location}", diagnostic.severity, diagnostic.message);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-scene composition image
// ---------------------------------------------------------------------------

/// Render a multi-scene composition to a PNG image at the given time.
///
/// A composition frame is a single raster with no frames to count, so there is
/// no progress/cancel variant of this entry point.
pub fn render_image_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    if !composition.has_scenes() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "Composition has no scenes to render".into(),
        });
    }

    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let (scene_name, local_time_s, transition_blend) = composition.evaluate(time as f64);

    let dims = SceneDimensions { width, height };

    let frame =
        if let Some(blend) = transition_blend {
            let from_scene = composition.scenes.get(&blend.from_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: 0,
                    message: format!("From scene '{}' not found in composition", blend.from_scene),
                }
            })?;
            let to_scene = composition.scenes.get(&blend.to_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: 0,
                    message: format!("To scene '{}' not found in composition", blend.to_scene),
                }
            })?;
            renderer
                .render_transition(
                    &from_scene.timeline,
                    blend.from_local,
                    &to_scene.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    dims,
                    DebugRenderOptions::default(),
                )
                .map_err(|e| ExportError::FrameRender {
                    frame: 0,
                    message: e,
                })?
        } else {
            let scene =
                composition.scenes.get(&scene_name).ok_or_else(|| ExportError::FrameRender {
                    frame: 0,
                    message: format!("Scene '{}' not found in composition", scene_name),
                })?;

            renderer
                .render_timeline_with_debug(
                    &scene.timeline,
                    local_time_s,
                    dims,
                    DebugRenderOptions::default(),
                )
                .map_err(|e| ExportError::FrameRender {
                    frame: 0,
                    message: e,
                })?
        };

    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.to_vec())
        .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;
    img.save(output_file).map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    info!("Image saved to {}", output_file.display());
    Ok(())
}
