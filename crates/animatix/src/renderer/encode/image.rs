//! Image (PNG) single-frame export.
//!
//! Provides single-timeline and multi-scene composition image rendering
//! to PNG files using the `image` crate.

use crate::composition::Composition;
use crate::renderer::encode::ExportError;
use crate::renderer::offscreen::OffscreenRenderer;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use std::sync::atomic::{AtomicBool, AtomicU32};
use tracing::info;

// ---------------------------------------------------------------------------
// Public API: single-timeline image
// ---------------------------------------------------------------------------

pub fn render_image(
    ast: &[crate::ast::Stmt],
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    pollster::block_on(render_image_async(
        Timeline::build(ast),
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
        None,
        None,
    ))
}

pub fn render_image_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_image_timeline_with_debug(
        timeline,
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
    )
}

pub fn render_image_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<(), ExportError> {
    pollster::block_on(render_image_async(
        timeline, width, height, time, output_file, debug_options, None, None,
    ))
}

pub fn render_image_timeline_with_progress(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    pollster::block_on(render_image_async(
        timeline, width, height, time, output_file, debug_options, progress, cancel,
    ))
}

// ---------------------------------------------------------------------------
// Internal async image renderer
// ---------------------------------------------------------------------------

pub(super) async fn render_image_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    _progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
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
        .map_err(|e| ExportError::FrameRender { frame: 0, message: e })?;
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;
    img.save(output_file)
        .map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    info!("Image saved to {}", output_file.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-Scene Composition Image Export
// ---------------------------------------------------------------------------

pub fn render_image_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    pollster::block_on(render_image_composition_async(
        composition,
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
        None,
        None,
    ))
}

pub(super) async fn render_image_composition_async(
    composition: &Composition,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    _progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err(ExportError::Cancelled);
    }

    if !composition.has_scenes() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "Composition has no scenes to render".into(),
        });
    }

    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let (scene_name, local_time_s, transition_blend) = composition.evaluate(time as f64);

    let dims = SceneDimensions { width, height };

    let frame = if let Some(blend) = transition_blend {
        let from_scene = composition
            .scenes
            .get(&blend.from_scene)
            .ok_or_else(|| ExportError::FrameRender {
                frame: 0,
                message: format!(
                    "From scene '{}' not found in composition",
                    blend.from_scene
                ),
            })?;
        let to_scene = composition
            .scenes
            .get(&blend.to_scene)
            .ok_or_else(|| ExportError::FrameRender {
                frame: 0,
                message: format!("To scene '{}' not found in composition", blend.to_scene),
            })?;
        let to_start = composition
            .scene_start_times
            .get(&blend.to_scene)
            .copied()
            .unwrap_or(0.0);
        let to_local = time as f64 - to_start;

        renderer
            .render_transition(
                &from_scene.timeline,
                local_time_s,
                &to_scene.timeline,
                to_local,
                blend.progress as f32,
                blend.id.clone(),
                blend.easing,
                dims,
                debug_options,
            )
            .map_err(|e| ExportError::FrameRender { frame: 0, message: e })?
    } else {
        let scene = composition
            .scenes
            .get(&scene_name)
            .ok_or_else(|| ExportError::FrameRender {
                frame: 0,
                message: format!("Scene '{}' not found in composition", scene_name),
            })?;

        renderer
            .render_timeline_with_debug(&scene.timeline, local_time_s, dims, debug_options)
            .map_err(|e| ExportError::FrameRender { frame: 0, message: e })?
    };

    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;
    img.save(output_file)
        .map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    info!("Image saved to {}", output_file.display());
    Ok(())
}