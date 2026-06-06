//! Video encoding (MP4/H.264) export.
//!
//! Provides:
//! - Encoder auto-detection (HW/SW)
//! - Adaptive thread count computation
//! - Single-timeline and multi-scene composition video encoding
//! - Public wrapper functions with varying levels of control

use crate::ast::Stmt;
use crate::composition::Composition;
use crate::renderer::encode::{
    mux_audio_segments,
    ExportError, ExportSettings,
};
use crate::renderer::render_pipeline::{fill_rgba_frame, render_frames_streaming, render_frames_streaming_composition};
use crate::timeline::{AudioSegment, DebugRenderOptions, Timeline};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVDictionary, AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32};
use tracing::info;

// ---------------------------------------------------------------------------
// Public API: single-timeline video
// ---------------------------------------------------------------------------

/// Render AST statements directly to an MP4 video file.
pub fn render_video(
    ast: &[Stmt],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    pollster::block_on(render_video_async(
        Timeline::build(ast),
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
        ExportSettings::default(),
        None,
        None,
    ))
}

/// Render a single timeline to an MP4 video file.
pub fn render_video_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_video_timeline_with_debug(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
    )
}

/// Render a single timeline to an MP4 video file with debug options.
pub fn render_video_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<(), ExportError> {
    pollster::block_on(render_video_async(
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

/// Render a single timeline to an MP4 video file with debug options and export settings.
pub fn render_video_timeline_with_settings(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
) -> Result<(), ExportError> {
    pollster::block_on(render_video_async(
        timeline, width, height, fps, duration, output_file, debug_options, settings, None, None,
    ))
}

/// Render a single timeline to an MP4 video file with full control: debug options,
/// export settings, progress tracking, and cancellation.
pub fn render_video_timeline_with_progress(
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
    pollster::block_on(render_video_async(
        timeline, width, height, fps, duration, output_file, debug_options, settings, progress, cancel,
    ))
}

// ---------------------------------------------------------------------------
// Shared encoder setup helper
// ---------------------------------------------------------------------------

/// Result of video encoder initialization.
/// Caller owns these fields for the frame encoding loop.
struct VideoEncoderParts {
    format_context: AVFormatContextOutput,
    encode_context: AVCodecContext,
    sws_context: SwsContext,
    yuv_frame: AVFrame,
    stream_index: i32,
    stream_time_base: AVRational,
    is_hw_encoder: bool,
}

/// Initialize the video encoder, format context, and color converter.
/// Returns the parts needed for frame encoding.
fn setup_video_encoder(
    output_file: &std::path::Path,
    width: u32,
    height: u32,
    fps: u32,
    settings: &ExportSettings,
) -> Result<VideoEncoderParts, ExportError> {
    let filename = CString::new(
        output_file
            .to_str()
            .ok_or_else(|| ExportError::VideoEncode("Invalid output path".into()))?,
    )?;
    let mut format_context = AVFormatContextOutput::create(&filename)
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let (encoder, is_hw_encoder) = select_video_encoder(settings)?;
    let mut encode_context = AVCodecContext::new(&encoder);

    encode_context.set_width(width as i32);
    encode_context.set_height(height as i32);
    encode_context.set_time_base(AVRational { num: 1, den: fps as i32 });
    encode_context.set_framerate(AVRational { num: fps as i32, den: 1 });
    encode_context.set_pix_fmt(rsmpeg::ffi::AV_PIX_FMT_YUV420P);

    if format_context.oformat().flags & rsmpeg::ffi::AVFMT_GLOBALHEADER as i32 != 0 {
        encode_context
            .set_flags(encode_context.flags | rsmpeg::ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
    }

    let dict = if !is_hw_encoder {
        Some(AVDictionary::new(
            &CString::new("preset")?,
            &CString::new(settings.h264_preset.as_str())?,
            0,
        ))
    } else {
        None
    };

    encode_context
        .open(dict)
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let stream_index;
    {
        let mut stream = format_context.new_stream();
        stream.set_time_base(encode_context.time_base);
        stream.set_codecpar(encode_context.extract_codecpar());
        stream_index = stream.index;
    }

    format_context
        .write_header(&mut None)
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let stream_time_base = format_context.streams()[stream_index as usize].time_base;

    let sws_context = SwsContext::get_context(
        width as i32, height as i32, rsmpeg::ffi::AV_PIX_FMT_RGBA,
        width as i32, height as i32, rsmpeg::ffi::AV_PIX_FMT_YUV420P,
        rsmpeg::ffi::SWS_FAST_BILINEAR, None, None, None,
    )
    .ok_or_else(|| ExportError::VideoEncode("Failed to create SWS context".into()))?;

    let mut yuv_frame = AVFrame::new();
    yuv_frame.set_format(rsmpeg::ffi::AV_PIX_FMT_YUV420P);
    yuv_frame.set_width(width as i32);
    yuv_frame.set_height(height as i32);
    yuv_frame
        .alloc_buffer()
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    Ok(VideoEncoderParts {
        format_context,
        encode_context,
        sws_context,
        yuv_frame,
        stream_index,
        stream_time_base,
        is_hw_encoder,
    })
}

/// Drain pending packets and write trailer.
fn finish_video_encoder(
    encode_context: &mut AVCodecContext,
    format_context: &mut AVFormatContextOutput,
    stream_time_base: AVRational,
    stream_index: i32,
) -> Result<(), ExportError> {
    encode_context
        .send_frame(None)
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
    loop {
        match encode_context.receive_packet() {
            Ok(mut packet) => {
                packet.rescale_ts(encode_context.time_base, stream_time_base);
                packet.set_stream_index(stream_index);
                format_context
                    .interleaved_write_frame(&mut packet)
                    .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            }
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => return Err(ExportError::VideoEncode(format!("{e:?}"))),
        }
    }
    format_context
        .write_trailer()
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
    Ok(())
}

/// Collect audio segments and mux into the output file.
fn mux_audio_if_present(
    audio_segments: &[AudioSegment],
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    if !audio_segments.is_empty() {
        super::require_ffmpeg()?;
        mux_audio_segments(output_file, audio_segments, output_file)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal async video encoder
// ---------------------------------------------------------------------------

pub(super) async fn render_video_async(
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
    let total_frames = (duration * fps as f32).ceil() as u32;
    info!("Encoding {} frames to video...", total_frames);

    let VideoEncoderParts {
        mut format_context,
        mut encode_context,
        mut sws_context,
        mut yuv_frame,
        stream_index,
        stream_time_base,
        is_hw_encoder,
    } = setup_video_encoder(output_file, width, height, fps, &settings)?;

    // ------------------------------------------------------------------------
    // 2. Parallel frame rendering with streaming to encoder
    // ------------------------------------------------------------------------
    let num_threads = adaptive_thread_count(width, height, total_frames, false, is_hw_encoder, &settings);
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
            let mut rgba_frame = AVFrame::new();
            fill_rgba_frame(&mut rgba_frame, &scene_frame.rgba, width as i32, height as i32)
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

            sws_context
                .scale_frame(&rgba_frame, 0, height as i32, &mut yuv_frame)
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            yuv_frame.set_pts(frame as i64);

            encode_context
                .send_frame(Some(&yuv_frame))
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            loop {
                match encode_context.receive_packet() {
                    Ok(mut packet) => {
                        packet.rescale_ts(encode_context.time_base, stream_time_base);
                        packet.set_stream_index(stream_index);
                        format_context
                            .interleaved_write_frame(&mut packet)
                            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
                    }
                    Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => {
                        break;
                    }
                    Err(e) => return Err(ExportError::VideoEncode(format!("{e:?}"))),
                }
            }

            use std::io::Write;
            print!("\rEncoding frame {}/{}", frame + 1, total_frames);
            std::io::stdout().flush()?;
            Ok(())
        },
    )?;

    finish_video_encoder(&mut encode_context, &mut format_context, stream_time_base, stream_index)?;
    mux_audio_if_present(&timeline.audio_segments, output_file)?;

    info!("Render complete!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-Scene Composition Video Export
// ---------------------------------------------------------------------------

/// Render a multi-scene composition to an MP4 video file.
pub fn render_video_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_video_composition_with_settings(
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

/// Render a multi-scene composition to an MP4 video file with debug options and export settings.
pub fn render_video_composition_with_settings(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    settings: ExportSettings,
) -> Result<(), ExportError> {
    pollster::block_on(render_video_composition_async(
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

/// Render a multi-scene composition to an MP4 video file with full control: debug options,
/// export settings, progress tracking, and cancellation.
pub fn render_video_composition_with_progress(
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
    pollster::block_on(render_video_composition_async(
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

pub(super) async fn render_video_composition_async(
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
    let total_frames = (duration * fps as f32).ceil() as u32;
    info!("Encoding {} frames to video...", total_frames);

    let VideoEncoderParts {
        mut format_context,
        mut encode_context,
        mut sws_context,
        mut yuv_frame,
        stream_index,
        stream_time_base,
        is_hw_encoder,
    } = setup_video_encoder(output_file, width, height, fps, &settings)?;

    let num_threads =
        adaptive_thread_count(width, height, total_frames, false, is_hw_encoder, &settings);
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
            let mut rgba_frame = AVFrame::new();
            fill_rgba_frame(&mut rgba_frame, &scene_frame.rgba, width as i32, height as i32)
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

            sws_context
                .scale_frame(&rgba_frame, 0, height as i32, &mut yuv_frame)
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            yuv_frame.set_pts(frame as i64);

            encode_context
                .send_frame(Some(&yuv_frame))
                .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            loop {
                match encode_context.receive_packet() {
                    Ok(mut packet) => {
                        packet.rescale_ts(encode_context.time_base, stream_time_base);
                        packet.set_stream_index(stream_index);
                        format_context
                            .interleaved_write_frame(&mut packet)
                            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
                    }
                    Err(RsmpegError::EncoderDrainError)
                    | Err(RsmpegError::EncoderFlushedError) => {
                        break;
                    }
                    Err(e) => return Err(ExportError::VideoEncode(format!("{e:?}"))),
                }
            }

            use std::io::Write;
            print!("\rEncoding frame {}/{}", frame + 1, total_frames);
            std::io::stdout().flush()?;
            Ok(())
        },
    )?;

    finish_video_encoder(&mut encode_context, &mut format_context, stream_time_base, stream_index)?;

    // Mux audio segments from all scenes in the composition
    let audio_segments: Vec<AudioSegment> = composition
        .scenes
        .values()
        .flat_map(|s| s.timeline.audio_segments.clone())
        .collect();
    mux_audio_if_present(&audio_segments, output_file)?;

    info!("Render complete!");
    Ok(())
}

// ---------------------------------------------------------------------------
// Adaptive parallelism
// ---------------------------------------------------------------------------

/// Compute an appropriate render thread count based on workload characteristics.
pub(crate) fn adaptive_thread_count(
    width: u32,
    height: u32,
    total_frames: u32,
    is_gif: bool,
    is_hw_encoder: bool,
    settings: &ExportSettings,
) -> usize {
    use crate::renderer::encode::MaxRenderThreads;
    match settings.max_render_threads {
        MaxRenderThreads::Fixed(n) => n.max(1),
        MaxRenderThreads::Auto => {
            let num_cpus = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);

            // Format-specific base caps.
            // Encoding is the bottleneck for software; hardware encoding is fast.
            let format_cap = if is_gif {
                2
            } else if is_hw_encoder {
                num_cpus.min(8)
            } else {
                4
            };

            // Resolution scaling: larger frames = more GPU memory per thread.
            let pixels = (width as u64) * (height as u64);
            let resolution_cap = if pixels > 3840 * 2160 {
                if is_hw_encoder { 2 } else { 1 }
            } else if pixels > 1920 * 1080 {
                if is_hw_encoder { num_cpus } else { 2 }
            } else {
                num_cpus
            };

            // Short content: thread overhead dominates.
            let duration_cap = if total_frames < 30 { 1 } else { num_cpus };

            format_cap.min(resolution_cap).min(duration_cap).max(1)
        }
    }
}

// ---------------------------------------------------------------------------
// Encoder selection
// ---------------------------------------------------------------------------

/// Select a video encoder based on settings. Returns the encoder and whether it
/// is a hardware-accelerated encoder.
pub(crate) fn select_video_encoder(
    settings: &ExportSettings,
) -> Result<(rsmpeg::avcodec::AVCodecRef<'static>, bool), ExportError> {
    use crate::renderer::encode::VideoCodec;
    match settings.video_codec {
        VideoCodec::Libx264 => {
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find libx264 encoder".into()),
            )?;
            Ok((codec, false))
        }
        VideoCodec::H264Nvenc => {
            let codec = AVCodec::find_encoder_by_name(&CString::new("h264_nvenc")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find h264_nvenc encoder".into()),
            )?;
            Ok((codec, true))
        }
        VideoCodec::H264Vaapi => {
            let codec = AVCodec::find_encoder_by_name(&CString::new("h264_vaapi")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find h264_vaapi encoder".into()),
            )?;
            Ok((codec, true))
        }
        VideoCodec::Auto => {
            for (name, is_hw) in [("h264_nvenc", true), ("h264_vaapi", true)] {
                if let Some(codec) = AVCodec::find_encoder_by_name(&CString::new(name)?) {
                    info!("Auto-selected hardware encoder: {name}");
                    return Ok((codec, is_hw));
                }
            }
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find libx264 encoder".into()),
            )?;
            info!("Auto-selected software encoder: libx264");
            Ok((codec, false))
        }
        VideoCodec::Vp9 => {
            let codec = AVCodec::find_encoder_by_name(&CString::new("libvpx-vp9")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find libvpx-vp9 encoder".into()),
            )?;
            Ok((codec, false))
        }
    }
}