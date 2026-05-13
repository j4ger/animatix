use super::offscreen::{OffscreenRenderer, RenderedFrame};
use crate::ast::Stmt;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVDictionary, AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Debug)]
pub enum ExportError {
    RendererCreation(String),
    FrameRender { frame: usize, message: String },
    ImageEncode(String),
    ImageSave(std::io::Error),
    VideoEncode(String),
    GifEncode(String),
    InvalidPath(std::ffi::NulError),
    ThreadPanicked,
    Cancelled,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RendererCreation(msg) => write!(f, "Failed to create renderer: {msg}"),
            Self::FrameRender { frame, message } => {
                write!(f, "Failed to render frame {frame}: {message}")
            }
            Self::ImageEncode(msg) => write!(f, "Image encoding error: {msg}"),
            Self::ImageSave(err) => write!(f, "Failed to save image: {err}"),
            Self::VideoEncode(msg) => write!(f, "Video encoding error: {msg}"),
            Self::GifEncode(msg) => write!(f, "GIF encoding error: {msg}"),
            Self::InvalidPath(_) => write!(f, "Output path contains null bytes"),
            Self::ThreadPanicked => write!(f, "Render thread panicked"),
            Self::Cancelled => write!(f, "Export cancelled by user"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(err: std::io::Error) -> Self {
        Self::ImageSave(err)
    }
}

impl From<std::ffi::NulError> for ExportError {
    fn from(err: std::ffi::NulError) -> Self {
        Self::InvalidPath(err)
    }
}

// ----------------------------------------------------------------------------
// Export settings
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct ExportSettings {
    /// Render thread limit. `Auto` picks based on format, resolution, and duration.
    pub max_render_threads: MaxRenderThreads,
    /// Video encoder selection. `Auto` probes hardware first.
    pub video_codec: VideoCodec,
    /// libx264 quality-speed preset. Ignored for hardware encoders.
    pub h264_preset: H264Preset,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            max_render_threads: MaxRenderThreads::Auto,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MaxRenderThreads {
    Auto,
    Fixed(usize),
}

impl std::fmt::Display for MaxRenderThreads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Fixed(n) => write!(f, "{n}"),
        }
    }
}

impl std::str::FromStr for MaxRenderThreads {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(Self::Auto)
        } else {
            let n = s.parse::<usize>().map_err(|e| format!("Invalid thread count: {e}"))?;
            Ok(Self::Fixed(n))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VideoCodec {
    Auto,
    Libx264,
    H264Nvenc,
    H264Vaapi,
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Libx264 => write!(f, "libx264"),
            Self::H264Nvenc => write!(f, "h264_nvenc"),
            Self::H264Vaapi => write!(f, "h264_vaapi"),
        }
    }
}

impl std::str::FromStr for VideoCodec {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "libx264" => Ok(Self::Libx264),
            "h264_nvenc" | "nvenc" => Ok(Self::H264Nvenc),
            "h264_vaapi" | "vaapi" => Ok(Self::H264Vaapi),
            _ => Err(format!(
                "Unknown codec: {s}. Expected: auto, libx264, h264_nvenc, h264_vaapi"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum H264Preset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl H264Preset {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

impl std::fmt::Display for H264Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for H264Preset {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ultrafast" => Ok(Self::Ultrafast),
            "superfast" => Ok(Self::Superfast),
            "veryfast" => Ok(Self::Veryfast),
            "faster" => Ok(Self::Faster),
            "fast" => Ok(Self::Fast),
            "medium" => Ok(Self::Medium),
            "slow" => Ok(Self::Slow),
            "slower" => Ok(Self::Slower),
            "veryslow" => Ok(Self::Veryslow),
            _ => Err(format!(
                "Unknown preset: {s}. Expected: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow"
            )),
        }
    }
}

// ----------------------------------------------------------------------------
// Adaptive parallelism
// ----------------------------------------------------------------------------

/// Compute an appropriate render thread count based on workload characteristics.
fn adaptive_thread_count(
    width: u32,
    height: u32,
    total_frames: u32,
    is_gif: bool,
    is_hw_encoder: bool,
    settings: &ExportSettings,
) -> usize {
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

// ----------------------------------------------------------------------------
// Encoder selection
// ----------------------------------------------------------------------------

/// Select a video encoder based on settings. Returns the encoder and whether it
/// is a hardware-accelerated encoder.
fn select_video_encoder(
    settings: &ExportSettings,
) -> Result<(rsmpeg::avcodec::AVCodecRef<'static>, bool), ExportError> {
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
                    println!("Auto-selected hardware encoder: {name}");
                    return Ok((codec, is_hw));
                }
            }
            let codec = AVCodec::find_encoder_by_name(&CString::new("libx264")?).ok_or_else(
                || ExportError::VideoEncode("Failed to find libx264 encoder".into()),
            )?;
            println!("Auto-selected software encoder: libx264");
            Ok((codec, false))
        }
    }
}

// ----------------------------------------------------------------------------
// Streaming parallel frame rendering helper
// ----------------------------------------------------------------------------

/// Renders frames in parallel using up to `num_threads` workers, but feeds them
/// to `process_frame` in strict sequential order.  A small bounded channel per
/// chunk limits in-flight frames so memory stays O(num_threads × frame_size)
/// rather than O(total_frames × frame_size).
fn render_frames_streaming<F>(
    timeline: &Timeline,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
    num_threads: usize,
    debug_options: DebugRenderOptions,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
    mut process_frame: F,
) -> Result<(), ExportError>
where
    F: FnMut(usize, RenderedFrame) -> Result<(), ExportError>,
{
    if total_frames == 0 {
        return Ok(());
    }

    let num_threads = num_threads.max(1);
    let chunk_size = ((total_frames as usize + num_threads - 1) / num_threads).max(1);
    let num_chunks = (total_frames as usize + chunk_size - 1) / chunk_size;

    println!(
        "Rendering {} frames using {} thread(s) ({} frame(s) per chunk)...",
        total_frames, num_chunks, chunk_size
    );

    // Create one renderer per chunk on the main thread.
    // Concurrent wgpu adapter enumeration from multiple threads can be flaky
    // on some drivers, so we stagger creation here.
    let mut renderers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        renderers.push(OffscreenRenderer::new().map_err(ExportError::RendererCreation)?);
    }

    // Bounded channels limit memory to a few frames per chunk.
    const CHANNEL_CAPACITY: usize = 2;
    let mut senders = Vec::with_capacity(num_chunks);
    let mut receivers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        let (tx, rx) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        senders.push(tx);
        receivers.push(rx);
    }

    let mut handles = Vec::with_capacity(num_chunks);
    for (chunk_idx, (renderer, sender)) in renderers.into_iter().zip(senders.into_iter()).enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        let timeline = timeline.clone();

        handles.push(std::thread::spawn(move || -> Result<(), ExportError> {
            let mut renderer = renderer;
            for frame in start..end {
                let time = (frame as f64) / (fps as f64);
                let rendered = renderer
                    .render_timeline_with_debug(
                        &timeline,
                        time,
                        SceneDimensions { width, height },
                        debug_options,
                    )
                    .map_err(|e| ExportError::FrameRender { frame, message: e })?;
                sender
                    .send(rendered)
                    .map_err(|_| ExportError::ThreadPanicked)?;
            }
            Ok(())
        }));
    }

    // Consume chunks in strict order so the encoder receives frames sequentially.
    for (chunk_idx, receiver) in receivers.into_iter().enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        for frame in start..end {
            if cancel.map_or(false, |c| c.load(Ordering::Relaxed)) {
                return Err(ExportError::Cancelled);
            }
            let rendered = receiver.recv().map_err(|_| ExportError::ThreadPanicked)?;
            process_frame(frame, rendered)?;
            if let Some(p) = progress {
                p.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Threads should have finished because we drained every receiver, but join
    // to be safe and to propagate any panics.
    for handle in handles {
        handle.join().map_err(|_| ExportError::ThreadPanicked)??;
    }

    Ok(())
}

// ----------------------------------------------------------------------------

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

async fn render_video_async(
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

    // ------------------------------------------------------------------------
    // 1. Encoder setup (before streaming so we know if HW accel is available)
    // ------------------------------------------------------------------------
    println!("\nEncoding {} frames to video...", total_frames);

    let filename = CString::new(
        output_file
            .to_str()
            .ok_or_else(|| ExportError::VideoEncode("Invalid output path".into()))?,
    )?;
    let mut format_context =
        AVFormatContextOutput::create(&filename).map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

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

    // Apply libx264 preset via dictionary. Hardware encoder presets are
    // codec-specific and left at default for now.
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

    let mut sws_context = SwsContext::get_context(
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

    // ------------------------------------------------------------------------
    // 2. Parallel frame rendering with streaming to encoder
    // ------------------------------------------------------------------------
    let num_threads = adaptive_thread_count(width, height, total_frames, false, is_hw_encoder, &settings);
    println!("Using {num_threads} render thread(s) (adaptive).");

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
            let data_ptr = scene_frame.rgba.as_ptr() as *mut u8;

            unsafe {
                rgba_frame
                    .fill_arrays(
                        data_ptr,
                        rsmpeg::ffi::AV_PIX_FMT_RGBA,
                        width as i32,
                        height as i32,
                    )
                    .map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;
            }

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

    println!("\nRender complete!");
    Ok(())
}

pub fn render_image(
    ast: &[Stmt],
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

async fn render_image_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    _progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
) -> Result<(), ExportError> {
    if cancel.map_or(false, |c| c.load(Ordering::Relaxed)) {
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
    Ok(())
}

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

async fn render_gif_async(
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
    println!("\nEncoding {} frames to GIF...", total_frames);

    let output = std::fs::File::create(output_file)?;
    let mut encoder = GifEncoder::new(output);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    let num_threads = adaptive_thread_count(width, height, total_frames, true, false, &settings);
    println!("Using {num_threads} render thread(s) (adaptive).");

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

    println!("\nGIF render complete!");
    Ok(())
}
