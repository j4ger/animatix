//! GIF encoding export.
//!
//! Single-timeline and multi-scene composition GIF encoding using the `image`
//! crate's GIF encoder with parallel frame rendering.

use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, Timeline};
use tracing::info;

use crate::encode::video::adaptive_thread_count;
use crate::encode::{ExportError, ExportOptions};
use crate::render_pipeline::{render_frames_streaming, render_frames_streaming_composition};

// ---------------------------------------------------------------------------
// Single-timeline GIF
// ---------------------------------------------------------------------------

/// Render a single timeline to a GIF file with default debug options and export
/// settings.
pub fn render_gif_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_gif_timeline_with_options(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
        ExportOptions::default(),
    )
}

/// Render a single timeline to a GIF file.
pub fn render_gif_timeline_with_options(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    let ExportOptions {
        settings,
        progress,
        cancel,
    } = options;
    let total_frames = (duration * fps as f32).ceil() as u32;
    let frame_duration_ms = ((1000.0 / fps as f64).round() as u16).max(1);

    info!("Encoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = image::codecs::gif::GifEncoder::new(output);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
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
            encode_gif_frame(&mut encoder, frame, total_frames, frame_duration_ms, scene_frame)
        },
    )?;

    info!("GIF render complete!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-scene composition GIF
// ---------------------------------------------------------------------------

/// Render a multi-scene composition to a GIF file with default debug options
/// and export settings.
pub fn render_gif_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_gif_composition_with_options(
        composition,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
        ExportOptions::default(),
    )
}

/// Render a multi-scene composition to a GIF file.
pub fn render_gif_composition_with_options(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    let ExportOptions {
        settings,
        progress,
        cancel,
    } = options;
    let total_frames = (duration * fps as f32).ceil() as u32;
    let frame_duration_ms = ((1000.0 / fps as f64).round() as u16).max(1);

    info!("Encoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = image::codecs::gif::GifEncoder::new(output);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
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
            encode_gif_frame(&mut encoder, frame, total_frames, frame_duration_ms, scene_frame)
        },
    )?;

    info!("GIF render complete!");
    Ok(())
}

/// Append one rendered frame to the GIF and report progress on stdout.
fn encode_gif_frame(
    encoder: &mut image::codecs::gif::GifEncoder<std::fs::File>,
    frame: usize,
    total_frames: u32,
    frame_duration_ms: u16,
    scene_frame: crate::offscreen::RenderedFrame,
) -> Result<(), ExportError> {
    let img = image::RgbaImage::from_raw(
        scene_frame.width,
        scene_frame.height,
        scene_frame.rgba.to_vec(),
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
}
