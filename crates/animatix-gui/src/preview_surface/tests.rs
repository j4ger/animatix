use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use super::PreviewSurface;
use animatix::composition::{BuildTarget, Composition};
use animatix::timeline::{BuildOptions, DebugRenderOptions, SceneDimensions, Timeline};

struct ThreadWake(std::thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park_timeout(Duration::from_millis(10)),
        }
    }
}

fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok()?;
    eprintln!("[preview-texture-adapter] {:?}", adapter.get_info());
    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Animatix GUI Preview Texture Test Device"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        memory_hints: Default::default(),
        ..Default::default()
    }))
    .ok()
}

fn device_or_skip() -> Option<(wgpu::Device, wgpu::Queue)> {
    match headless_device() {
        Some(device) => Some(device),
        None => {
            animatix_render::testing::skip_if_no_gpu();
            None
        },
    }
}

fn make_texture(
    device: &wgpu::Device,
    size: SceneDimensions,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: &[wgpu::TextureFormat],
    label: &'static str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats,
    })
}

struct SamplingPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl SamplingPipeline {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Preview sRGB Sampling Test Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Preview sRGB Sampling Test Pipeline Layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Preview sRGB Sampling Test Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, input.uv);
}
"#
                .into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Preview sRGB Sampling Test Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Preview sRGB Sampling Test Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            layout,
            pipeline,
            sampler,
        }
    }

    fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Preview sRGB Sampling Test Bind Group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

fn output_target(device: &wgpu::Device, size: SceneDimensions) -> wgpu::Texture {
    make_texture(
        device,
        size,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        &[],
        "Preview sRGB Sampling Test Output",
    )
}

fn render_sample(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &SamplingPipeline,
    bind_group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Preview sRGB Sampling Test Encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Preview sRGB Sampling Test Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(std::iter::once(encoder.finish()));
}

fn pixel(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> [u8; 4] {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Preview sRGB Sampling Test Readback"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Preview sRGB Sampling Test Readback Encoder"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let slice = buffer.slice(..);
    let (send, receive) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = send.send(result);
    });
    wait_for_gpu(device);
    receive.recv().expect("map callback").expect("map readback");
    let mapped = slice.get_mapped_range().expect("mapped readback range");
    let result = [mapped[0], mapped[1], mapped[2], mapped[3]];
    drop(mapped);
    buffer.unmap();
    result
}

fn sample_pixel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &SamplingPipeline,
    view: &wgpu::TextureView,
) -> [u8; 4] {
    let target = output_target(
        device,
        SceneDimensions {
            width: 1,
            height: 1,
        },
    );
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = pipeline.bind(device, view);
    render_sample(device, queue, pipeline, &bind_group, &target_view);
    pixel(device, queue, &target)
}

fn clear_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    color: wgpu::Color,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Preview Texture Target Test Clear Encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Preview Texture Target Test Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
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
    queue.submit(std::iter::once(encoder.finish()));
}

fn wait_for_gpu(device: &wgpu::Device) {
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("GPU poll");
}

fn assert_no_validation_error(device: &wgpu::Device, scope: wgpu::ErrorScopeGuard) {
    wait_for_gpu(device);
    let error = block_on(scope.pop());
    assert!(error.is_none(), "unexpected WebGPU validation error: {error:?}");
}

fn copy_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Texture,
    target: &wgpu::Texture,
    size: SceneDimensions,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Legacy Egui Preview Copy Encoder"),
    });
    encoder.copy_texture_to_texture(
        source.as_image_copy(),
        target.as_image_copy(),
        wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));
}

#[test]
fn direct_srgb_view_matches_legacy_copy() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let one = SceneDimensions {
        width: 1,
        height: 1,
    };
    let source = make_texture(
        &device,
        one,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        &[wgpu::TextureFormat::Rgba8UnormSrgb],
        "Legacy and Direct Preview Source",
    );
    let encoded_srgb = [64_u8, 128, 192, 255];
    queue.write_texture(
        source.as_image_copy(),
        &encoded_srgb,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let direct_view = source.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Direct Preview sRGB View"),
        format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        ..Default::default()
    });
    let legacy_texture = make_texture(
        &device,
        one,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        &[],
        "Legacy Egui Preview sRGB Sample Texture",
    );
    copy_texture(&device, &queue, &source, &legacy_texture, one);
    let legacy_view = legacy_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampling = SamplingPipeline::new(&device);
    let legacy_pixel = sample_pixel(&device, &queue, &sampling, &legacy_view);
    let direct_pixel = sample_pixel(&device, &queue, &sampling, &direct_view);
    assert_eq!(direct_pixel, legacy_pixel, "sRGB sampling must preserve preview colors");
    assert_ne!(direct_pixel, encoded_srgb, "the test pixel must exercise sRGB decode");

    assert_no_validation_error(&device, validation_scope);
}

const FADE_COMPOSITION: &str = r##"
# From
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
red: Rect, size: (2000, 2000), at: (32, 32), color: hex_color("ff0000")
play To [fade, 500ms]

# To
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
blue: Rect, size: (2000, 2000), at: (32, 32), color: hex_color("0000ff")
"##;

const MIXED_FILTER_COMPOSITION: &str = r##"
# From
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
red: Rect, size: (2000, 2000), at: (32, 32), color: hex_color("ff0000")
play To [fade, 500ms]

# To
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
fx: Filter {
    soft: Blur, radius: 3
    blue: Rect, size: (2000, 2000), color: hex_color("0000ff")
}
"##;

fn build_composition(source: &str) -> Composition {
    let (statements, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "fade fixture parse errors: {parse_errors:?}");
    let statements = statements.expect("fade composition parses");
    let report = BuildTarget::build_with_options(
        &statements,
        &std::collections::HashMap::new(),
        None,
        BuildOptions::default(),
    );
    let errors: Vec<_> = report.diagnostics.iter().filter(|item| item.is_error()).collect();
    assert!(errors.is_empty(), "fade fixture diagnostics: {errors:?}");
    match report.output {
        BuildTarget::MultiScene(composition) => composition,
        BuildTarget::SingleScene(_) => panic!("fade fixture must build a composition"),
    }
}

fn fade_composition() -> Composition {
    build_composition(FADE_COMPOSITION)
}

#[test]
fn composition_preview_switches_sample_view_across_fade_frames() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let composition = fade_composition();
    let transition_start = composition.scene_start_times["To"];
    let transition_time = transition_start + 0.25;
    let from_end = composition.scene_start_times["From"] + composition.scenes["From"].duration_s;
    let after_time = from_end + 0.01;
    assert!(composition.evaluate(0.0).2.is_none(), "time zero is a single-scene frame");
    assert!(
        composition.evaluate(transition_time).2.is_some(),
        "middle sample must be inside the fade"
    );
    assert!(
        composition.evaluate(after_time).2.is_none(),
        "last sample must be after the fade"
    );

    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut preview = PreviewSurface::new(&device, &queue).expect("preview surface");
    preview.set_dimensions(
        &device,
        SceneDimensions {
            width: 64,
            height: 64,
        },
    );
    let sampling = SamplingPipeline::new(&device);
    preview
        .render_composition(&device, &queue, &composition, 0.0, DebugRenderOptions::default())
        .expect("single-scene composition frame");
    let from = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("from scene sample view"),
    );
    assert_eq!(from, [255, 0, 0, 255]);

    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            transition_time,
            DebugRenderOptions::default(),
        )
        .expect("transition composition frame");
    let transition = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("transition sample view"),
    );
    assert!(
        transition[0] > 0 && transition[2] > 0,
        "fade pixel should blend red and blue: {transition:?}"
    );

    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            after_time,
            DebugRenderOptions::default(),
        )
        .expect("post-transition composition frame");
    let to = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("to scene sample view"),
    );
    assert_eq!(to, [0, 0, 255, 255]);

    assert_no_validation_error(&device, validation_scope);
}

#[test]
fn mixed_filter_transition_preserves_both_scenes() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let composition = build_composition(MIXED_FILTER_COMPOSITION);
    let transition_time = composition.scene_start_times["To"] + 0.25;
    assert!(composition.evaluate(transition_time).2.is_some());

    let mut preview = PreviewSurface::new(&device, &queue).expect("preview surface");
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let first = SceneDimensions {
        width: 64,
        height: 64,
    };
    preview.set_dimensions(&device, first);
    let sampling = SamplingPipeline::new(&device);
    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            transition_time,
            DebugRenderOptions::default(),
        )
        .expect("plain to Filter transition");

    let transition = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("mixed transition sample view"),
    );
    assert!(
        transition[0] > 0 && transition[2] > 0,
        "transition must retain both scene colors after the Filter pass: {transition:?}"
    );

    let after_transition = composition.scene_start_times["To"] + 0.75;
    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            after_transition,
            DebugRenderOptions::default(),
        )
        .expect("single Filter scene after transition");

    preview
        .render(&composition.scenes["From"].timeline, 0.0, DebugRenderOptions::default())
        .expect("plain scene after Filter frame");

    let plain_pixel = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("plain scene sample view"),
    );
    assert_eq!(plain_pixel, [255, 0, 0, 255]);

    let resized = SceneDimensions {
        width: 96,
        height: 80,
    };
    preview.set_dimensions(&device, resized);
    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            transition_time,
            DebugRenderOptions::default(),
        )
        .expect("resized plain to Filter transition");

    assert_no_validation_error(&device, validation_scope);
}

const GLASS_SCENE: &str = r##"
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
plate: Rect, size: (60, 60), at: (32, 32), color: hex_color("304050")
card: Glass, at: (32, 32), size: (28, 22), corner_radius: 4 {
    frost: Blur, radius: 3
    surface: Rect, size: (28, 22), color: hex_color("ffffff1a")
}
"##;

const TRANSITION_GLASS_COMPOSITION: &str = r##"
# From
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
backdrop: Rect, size: (2000, 2000), at: (32, 32), color: hex_color("ff0000")
card: Glass, at: (32, 32), size: (64, 64), corner_radius: 0 {
    frost: Blur, radius: 6
    surface: Rect, size: (64, 64), color: hex_color("ffffff")
}
play To [fade, 500ms]

# To
config { colorscheme: "editorial-dark", resolution: (64, 64), duration: 2.0 }
#0s
backdrop: Rect, size: (2000, 2000), at: (32, 32), color: hex_color("0000ff")
card: Glass, at: (32, 32), size: (64, 64), corner_radius: 0 {
    frost: Blur, radius: 6
    surface: Rect, size: (64, 64), color: hex_color("ffffff")
}
"##;

fn glass_timeline() -> Timeline {
    let (statements, parse_errors) = animatix_syntax::parser::parse_source(GLASS_SCENE);
    assert!(parse_errors.is_empty(), "Glass preview fixture parse errors: {parse_errors:?}");
    let statements = statements.expect("Glass preview fixture parses");
    let report = Timeline::build_with_options(
        &statements,
        &std::collections::HashMap::new(),
        BuildOptions::default(),
    );
    let errors: Vec<_> = report.diagnostics.iter().filter(|item| item.is_error()).collect();
    assert!(errors.is_empty(), "Glass preview fixture diagnostics: {errors:?}");
    report.output
}

#[test]
fn transition_glass_layers_render_after_resize() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let composition = build_composition(TRANSITION_GLASS_COMPOSITION);
    let transition_time = composition.scene_start_times["To"] + 0.25;
    assert!(
        composition.evaluate(transition_time).2.is_some(),
        "sample must be in the fade window"
    );

    let mut preview = PreviewSurface::new(&device, &queue).expect("preview surface");
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let first = SceneDimensions {
        width: 64,
        height: 64,
    };
    preview.set_dimensions(&device, first);
    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            transition_time,
            DebugRenderOptions::default(),
        )
        .expect("first Glass transition frame");

    let sampling = SamplingPipeline::new(&device);
    let first_pixel = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("first transition sample view"),
    );
    assert_eq!(first_pixel, [255, 255, 255, 255]);

    let resized = SceneDimensions {
        width: 96,
        height: 80,
    };
    preview.set_dimensions(&device, resized);

    preview
        .render_composition(
            &device,
            &queue,
            &composition,
            transition_time,
            DebugRenderOptions::default(),
        )
        .expect("resized Glass transition frame");

    let resized_pixel = sample_pixel(
        &device,
        &queue,
        &sampling,
        preview.sample_view().expect("resized transition sample view"),
    );
    assert_eq!(resized_pixel, [255, 255, 255, 255]);
    assert_no_validation_error(&device, validation_scope);
}

#[test]
fn glass_preview_keeps_backdrop_copy_valid_and_rebuilds_after_resize() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let timeline = glass_timeline();
    let mut preview = PreviewSurface::new(&device, &queue).expect("preview surface");
    let debug = DebugRenderOptions::default();
    let first = SceneDimensions {
        width: 64,
        height: 64,
    };
    let first_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    preview.set_dimensions(&device, first);
    preview.render(&timeline, 0.0, debug).expect("Glass preview render");

    assert_no_validation_error(&device, first_scope);

    let resized = SceneDimensions {
        width: 96,
        height: 80,
    };
    let resized_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    preview.set_dimensions(&device, resized);

    preview.render(&timeline, 0.0, debug).expect("resized Glass preview render");

    assert_no_validation_error(&device, resized_scope);
}

#[test]
fn glass_preview_remains_visible_after_resize() {
    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut preview = PreviewSurface::new(&device, &queue).expect("Vello GPU preview surface");
    let timeline = glass_timeline();
    let sampling = SamplingPipeline::new(&device);
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    for dimensions in [
        SceneDimensions {
            width: 64,
            height: 64,
        },
        SceneDimensions {
            width: 96,
            height: 80,
        },
    ] {
        preview.set_dimensions(&device, dimensions);
        preview
            .render(&timeline, 0.0, DebugRenderOptions::default())
            .expect("Vello GPU Glass preview render");
        let pixel = sample_pixel(
            &device,
            &queue,
            &sampling,
            preview.sample_view().expect("Vello GPU sample view"),
        );
        assert!(pixel[0] > 0 && pixel[1] > 0 && pixel[2] > 0, "Glass render is blank: {pixel:?}");
    }

    assert_no_validation_error(&device, validation_scope);
}

fn median_ms(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

#[test]
#[ignore = "manual GPU A/B for legacy copy versus direct sRGB view presentation"]
fn benchmark_legacy_copy_versus_direct_srgb_view() {
    const SIZE: SceneDimensions = SceneDimensions {
        width: 1280,
        height: 720,
    };
    const WARMUP: usize = 8;
    const SAMPLES: usize = 120;

    let Some((device, queue)) = device_or_skip() else {
        return;
    };
    let mut preview = PreviewSurface::new(&device, &queue).expect("preview surface");
    preview.set_dimensions(&device, SIZE);
    clear_view(
        &device,
        &queue,
        preview.render_view.as_ref().expect("render view"),
        wgpu::Color {
            r: 0.2,
            g: 0.4,
            b: 0.7,
            a: 1.0,
        },
    );

    let legacy_texture = make_texture(
        &device,
        SIZE,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        &[],
        "Legacy Preview Benchmark Sample Texture",
    );
    let legacy_view = legacy_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let direct_view = preview.sample_view().expect("direct sRGB sample view");
    let legacy_group = SamplingPipeline::new(&device);
    let direct_bind_group = legacy_group.bind(&device, direct_view);
    let legacy_bind_group = legacy_group.bind(&device, &legacy_view);
    let output = output_target(&device, SIZE);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());

    let run_direct = || {
        render_sample(&device, &queue, &legacy_group, &direct_bind_group, &output_view);
        wait_for_gpu(&device);
    };
    let run_legacy = || {
        copy_texture(
            &device,
            &queue,
            preview.render_texture.as_ref().expect("render texture"),
            &legacy_texture,
            SIZE,
        );
        render_sample(&device, &queue, &legacy_group, &legacy_bind_group, &output_view);
        wait_for_gpu(&device);
    };

    for _ in 0..WARMUP {
        run_legacy();
        run_direct();
    }
    let mut legacy_samples = Vec::with_capacity(SAMPLES);
    let mut direct_samples = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let start = Instant::now();
        if index % 2 == 0 {
            run_legacy();
        } else {
            run_direct();
        }
        let first_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        if index % 2 == 0 {
            run_direct();
        } else {
            run_legacy();
        }
        let second_ms = start.elapsed().as_secs_f64() * 1000.0;
        if index % 2 == 0 {
            legacy_samples.push(first_ms);
            direct_samples.push(second_ms);
        } else {
            direct_samples.push(first_ms);
            legacy_samples.push(second_ms);
        }
    }
    eprintln!(
        "[preview-texture-bench] {}x{} samples={} legacy-copy+sample p50={:.3}ms direct-srgb-view+sample p50={:.3}ms",
        SIZE.width,
        SIZE.height,
        SAMPLES,
        median_ms(legacy_samples),
        median_ms(direct_samples),
    );
}
