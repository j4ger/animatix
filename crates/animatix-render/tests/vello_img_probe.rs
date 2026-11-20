//! Image visibility across interleaved Vello GPU frames.

use animatix::scene::Scene;
use animatix_render::frame_gpu::FrameGpu;
use peniko::{Color, ImageAlphaType, ImageData as PenikoImageData, ImageFormat, ImageQuality};
use wgpu::TextureUsages;

fn checker_image() -> PenikoImageData {
    let w = 24u32;
    let h = 24u32;
    let mut px = vec![0u8; (w * h * 4) as usize];
    for i in 0..(w * h) as usize {
        px[i * 4] = 170;
        px[i * 4 + 1] = 190;
        px[i * 4 + 2] = 180;
        px[i * 4 + 3] = 255;
    }
    PenikoImageData {
        data: px.into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: w,
        height: h,
    }
}

fn img_scene(alpha: f32, data: &PenikoImageData) -> Scene {
    let mut scene = Scene::new();
    let brush = peniko::ImageBrush::new(data.clone())
        .with_extend(peniko::Extend::Pad)
        .with_quality(ImageQuality::Medium)
        .with_alpha(alpha);
    let t = kurbo::Affine::new([53.333_332, 0.0, 0.0, 30.0, 0.0, 0.0]);
    scene.draw_image(&brush, t);
    scene
}

fn rect_scene() -> Scene {
    let mut scene = Scene::new();
    let rect = kurbo::Rect::new(440.0, 340.0, 460.0, 380.0);
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        Color::from_rgba8(255, 51, 51, 255),
        None,
        &rect,
    );
    scene
}

fn make_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_DST
            | TextureUsages::TEXTURE_BINDING,
        label: Some("probe target"),
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn readback_bright(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> usize {
    let bytes_per_row = (1280 * 4usize).next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: (bytes_per_row * 720) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        label: Some("probe readback"),
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row as u32),
                rows_per_image: Some(720),
            },
        },
        wgpu::Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).ok();
    });
    match device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    }) {
        Ok(_) => {},
        Err(e) => panic!("poll failed: {e:?}"),
    }
    if let Err(e) = rx.recv().unwrap() {
        panic!("map failed: {e:?}");
    }
    let data = slice.get_mapped_range().expect("mapped range");
    let mut bright = 0usize;
    for i in 0..(1280 * 720) as usize {
        if data[i * 4] > 60 {
            bright += 1;
        }
    }
    drop(data);
    buffer.unmap();
    bright
}

fn run_case(name: &str, steps: &[(&str, f32)]) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("probe device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
        ..Default::default()
    }))
    .expect("device");
    let mut renderer = FrameGpu::new(&device, &queue);
    let (texture, view) = make_texture(&device);
    let data = checker_image();
    for (i, (kind, alpha)) in steps.iter().enumerate() {
        let scene = match *kind {
            "img" => img_scene(*alpha, &data),
            "rect" => rect_scene(),
            other => panic!("unknown step {other}"),
        };
        renderer.render_into(&scene, &view, kurbo::Affine::IDENTITY).expect("render");
        let bright = readback_bright(&device, &queue, &texture);
        eprintln!("[vp] {name} step {kind} alpha={alpha}: bright={bright}");
        match (*kind, *alpha) {
            ("img", a) if a > 0.5 => assert!(
                bright >= 900_000,
                "{name}: step {i} drew an image at alpha {a} and only {bright} of \
                 921_600 canvas pixels have ink — the vanished-image regression"
            ),
            ("img", _) => assert!(
                bright <= 1_000,
                "{name}: step {i} drew an image at alpha {alpha} and got {bright} \
                 bright pixels; an alpha-0 image must cover nothing"
            ),
            ("rect", _) => assert!(
                (700..=900).contains(&bright),
                "{name}: step {i} drew the 20×40 rect and got {bright} bright pixels, \
                 outside the 700..=900 the rect alone covers"
            ),
            _ => unreachable!("run_case is only given img and rect steps"),
        }
    }
}

#[test]
fn vello_image_history_matrix() {
    run_case("A img→img", &[("img", 0.0), ("img", 1.0)]);
    run_case("B img→rect→img", &[("img", 0.0), ("rect", 1.0), ("img", 1.0)]);
    run_case("C rect→img", &[("rect", 0.0), ("img", 1.0)]);
    run_case("D img-only", &[("img", 1.0)]);
    run_case("E img1→rect→img1", &[("img", 1.0), ("rect", 1.0), ("img", 1.0)]);
    run_case(
        "F img0→rect→rect→img1",
        &[("img", 0.0), ("rect", 1.0), ("rect", 1.0), ("img", 1.0)],
    );
    run_case(
        "G img1→img1→rect→img1",
        &[("img", 1.0), ("img", 1.0), ("rect", 1.0), ("img", 1.0)],
    );
    run_case(
        "H rect→img0→rect→img1",
        &[("rect", 1.0), ("img", 0.0), ("rect", 1.0), ("img", 1.0)],
    );
}
