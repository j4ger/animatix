//! Consecutive Vello rendering and final presentation through one RendererCore.
//!
//! These GPU probes model separate player-owned targets interleaved through
//! the page's shared renderer and compare the merged path against the existing
//! render-then-present path over every pixel.

use animatix_render::core::RendererCore;
use animatix_render::testing::skip_if_no_gpu;
use std::sync::Arc;
use vello::Scene;
use vello::peniko::{
    Color, ImageAlphaType, ImageData as PenikoImageData, ImageFormat, ImageQuality,
};
use wgpu::{AdapterInfo, Texture, TextureFormat, TextureUsages, TextureView};

struct Target {
    texture: Texture,
    view: TextureView,
    size: [u32; 2],
    format: TextureFormat,
}

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue, AdapterInfo)> {
    let force_fallback_adapter = std::env::var_os("ANIMATIX_TEST_FORCE_FALLBACK_GPU").is_some();
    pollster::block_on(async move {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter,
                apply_limit_buckets: false,
            })
            .await
            .ok()?;
        let info = adapter.get_info();
        let limits = wgpu::Limits::default().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Animatix merged presentation test device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .ok()?;
        Some((device, queue, info))
    })
}

fn target(
    device: &wgpu::Device,
    size: [u32; 2],
    format: TextureFormat,
    usage: TextureUsages,
    label: &str,
) -> Target {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Target {
        texture,
        view,
        size,
        format,
    }
}

fn render_target(device: &wgpu::Device, size: [u32; 2], label: &str) -> Target {
    target(
        device,
        size,
        TextureFormat::Rgba8Unorm,
        TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
        label,
    )
}

fn surface_target(
    device: &wgpu::Device,
    size: [u32; 2],
    format: TextureFormat,
    label: &str,
) -> Target {
    target(
        device,
        size,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        label,
    )
}

fn solid_scene(size: [u32; 2], color: [u8; 4]) -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        vello::peniko::Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        Color::from_rgba8(color[0], color[1], color[2], color[3]),
        None,
        &vello::kurbo::Rect::new(0.0, 0.0, size[0] as f64, size[1] as f64),
    );
    scene
}

fn test_image() -> PenikoImageData {
    let width = 8u32;
    let height = 8u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = (x + y) % 2 == 0;
            rgba.extend_from_slice(if checker {
                &[236, 34, 28, 176]
            } else {
                &[24, 176, 224, 224]
            });
        }
    }
    PenikoImageData {
        data: rgba.into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

fn solid_image() -> PenikoImageData {
    let width = 4u32;
    let height = 4u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        rgba.extend_from_slice(&[236, 34, 28, 255]);
    }
    PenikoImageData {
        data: rgba.into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
}

fn image_scene(alpha: f32, size: [u32; 2], image: &PenikoImageData) -> Scene {
    let brush = vello::peniko::ImageBrush::new(image.clone())
        .with_extend(vello::peniko::Extend::Pad)
        .with_quality(ImageQuality::Medium)
        .with_alpha(alpha);
    let mut scene = Scene::new();
    scene.draw_image(
        &brush,
        vello::kurbo::Affine::scale_non_uniform(
            size[0] as f64 / image.width as f64,
            size[1] as f64 / image.height as f64,
        ),
    );
    scene
}

fn mixed_scene(size: [u32; 2], image: &PenikoImageData) -> Scene {
    let mut scene = solid_scene(size, [15, 26, 48, 255]);
    let brush = vello::peniko::ImageBrush::new(image.clone())
        .with_extend(vello::peniko::Extend::Pad)
        .with_quality(ImageQuality::Medium)
        .with_alpha(0.73);
    let image_width = (size[0] as f64 * 0.58).max(1.0);
    let image_height = (size[1] as f64 * 0.72).max(1.0);
    scene.draw_image(
        &brush,
        vello::kurbo::Affine::translate((size[0] as f64 * 0.11, size[1] as f64 * 0.07))
            * vello::kurbo::Affine::scale_non_uniform(
                image_width / image.width as f64,
                image_height / image.height as f64,
            ),
    );
    let overlay = vello::kurbo::Rect::new(
        size[0] as f64 * 0.37,
        size[1] as f64 * 0.31,
        size[0] as f64 * 0.89,
        size[1] as f64 * 0.84,
    );
    scene.fill(
        vello::peniko::Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
        Color::from_rgba8(244, 116, 28, 139),
        None,
        &overlay,
    );
    scene
}

fn present(
    core: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    raster: &Target,
    scene: &Scene,
    surface: &Target,
) -> Result<(), animatix_core::error::RenderError> {
    core.render_vello_scene_and_present(
        device,
        queue,
        &raster.view,
        raster.size,
        scene,
        &surface.view,
        surface.size,
        surface.format,
    )
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, target: &Target) -> Vec<u8> {
    let unpadded_bytes_per_row = target.size[0] * 4;
    let padded_bytes_per_row = unpadded_bytes_per_row.next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("presentation test readback"),
        size: (padded_bytes_per_row * target.size[1]) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("presentation test readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(target.size[1]),
            },
        },
        wgpu::Extent3d {
            width: target.size[0],
            height: target.size[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).ok();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll readback");
    rx.recv().expect("readback callback").expect("map readback");
    let mapped = slice.get_mapped_range().expect("mapped readback");
    let mut compact = Vec::with_capacity((target.size[0] * target.size[1] * 4) as usize);
    for row in 0..target.size[1] as usize {
        let start = row * padded_bytes_per_row as usize;
        compact.extend_from_slice(&mapped[start..start + unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    buffer.unmap();
    compact
}

fn rgba_pixel(bytes: &[u8], target: &Target, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * target.size[0] + x) * 4) as usize;
    let raw = [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ];
    if target.format == TextureFormat::Bgra8Unorm {
        [raw[2], raw[1], raw[0], raw[3]]
    } else {
        raw
    }
}

fn assert_color(actual: [u8; 4], expected: [u8; 4], label: &str) {
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= 4,
            "{label}: expected {expected:?}, got {actual:?}"
        );
    }
}

fn clear_surface(device: &wgpu::Device, queue: &wgpu::Queue, target: &Target, color: [f64; 4]) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("presentation test clear"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("presentation test clear pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                        a: color[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
}

fn render_path_suite(force_fallback_adapter: bool) {
    let Some((device, queue, info)) = headless_device() else {
        skip_if_no_gpu();
        return;
    };
    eprintln!(
        "[presentation-submit] adapter={} type={:?} fallback_requested={force_fallback_adapter}",
        info.name, info.device_type
    );
    device.on_uncaptured_error(Arc::new(|error| {
        panic!("uncaptured wgpu error during merged presentation: {error}");
    }));
    let mut core = RendererCore::new(&device, &queue).expect("renderer core");

    // A and B model two player-owned target sets sharing the process-wide core.
    let source_a = render_target(&device, [16, 16], "player A render target");
    let surface_a =
        surface_target(&device, [32, 24], TextureFormat::Rgba8Unorm, "player A rgba surface");
    let source_b = render_target(&device, [16, 16], "player B render target");
    let surface_b =
        surface_target(&device, [16, 16], TextureFormat::Bgra8Unorm, "player B bgra surface");

    // Reuse the exact same image blob across the image-free frame boundary.
    let image = solid_image();
    let alpha_zero_image = image_scene(0.0, source_a.size, &image);
    present(&mut core, &device, &queue, &source_a, &alpha_zero_image, &surface_a)
        .expect("alpha-zero image frame");

    let green = [28, 210, 52, 255];
    let no_image_scene = solid_scene(source_b.size, green);
    present(&mut core, &device, &queue, &source_b, &no_image_scene, &surface_b)
        .expect("no-image frame on second target");

    let opaque_image = image_scene(1.0, source_a.size, &image);
    present(&mut core, &device, &queue, &source_a, &opaque_image, &surface_a)
        .expect("opaque image after no-image frame");

    let image_pixels = readback(&device, &queue, &surface_a);
    assert_color(
        rgba_pixel(&image_pixels, &surface_a, 16, 12),
        [236, 34, 28, 255],
        "image -> no-image -> image on player A",
    );
    let green_pixels = readback(&device, &queue, &surface_b);
    assert_color(
        rgba_pixel(&green_pixels, &surface_b, 8, 8),
        green,
        "player B BGRA target remains independent",
    );

    // Resize the raster and presentation target while keeping the same Vello renderer.
    let resized_source_a = render_target(&device, [24, 16], "player A resized render target");
    let resized_surface_a =
        surface_target(&device, [48, 32], TextureFormat::Rgba8Unorm, "player A resized surface");
    let blue = [24, 48, 224, 255];
    let resized_scene = solid_scene(resized_source_a.size, blue);
    present(
        &mut core,
        &device,
        &queue,
        &resized_source_a,
        &resized_scene,
        &resized_surface_a,
    )
    .expect("resized target frame");
    let resized_pixels = readback(&device, &queue, &resized_surface_a);
    assert_color(
        rgba_pixel(&resized_pixels, &resized_surface_a, 24, 16),
        blue,
        "resized frame is presented",
    );

    // Fail both invalid-raster and invalid-surface dimensions before encoding.
    clear_surface(&device, &queue, &surface_a, [0.13, 0.27, 0.61, 1.0]);
    let before_error = readback(&device, &queue, &surface_a);
    for (raster_size, surface_size) in [([0, 16], surface_a.size), (source_a.size, [0, 24])] {
        let error = core.render_vello_scene_and_present(
            &device,
            &queue,
            &source_a.view,
            raster_size,
            &opaque_image,
            &surface_a.view,
            surface_size,
            surface_a.format,
        );
        assert!(error.is_err(), "zero dimensions must be rejected");
        let after_error = readback(&device, &queue, &surface_a);
        assert_eq!(
            after_error, before_error,
            "failed frame must not submit the presentation suffix"
        );
    }
}

#[test]
fn merged_present_keeps_shared_renderer_targets_images_formats_and_resize_ordered() {
    // Default adapter selection uses hardware where available. Setting this to
    // any value exercises the fallback/software adapter (e.g. lavapipe) instead.
    let force_fallback_adapter = std::env::var_os("ANIMATIX_TEST_FORCE_FALLBACK_GPU").is_some();
    render_path_suite(force_fallback_adapter);
}

#[test]
fn merged_present_matches_legacy_full_image_for_rgba_bgra_and_resized_targets() {
    let force_fallback_adapter = std::env::var_os("ANIMATIX_TEST_FORCE_FALLBACK_GPU").is_some();
    let Some((device, queue, info)) = headless_device() else {
        skip_if_no_gpu();
        return;
    };
    eprintln!(
        "[presentation-submit-parity] adapter={} type={:?} fallback_requested={force_fallback_adapter}",
        info.name, info.device_type
    );
    device.on_uncaptured_error(Arc::new(|error| {
        panic!("uncaptured wgpu error during legacy/merged parity: {error}");
    }));
    let mut core = RendererCore::new(&device, &queue).expect("renderer core");
    let image = test_image();

    for (index, raster_size, surface_size, format) in [
        (0, [32, 24], [32, 24], TextureFormat::Rgba8Unorm),
        (1, [24, 16], [48, 32], TextureFormat::Bgra8Unorm),
    ] {
        let legacy_source = render_target(&device, raster_size, "legacy comparison source");
        let merged_source = render_target(&device, raster_size, "merged comparison source");
        let legacy_surface =
            surface_target(&device, surface_size, format, "legacy comparison surface");
        let merged_surface =
            surface_target(&device, surface_size, format, "merged comparison surface");
        let scene = mixed_scene(raster_size, &image);
        let background = [0.14, 0.19, 0.31, 1.0];
        clear_surface(&device, &queue, &legacy_surface, background);
        clear_surface(&device, &queue, &merged_surface, background);

        core.render_vello_scene(
            &device,
            &queue,
            &legacy_source.view,
            raster_size[0],
            raster_size[1],
            &scene,
        )
        .expect("legacy Vello render");
        core.blit_texture_to_format(
            &device,
            &queue,
            &legacy_source.view,
            &legacy_surface.view,
            surface_size[0],
            surface_size[1],
            1.0,
            format,
        );
        core.render_vello_scene_and_present(
            &device,
            &queue,
            &merged_source.view,
            raster_size,
            &scene,
            &merged_surface.view,
            surface_size,
            format,
        )
        .expect("merged Vello render and present");

        let legacy_pixels = readback(&device, &queue, &legacy_surface);
        let merged_pixels = readback(&device, &queue, &merged_surface);
        assert_eq!(
            merged_pixels, legacy_pixels,
            "full readback changed between legacy and merged path for case {index}, format {format:?}, raster {raster_size:?}, surface {surface_size:?}"
        );
    }
}
