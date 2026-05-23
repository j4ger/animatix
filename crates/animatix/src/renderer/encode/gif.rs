//! GIF encoding export.
//!
//! Provides single-timeline and multi-scene composition GIF encoding
//! using the `image` crate's GIF encoder with parallel frame rendering.

use crate::composition::Composition;
use crate::renderer::encode::ExportError;
use crate::renderer::encode::video::adaptive_thread_count;
use crate::renderer::render_pipeline::{render_frames_streaming, render_frames_streaming_composition};
use crate::renderer::encode::ExportSettings;
use crate::timeline::{DebugRenderOptions, Timeline};
use std::sync::atomic::{AtomicBool, AtomicU32};
use tracing::info;

// ---------------------------------------------------------------------------
// Public API: single-timeline GIF
// ---------------------------------------------------------------------------

/// Render a single timeline to a GIF file.
pub fn render_gif_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_gif_timeline_with_debug(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
    )
}

/// Render a single timeline to a GIF file with debug options.
pub fn render_gif_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<(), ExportError> {
    pollster::block_on(render_gif_async(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        ExportSettings::default(),
        None,
        None,
    ))
}

/// Render a single timeline to a GIF file with debug options and export settings.
pub fn render_gif_timeline_with_settings(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
) -> Result<(), ExportError> {
    pollster::block_on(render_gif_async(
        timeline, width, height, fps, duration, output_file, debug_options, settings, None, None,
    ))
}

/// Render a single timeline to a GIF file with full control: debug options,
/// export settings, progress tracking, and cancellation.
pub fn render_gif_timeline_with_progress(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    pollster::block_on(render_gif_async(
        timeline, width, height, fps, duration, output_file, debug_options, settings, progress, cancel,
    ))
}

// ---------------------------------------------------------------------------
// Internal async GIF encoder
// ---------------------------------------------------------------------------

pub(super) async fn render_gif_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    use image::codecs::gif::{GifEncoder, Repeat};

    let total_frames = (duration * fps as f32).ceil() as u32;
    let frame_duration_ms = (1000 / fps) as u16;

    // ------------------------------------------------------------------------
    // 1. Parallel frame rendering with streaming to encoder
    // ------------------------------------------------------------------------
    info!("Encoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = GifEncoder::new(output);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    let num_threads = adaptive_thread_count(width, height, total_frames, true, false, &settings);
    info!("Using {num_threads} render thread(s) (adaptive).");

    render_frames_streaming(
        &timeline,
        width,
        height,
        fps,
        total_frames,
        num_threads,
        debug_options,
        progress,
        cancel,
        |frame, scene_frame| {
            let img = image::RgbaImage::from_raw(
                scene_frame.width,
                scene_frame.height,
                scene_frame.rgba,
            )
            .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;

            encoder
                .encode_frame(image::Frame::from_parts(
                    img,
                    0,
                    0,
                    image::Delay::from_saturating_duration(std::time::Duration::from_millis(
                        frame_duration_ms as u64,
                    )),
                ))
                .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

            use std::io::Write;
            print!("\rEncoding GIF frame {}/{}", frame + 1, total_frames);
            std::io::stdout().flush()?;
            Ok(())
        },
    )?;

    info!("GIF render complete!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-Scene Composition GIF Export
// ---------------------------------------------------------------------------

/// Render a multi-scene composition to a GIF file.
pub fn render_gif_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_gif_composition_with_settings(
        composition,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
        ExportSettings::default(),
    )
}

/// Render a multi-scene composition to a GIF file with debug options and export settings.
pub fn render_gif_composition_with_settings(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
) -> Result<(), ExportError> {
    pollster::block_on(render_gif_composition_async(
        composition,
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        settings,
        None,
        None,
    ))
}

/// Render a multi-scene composition to a GIF file with full control: debug options,
/// export settings, progress tracking, and cancellation.
pub fn render_gif_composition_with_progress(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    pollster::block_on(render_gif_composition_async(
        composition,
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        settings,
        progress,
        cancel,
    ))
}

pub(super) async fn render_gif_composition_async(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    use image::codecs::gif::{GifEncoder, Repeat};

    let total_frames = (duration * fps as f32).ceil() as u32;
    let frame_duration_ms = (1000 / fps) as u16;

    info!("Encoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = GifEncoder::new(output);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    let num_threads = adaptive_thread_count(width, height, total_frames, true, false, &settings);
    info!("Using {num_threads} render thread(s) (adaptive).");

    render_frames_streaming_composition(
        composition,
        width,
        height,
        fps,
        total_frames,
        num_threads,
        debug_options,
        progress,
        cancel,
        |frame, scene_frame| {
            let img = image::RgbaImage::from_raw(
                scene_frame.width,
                scene_frame.height,
                scene_frame.rgba,
            )
            .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;

            encoder
                .encode_frame(image::Frame::from_parts(
                    img,
                    0,
                    0,
                    image::Delay::from_saturating_duration(std::time::Duration::from_millis(
                        frame_duration_ms as u64,
                    )),
                ))
                .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

            use std::io::Write;
            print!("\rEncoding GIF frame {}/{}", frame + 1, total_frames);
            std::io::stdout().flush()?;
            Ok(())
        },
    )?;

    info!("GIF render complete!");
    Ok(())
}