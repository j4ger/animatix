use super::offscreen::{OffscreenRenderer, RenderedFrame};
use crate::ast::Stmt;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use std::ffi::CString;

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
) -> Vec<RenderedFrame> {
    if total_frames == 0 {
        return Vec::new();
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
        renderers.push(
            OffscreenRenderer::new().expect("Failed to create offscreen renderer"),
        );
    }

    let mut handles = Vec::with_capacity(num_chunks);
    for (chunk_idx, renderer) in renderers.into_iter().enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        let timeline = timeline.clone();

        handles.push(std::thread::spawn(move || {
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
                        .expect("Failed to render offscreen frame")
                })
                .collect::<Vec<_>>()
        }));
    }

    let chunks: Vec<Vec<RenderedFrame>> = handles
        .into_iter()
        .map(|h| h.join().expect("Render thread panicked"))
        .collect();

    // Flatten while preserving frame order.
    chunks.into_iter().flatten().collect()
}

// ----------------------------------------------------------------------------

pub fn render_video(
    ast: &[Stmt],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) {
    pollster::block_on(render_video_async(
        Timeline::build(ast),
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
    ));
}

async fn render_video_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
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
    );

    // ------------------------------------------------------------------------
    // 2. Sequential video encoding
    // ------------------------------------------------------------------------
    println!("\nEncoding {} frames to video...", frames.len());

    let filename = CString::new(output_file.to_str().unwrap()).unwrap();
    let mut format_context = AVFormatContextOutput::create(&filename).unwrap();

    let encoder = AVCodec::find_encoder_by_name(&CString::new("libx264").unwrap()).unwrap();
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

    encode_context.open(None).unwrap();

    let stream_index;
    {
        let mut stream = format_context.new_stream();
        stream.set_time_base(encode_context.time_base);
        stream.set_codecpar(encode_context.extract_codecpar());
        stream_index = stream.index;
    }

    format_context.write_header(&mut None).unwrap();

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
    .unwrap();

    let mut yuv_frame = AVFrame::new();
    yuv_frame.set_format(rsmpeg::ffi::AV_PIX_FMT_YUV420P);
    yuv_frame.set_width(width as i32);
    yuv_frame.set_height(height as i32);
    yuv_frame.alloc_buffer().unwrap();

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
                .unwrap();
        }

        sws_context
            .scale_frame(&rgba_frame, 0, height as i32, &mut yuv_frame)
            .unwrap();
        yuv_frame.set_pts(frame as i64);

        encode_context.send_frame(Some(&yuv_frame)).unwrap();
        loop {
            match encode_context.receive_packet() {
                Ok(mut packet) => {
                    packet.rescale_ts(encode_context.time_base, stream_time_base);
                    packet.set_stream_index(stream_index);
                    format_context.interleaved_write_frame(&mut packet).unwrap();
                }
                Err(RsmpegError::EncoderDrainError)
                | Err(RsmpegError::EncoderFlushedError) => {
                    break;
                }
                Err(e) => panic!("Encoding error: {:?}", e),
            }
        }

        use std::io::Write;
        print!("\rEncoding frame {}/{}", frame + 1, total_frames);
        std::io::stdout().flush().unwrap();
    }

    encode_context.send_frame(None).unwrap();
    loop {
        match encode_context.receive_packet() {
            Ok(mut packet) => {
                packet.rescale_ts(encode_context.time_base, stream_time_base);
                packet.set_stream_index(stream_index);
                format_context.interleaved_write_frame(&mut packet).unwrap();
            }
            Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => break,
            Err(e) => panic!("Encoding error: {:?}", e),
        }
    }
    format_context.write_trailer().unwrap();

    println!("\nRender complete!");
}

pub fn render_image(
    ast: &[Stmt],
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) {
    pollster::block_on(render_image_async(
        Timeline::build(ast),
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
    ));
}

async fn render_image_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
    let mut renderer = OffscreenRenderer::new().expect("Failed to create offscreen renderer");
    let frame = renderer
        .render_timeline_with_debug(
            &timeline,
            time as f64,
            SceneDimensions { width, height },
            debug_options,
        )
        .expect("Failed to render offscreen frame");
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        .expect("Failed to create image buffer from offscreen frame");
    img.save(output_file).unwrap();
}

pub fn render_video_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) {
    render_video_timeline_with_debug(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
    );
}

pub fn render_video_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
    pollster::block_on(render_video_async(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
    ));
}

pub fn render_image_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) {
    render_image_timeline_with_debug(
        timeline,
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
    );
}

pub fn render_image_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
    pollster::block_on(render_image_async(
        timeline,
        width,
        height,
        time,
        output_file,
        debug_options,
    ));
}

pub fn render_gif_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) {
    render_gif_timeline_with_debug(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        DebugRenderOptions::default(),
    );
}

pub fn render_gif_timeline_with_debug(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
    pollster::block_on(render_gif_async(
        timeline,
        width,
        height,
        fps,
        duration,
        output_file,
        debug_options,
    ));
}

async fn render_gif_async(
    timeline: Timeline,
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) {
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
    );

    // ------------------------------------------------------------------------
    // 2. Sequential GIF encoding
    // ------------------------------------------------------------------------
    println!("\nEncoding {} frames to GIF...", frames.len());

    let output = std::fs::File::create(output_file).expect("Failed to create GIF file");
    let mut encoder = GifEncoder::new(output);
    encoder
        .set_repeat(Repeat::Infinite)
        .expect("Failed to set GIF repeat");

    for (frame, scene_frame) in frames.into_iter().enumerate() {
        let img = image::RgbaImage::from_raw(
            scene_frame.width,
            scene_frame.height,
            scene_frame.rgba,
        )
        .expect("Failed to create image buffer from offscreen frame");

        encoder
            .encode_frame(image::Frame::from_parts(
                img,
                0,
                0,
                image::Delay::from_saturating_duration(std::time::Duration::from_millis(
                    frame_duration_ms as u64,
                )),
            ))
            .expect("Failed to encode GIF frame");

        use std::io::Write;
        print!("\rEncoding GIF frame {}/{}", frame + 1, total_frames);
        std::io::stdout().flush().unwrap();
    }

    println!("\nGIF render complete!");
}
