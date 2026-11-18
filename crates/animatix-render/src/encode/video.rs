//! Video encoding (MP4/H.264, WebM/VP9) export.
//!
//! Provides:
//! - Encoder auto-detection (HW/SW)
//! - Adaptive thread count computation
//! - Single-timeline and multi-scene composition video encoding

use std::ffi::CString;

use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, Timeline};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVDictionary, AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use tracing::info;

use super::audio::AudioEncoder;
use crate::encode::{ExportError, ExportOptions, ExportSettings, VideoCodec};
use crate::render_pipeline::{
    fill_rgba_frame, render_frames_streaming, render_frames_streaming_composition,
};

/// Render a single timeline to an MP4 video file with default debug options and
/// export settings.
pub fn render_video_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_video_timeline_with_options(
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

/// Render a single timeline to an MP4 video file.
pub fn render_video_timeline_with_options(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    encode_video(
        VideoSource::Timeline(&timeline),
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        options,
    )
}

/// Render a multi-scene composition to an MP4 video file with default debug
/// options and export settings.
pub fn render_video_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_video_composition_with_options(
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

/// Render a multi-scene composition to an MP4 video file.
pub fn render_video_composition_with_options(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    encode_video(
        VideoSource::Composition(composition),
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
        options,
    )
}

/// Where a video's frames come from.
///
/// The two entry points differ only in the renderer they stream from and in
/// how their audio segments are gathered; everything else lives in
/// [`encode_video`].
enum VideoSource<'a> {
    Timeline(&'a Timeline),
    Composition(&'a Composition),
}

/// Encode `source` into one video file.
fn encode_video(
    source: VideoSource<'_>,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    if !duration.is_finite() || duration <= 0.0 || fps == 0 {
        return Err(ExportError::VideoEncode("Duration and FPS must be positive".into()));
    }
    let total_frames = (duration * fps as f32).ceil() as u32;
    let duration_s = f64::from(duration);
    let warn = |message| options.warn(message);
    let segments = match &source {
        VideoSource::Timeline(timeline) => {
            warn_caption_cutoff(timeline.caption_end_seconds(), duration_s, "", warn);
            timeline.audio_segments().to_vec()
        },
        VideoSource::Composition(composition) => {
            for (name, start) in &composition.scene_start_times {
                let Some(scene) = composition.scenes.get(name) else {
                    continue;
                };
                let caption_end = scene.timeline.caption_end_seconds();
                if caption_end > 0.0 {
                    warn_caption_cutoff(
                        start + caption_end,
                        (start + scene.duration_s).min(duration_s),
                        &format!(" in scene '{name}'"),
                        warn,
                    );
                }
            }
            composition.audio_segments()
        },
    };
    let parent = output_file.parent().filter(|path| !path.as_os_str().is_empty());
    let extension = output_file.extension().and_then(|ext| ext.to_str()).ok_or_else(|| {
        ExportError::VideoEncode("Output path must have a video file extension".into())
    })?;
    let suffix = format!(".{extension}");
    let temporary = tempfile::Builder::new()
        .prefix(".animatix-")
        .suffix(&suffix)
        .tempfile_in(parent.unwrap_or_else(|| std::path::Path::new(".")))?;
    let mut encoder = setup_video_encoder(temporary.path(), width, height, fps, &options.settings)?;
    encoder.audio = AudioEncoder::prepare(
        &mut encoder.format_context,
        &segments,
        duration_s,
        extension.eq_ignore_ascii_case("webm"),
        warn,
    )?;
    encoder.write_header()?;

    let num_threads = adaptive_thread_count(
        width,
        height,
        total_frames,
        false,
        encoder.is_hw_encoder,
        &options.settings,
    );
    info!("Encoding {total_frames} frames to video using {num_threads} render thread(s).");
    let push_frame = |frame, scene_frame| {
        encoder.push_frame(frame, total_frames, width, height, scene_frame)?;
        if let Some(audio) = &mut encoder.audio {
            let time_s = (frame + 1) as f64 / f64::from(fps);
            audio.advance(&mut encoder.format_context, time_s.min(duration_s))?;
        }
        Ok(())
    };

    match source {
        VideoSource::Timeline(timeline) => render_frames_streaming(
            timeline,
            width,
            height,
            fps,
            total_frames,
            num_threads,
            debug_options,
            options.progress,
            options.cancel,
            push_frame,
        )?,
        VideoSource::Composition(composition) => render_frames_streaming_composition(
            composition,
            width,
            height,
            fps,
            total_frames,
            num_threads,
            debug_options,
            options.progress,
            options.cancel,
            push_frame,
        )?,
    }

    encoder.finish()?;
    drop(encoder);
    if options
        .cancel
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    {
        return Err(ExportError::Cancelled);
    }
    temporary.persist(output_file).map_err(|err| err.error)?;
    info!("Render complete!");
    Ok(())
}

fn warn_caption_cutoff(end_s: f64, cutoff_s: f64, context: &str, warn: impl Fn(String)) {
    if end_s > cutoff_s + 0.001 {
        warn(format!(
            "Export will cut off captions{context} at {cutoff_s:.3}s; the last caption ends at {end_s:.3}s."
        ));
    }
}

/// Result of video encoder initialization.
/// Caller owns these fields for the frame encoding loop.
struct VideoEncoder {
    format_context: AVFormatContextOutput,
    audio: Option<AudioEncoder>,
    encode_context: AVCodecContext,
    sws_context: SwsContext,
    yuv_frame: AVFrame,
    stream_index: i32,
    stream_time_base: AVRational,
    is_hw_encoder: bool,
}

/// Initialize the video encoder, format context, and color converter.
fn setup_video_encoder(
    output_file: &std::path::Path,
    width: u32,
    height: u32,
    fps: u32,
    settings: &ExportSettings,
) -> Result<VideoEncoder, ExportError> {
    // WebM needs VP9; `Auto` means the portable H.264 default for everything else.
    let is_webm = output_file
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("webm"));
    let settings = if is_webm && matches!(settings.video_codec, VideoCodec::Auto) {
        ExportSettings {
            video_codec: VideoCodec::Vp9,
            ..*settings
        }
    } else {
        *settings
    };

    let filename = CString::new(
        output_file
            .to_str()
            .ok_or_else(|| ExportError::VideoEncode("Invalid output path".into()))?,
    )?;
    let mut format_context = AVFormatContextOutput::create(&filename)
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let (encoder, is_hw_encoder) = select_video_encoder(&settings)?;
    let mut encode_context = AVCodecContext::new(&encoder);

    encode_context.set_width(width as i32);
    encode_context.set_height(height as i32);
    encode_context.set_time_base(AVRational {
        num: 1,
        den: fps as i32,
    });
    encode_context.set_framerate(AVRational {
        num: fps as i32,
        den: 1,
    });
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

    let sws_context = SwsContext::get_context(
        width as i32,
        height as i32,
        rsmpeg::ffi::AV_PIX_FMT_RGBA,
        width as i32,
        height as i32,
        rsmpeg::ffi::AV_PIX_FMT_YUV420P,
        rsmpeg::ffi::SWS_FAST_BILINEAR,
        None,
        None,
        None,
    )
    .ok_or_else(|| ExportError::VideoEncode("Failed to create SWS context".into()))?;

    let mut yuv_frame = AVFrame::new();
    yuv_frame.set_format(rsmpeg::ffi::AV_PIX_FMT_YUV420P);
    yuv_frame.set_width(width as i32);
    yuv_frame.set_height(height as i32);
    yuv_frame
        .alloc_buffer()
        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let encode_context_time_base = encode_context.time_base;
    Ok(VideoEncoder {
        format_context,
        audio: None,
        encode_context,
        sws_context,
        yuv_frame,
        stream_index,
        stream_time_base: encode_context_time_base,
        is_hw_encoder,
    })
}

impl VideoEncoder {
    fn write_header(&mut self) -> Result<(), ExportError> {
        self.format_context
            .write_header(&mut None)
            .map_err(|err| ExportError::VideoEncode(format!("{err:?}")))?;
        self.stream_time_base = self.format_context.streams()[self.stream_index as usize].time_base;
        Ok(())
    }

    /// Convert one rendered RGBA frame, encode it, and write every packet the
    /// encoder has ready.
    fn push_frame(
        &mut self,
        frame: usize,
        total_frames: u32,
        width: u32,
        height: u32,
        scene_frame: crate::offscreen::RenderedFrame,
    ) -> Result<(), ExportError> {
        let mut rgba_frame = AVFrame::new();
        fill_rgba_frame(&mut rgba_frame, &scene_frame.rgba, width as i32, height as i32)
            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

        self.sws_context
            .scale_frame(&rgba_frame, 0, height as i32, &mut self.yuv_frame)
            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
        self.yuv_frame.set_pts(frame as i64);

        self.encode_context
            .send_frame(Some(&self.yuv_frame))
            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
        self.write_video_packets()?;

        use std::io::Write;
        print!("\rEncoding frame {}/{}", frame + 1, total_frames);
        std::io::stdout().flush()?;
        Ok(())
    }

    /// Drain pending packets and write the trailer.
    fn finish(&mut self) -> Result<(), ExportError> {
        self.encode_context
            .send_frame(None)
            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
        self.write_video_packets()?;
        if let Some(audio) = &mut self.audio {
            audio.finish(&mut self.format_context)?;
        }
        self.format_context
            .write_trailer()
            .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
        Ok(())
    }
    fn write_video_packets(&mut self) -> Result<(), ExportError> {
        loop {
            match self.encode_context.receive_packet() {
                Ok(mut packet) => {
                    if packet.duration <= 0 {
                        packet.set_duration(1);
                    }
                    packet.rescale_ts(self.encode_context.time_base, self.stream_time_base);
                    packet.set_stream_index(self.stream_index);
                    self.format_context
                        .interleaved_write_frame(&mut packet)
                        .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
                },
                Err(RsmpegError::EncoderDrainError | RsmpegError::EncoderFlushedError) => {
                    return Ok(());
                },
                Err(e) => return Err(ExportError::VideoEncode(format!("{e:?}"))),
            }
        }
    }
}

/// Compute an appropriate render thread count based on workload characteristics.
pub(crate) fn adaptive_thread_count(
    width: u32,
    height: u32,
    total_frames: u32,
    is_gif: bool,
    is_hw_encoder: bool,
    settings: &ExportSettings,
) -> usize {
    use crate::encode::MaxRenderThreads;
    match settings.max_render_threads {
        MaxRenderThreads::Fixed(n) => n.max(1),
        MaxRenderThreads::Auto => {
            let num_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);

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
        },
    }
}

/// Select a video encoder based on settings. Returns the encoder and whether it
/// is a hardware-accelerated encoder.
pub(crate) fn select_video_encoder(
    settings: &ExportSettings,
) -> Result<(rsmpeg::avcodec::AVCodecRef<'static>, bool), ExportError> {
    use crate::encode::VideoCodec;
    match settings.video_codec {
        VideoCodec::Libx264 => {
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264")?)
                .ok_or_else(|| ExportError::VideoEncode("Failed to find libx264 encoder".into()))?;
            Ok((codec, false))
        },
        VideoCodec::H264Nvenc => {
            let codec =
                AVCodec::find_encoder_by_name(&CString::new("h264_nvenc")?).ok_or_else(|| {
                    ExportError::VideoEncode("Failed to find h264_nvenc encoder".into())
                })?;
            Ok((codec, true))
        },
        VideoCodec::H264Vaapi => {
            let codec =
                AVCodec::find_encoder_by_name(&CString::new("h264_vaapi")?).ok_or_else(|| {
                    ExportError::VideoEncode("Failed to find h264_vaapi encoder".into())
                })?;
            Ok((codec, true))
        },
        VideoCodec::Auto => {
            // Deliberately software-first: hardware encoders may be *registered*
            // by the FFmpeg build yet fail to initialize at open time (nvenc
            // without a CUDA driver), and find_encoder cannot detect that. An
            // explicit `--codec h264_nvenc`/`h264_vaapi` opts into hardware.
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264")?)
                .ok_or_else(|| ExportError::VideoEncode("Failed to find libx264 encoder".into()))?;
            info!("Auto-selected software encoder: libx264");
            Ok((codec, false))
        },
        VideoCodec::Vp9 => {
            let codec =
                AVCodec::find_encoder_by_name(&CString::new("libvpx-vp9")?).ok_or_else(|| {
                    ExportError::VideoEncode("Failed to find libvpx-vp9 encoder".into())
                })?;
            Ok((codec, false))
        },
    }
}
