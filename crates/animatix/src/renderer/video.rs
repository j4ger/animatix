use super::offscreen::{OffscreenRenderer, RenderedFrame};
use crate::ast::Stmt;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use std::ffi::CString;

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
// Parallel frame rendering helper
// ----------------------------------------------------------------------------

/// Renders all frames in parallel using one thread per CPU core.
/// Each thread gets its own `OffscreenRenderer` (created sequentially on the
/// main thread to avoid concurrent wgpu adapter enumeration) and a cloned
/// `Timeline`.  Returns `Vec<RenderedFrame>` in strict frame order.
fn render_frames_in_parallel(
    timeline: &Timeline,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
    debug_options: DebugRenderOptions,
) -> Result<Vec<RenderedFrame>, ExportError> {
    if total_frames == 0 {
        return Ok(Vec::new());
    }

    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let chunk_size = ((total_frames as usize + num_cpus - 1) / num_cpus).max(1);
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

    let mut handles = Vec::with_capacity(num_chunks);
    for (chunk_idx, renderer) in renderers.into_iter().enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        let timeline = timeline.clone();

        handles.push(std::thread::spawn(move || -> Result<Vec<RenderedFrame>, ExportError> {
            let mut renderer = renderer;
            (start..end)
                .map(|frame| {
                    let time = (frame as f64) / (fps as f64);
                    renderer
                        .render_timeline_with_debug(
                            &timeline,
                            time,
                            SceneDimensions { width, height },
                            debug_options,
                        )
                        .map_err(|e| ExportError::FrameRender { frame, message: e })
                })
                .collect::<Result<Vec<_>, _>>()
        }));
    }

    let chunks: Vec<Vec<RenderedFrame>> = handles
        .into_iter()
        .map(|h| h.join().map_err(|_| ExportError::ThreadPanicked)?)
        .collect::<Result<Vec<_>, _>>()?;

    // Flatten while preserving frame order.
    Ok(chunks.into_iter().flatten().collect())
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
) -> Result<(), ExportError> {
    let total_frames = (duration * fps as f32).ceil() as u32;

    // ------------------------------------------------------------------------
    // 1. Parallel frame rendering
    // ------------------------------------------------------------------------
    let frames = render_frames_in_parallel(
        &timeline,
        width,
        height,
        fps,
        total_frames,
        debug_options,
    )?;

    // ------------------------------------------------------------------------
    // 2. Sequential video encoding
    // ------------------------------------------------------------------------
    println!("\nEncoding {} frames to video...", frames.len());

    let filename = CString::new(
        output_file
            .to_str()
            .ok_or_else(|| ExportError::VideoEncode("Invalid output path".into()))?,
    )?;
    let mut format_context =
        AVFormatContextOutput::create(&filename).map_err(|e| ExportError::VideoEncode(format!("{e:?}")))?;

    let encoder = AVCodec::find_encoder_by_name(&CString::new("libx264")?)
        .ok_or_else(|| ExportError::VideoEncode("Failed to find libx264 encoder".into()))?;
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

    encode_context
        .open(None)
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

    for (frame, scene_frame) in frames.into_iter().enumerate() {
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
    }

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
    ))
}

async fn render_image_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<(), ExportError> {
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
        timeline,
        width,
        height,
        time,
        output_file,
        debug_options,
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
) -> Result<(), ExportError> {
    use image::codecs::gif::{GifEncoder, Repeat};

    let total_frames = (duration * fps as f32).ceil() as u32;
    let frame_duration_ms = (1000 / fps) as u16;

    // ------------------------------------------------------------------------
    // 1. Parallel frame rendering
    // ------------------------------------------------------------------------
    let frames = render_frames_in_parallel(
        &timeline,
        width,
        height,
        fps,
        total_frames,
        debug_options,
    )?;

    // ------------------------------------------------------------------------
    // 2. Sequential GIF encoding
    // ------------------------------------------------------------------------
    println!("\nEncoding {} frames to GIF...", frames.len());

    let output = std::fs::File::create(output_file)?;
    let mut encoder = GifEncoder::new(output);
    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| ExportError::GifEncode(format!("{e:?}")))?;

    for (frame, scene_frame) in frames.into_iter().enumerate() {
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
    }

    println!("\nGIF render complete!");
    Ok(())
}
