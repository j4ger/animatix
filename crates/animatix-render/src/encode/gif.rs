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

/// Render a single timeline to a GIF file with default debug options and export
/// settings.
pub fn render_gif_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f64,
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
    duration: f64,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    encode_gif(
        GifSource::Timeline(&timeline),
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        options,
    )
}

/// Render a multi-scene composition to a GIF file with default debug options
/// and export settings.
pub fn render_gif_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f64,
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
    duration: f64,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    encode_gif(
        GifSource::Composition(composition),
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        options,
    )
}

/// Where a GIF's frames come from.
///
/// The two entry points differ only in the renderer they stream from; the
/// encoder prologue, thread budget, and epilogue live in [`encode_gif`].
enum GifSource<'a> {
    Timeline(&'a Timeline),
    Composition(&'a Composition),
}

/// Encode `source` into one GIF file.
fn encode_gif(
    source: GifSource<'_>,
    width: u32,
    height: u32,
    fps: u32,
    duration: f64,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    let ExportOptions {
        settings,
        progress,
        cancel,
        ..
    } = options;
    let total_frames = super::video_chapters::video_frame_count(duration, fps)
        .map_err(|_| ExportError::GifEncode("Duration and FPS must be positive".into()))?;

    info!("Encoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = image::codecs::gif::GifEncoder::new(output);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    let num_threads = adaptive_thread_count(width, height, total_frames, true, false, &settings);
    info!("Using {num_threads} render thread(s) (adaptive).");

    let encode_frame = |frame, scene_frame| {
        let delay = gif_delay(frame as u32, fps);
        encode_gif_frame(&mut encoder, frame, total_frames, delay, scene_frame)
    };

    match source {
        GifSource::Timeline(timeline) => render_frames_streaming(
            timeline,
            width,
            height,
            fps,
            total_frames,
            num_threads,
            debug_options,
            progress,
            cancel,
            encode_frame,
        )?,
        GifSource::Composition(composition) => render_frames_streaming_composition(
            composition,
            width,
            height,
            fps,
            total_frames,
            num_threads,
            debug_options,
            progress,
            cancel,
            encode_frame,
        )?,
    }

    info!("GIF render complete!");
    Ok(())
}

/// GIF stores integer centiseconds. Quantize the frame boundaries, not each
/// delay in isolation, so rounding error does not build up across the file.
fn gif_delay(frame: u32, fps: u32) -> image::Delay {
    let centiseconds =
        |frame| (animatix::time::frame_seconds(frame, f64::from(fps)) * 100.0).round() as u64;
    let delay_cs = (centiseconds(frame + 1) - centiseconds(frame)).max(1);
    image::Delay::from_numer_denom_ms((delay_cs * 10) as u32, 1)
}

/// Append one rendered frame to the GIF and report progress on stdout.
fn encode_gif_frame(
    encoder: &mut image::codecs::gif::GifEncoder<std::fs::File>,
    frame: usize,
    total_frames: u32,
    delay: image::Delay,
    scene_frame: crate::offscreen::RenderedFrame,
) -> Result<(), ExportError> {
    let img = image::RgbaImage::from_raw(
        scene_frame.width,
        scene_frame.height,
        scene_frame.rgba.to_vec(),
    )
    .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;

    encoder
        .encode_frame(image::Frame::from_parts(img, 0, 0, delay))
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    use std::io::Write;
    print!("\rEncoding GIF frame {}/{}", frame + 1, total_frames);
    std::io::stdout().flush()?;
    Ok(())
}
