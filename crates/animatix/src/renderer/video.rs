use super::core::RendererCore;
use crate::ast::Stmt;
use crate::timeline::Timeline;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextOutput;
use rsmpeg::avutil::{AVFrame, AVRational};
use rsmpeg::error::RsmpegError;
use rsmpeg::swscale::SwsContext;
use std::ffi::CString;

pub fn render_video(
    ast: &[Stmt],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) {
    pollster::block_on(render_video_async(
        ast,
        width,
        height,
        fps,
        duration,
        output_file,
    ));
}

async fn render_video_async(
    ast: &[Stmt],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
    output_file: &std::path::Path,
) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .expect("Failed to create device");

    let bytes_per_row = (width * 4 + 255) & !255;
    let texture_desc = wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::STORAGE_BINDING,
        label: Some("Output Texture"),
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let output_buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    });

    let mut core = RendererCore::new(&device, &queue);

    let timeline = Timeline::build(ast);

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

    let total_frames = (duration * fps as f32).ceil() as u32;

    for frame in 0..total_frames {
        let scene = timeline.evaluate((frame as f64) / (fps as f64));

        core.render_vello_scene(&device, &queue, &texture_view, width, height, &scene);

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();

        {
            let data = buffer_slice.get_mapped_range();

            let mut rgba_frame = AVFrame::new();
            let data_ptr = data.as_ptr() as *mut u8;

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
                    Err(RsmpegError::EncoderDrainError) | Err(RsmpegError::EncoderFlushedError) => {
                        break;
                    }
                    Err(e) => panic!("Encoding error: {:?}", e),
                }
            }
        }
        output_buffer.unmap();

        use std::io::Write;
        print!("\rRendering frame {}/{}", frame + 1, total_frames);
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
    pollster::block_on(render_image_async(ast, width, height, time, output_file));
}

async fn render_image_async(
    ast: &[Stmt],
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .expect("Failed to create device");

    let bytes_per_row = (width * 4 + 255) & !255;
    let texture_desc = wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::STORAGE_BINDING,
        label: Some("Output Texture"),
        view_formats: &[],
    };
    let texture = device.create_texture(&texture_desc);
    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let output_buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        label: None,
        mapped_at_creation: false,
    });

    let mut core = RendererCore::new(&device, &queue);

        let timeline = Timeline::build(ast);
    let scene = timeline.evaluate(time as f64);
    core.render_vello_scene(&device, &queue, &texture_view, width, height, &scene);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Render Encoder"),
    });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();

    let data = buffer_slice.get_mapped_range();

    let mut img = image::RgbaImage::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let idx = (y * bytes_per_row + x * 4) as usize;
        let r = data[idx];
        let g = data[idx + 1];
        let b = data[idx + 2];
        let a = data[idx + 3];
        *pixel = image::Rgba([r, g, b, a]);
    }
    img.save(output_file).unwrap();
    drop(data);

    output_buffer.unmap();
}
