//! Minimal vello-level repro: does an alpha-0 image render poison later renders?
//!
//!History matrix (each row = a fresh Renderer + texture):
//!  A: img(α0) → img(α1)              (bd_video shape — expected OK)
//!  B: img(α0) → rect → img(α1)       (case_vig shape — FAILS in production)
//!  C: rect → img(α1)                 (swap shape)
//!  D: img(α1)                        (image-mode shape)
use vello::peniko::{
    Color, ImageAlphaType, ImageData as PenikoImageData, ImageFormat, ImageQuality,
};
use vello::{AaConfig, RenderParams, Renderer, RendererOptions, Scene};
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
    let brush = vello::peniko::ImageBrush::new(data.clone())
        .with_extend(vello::peniko::Extend::Pad)
        .with_quality(ImageQuality::Medium)
        .with_alpha(alpha);
    let t = vello::kurbo::Affine::new([53.333_332, 0.0, 0.0, 30.0, 0.0, 0.0]);
    scene.draw_image(&brush, t);
    scene
}

fn rect_scene() -> Scene {
    let mut scene = Scene::new();
    let rect = vello::kurbo::Rect::new(440.0, 340.0, 460.0, 380.0);
    scene.fill(
        vello::peniko::Fill::NonZero,
        vello::kurbo::Affine::IDENTITY,
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
            | TextureUsages::STORAGE_BINDING
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
    let data = slice.get_mapped_range();
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

fn run_case(name: &str, use_cpu: bool, steps: &[(&str, f32)]) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
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
    let mut renderer = Renderer::new(
        &device,
        RendererOptions {
            use_cpu,
            pipeline_cache: None,
            antialiasing_support: vello::AaSupport::all(),
            num_init_threads: None,
        },
    )
    .expect("renderer");
    let (texture, view) = make_texture(&device);
    let params = RenderParams {
        base_color: Color::TRANSPARENT,
        width: 1280,
        height: 720,
        antialiasing_method: AaConfig::Area,
    };
    let data = checker_image();
    for (kind, alpha) in steps {
        let scene = match *kind {
            "img" => img_scene(*alpha, &data),
            "rect" => rect_scene(),
            other => panic!("unknown step {other}"),
        };
        renderer
            .render_to_texture(&device, &queue, &scene, &view, &params)
            .expect("render");
        let bright = readback_bright(&device, &queue, &texture);
        eprintln!("[vp] {name} step {kind} alpha={alpha}: bright={bright}");
    }
}

#[test]
fn vello_image_history_matrix() {
    let use_cpu = std::env::var_os("ANIMATIX_CPU_RENDER").is_some();
    run_case("A img→img", use_cpu, &[("img", 0.0), ("img", 1.0)]);
    run_case("B img→rect→img", use_cpu, &[("img", 0.0), ("rect", 1.0), ("img", 1.0)]);
    run_case("C rect→img", use_cpu, &[("rect", 0.0), ("img", 1.0)]);
    run_case("D img-only", use_cpu, &[("img", 1.0)]);
    run_case("E img1→rect→img1", use_cpu, &[("img", 1.0), ("rect", 1.0), ("img", 1.0)]);
    run_case(
        "F img0→rect→rect→img1",
        use_cpu,
        &[("img", 0.0), ("rect", 1.0), ("rect", 1.0), ("img", 1.0)],
    );
    run_case(
        "G img1→img1→rect→img1",
        use_cpu,
        &[("img", 1.0), ("img", 1.0), ("rect", 1.0), ("img", 1.0)],
    );
    run_case(
        "H rect→img0→rect→img1",
        use_cpu,
        &[("rect", 1.0), ("img", 0.0), ("rect", 1.0), ("img", 1.0)],
    );
}
