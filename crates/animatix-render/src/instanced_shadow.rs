//! Instanced erf analytical Gaussian shadow pipeline.
//!
//! Evaluates Evan Wallace / IQ closed-form erf Gaussian convolution for
//! rectangular and rounded cards directly on the GPU without texture sampling
//! or filter passes.

use std::borrow::Cow;

/// Instance data for one rectangular card shadow, 40 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowInstance {
    /// Destination rectangle in screen pixels: [min_x, min_y, max_x, max_y].
    pub rect: [f32; 4],
    /// Corner rounding radius in scene pixels.
    pub corner_radius: f32,
    /// Standard deviation sigma (blur / 2.0).
    pub sigma: f32,
    /// RGBA shadow color.
    pub color: [f32; 4],
}

const SHADOW_WGSL: &str = r#"
struct ShadowInstance {
    @location(0) rect: vec4<f32>,
    @location(1) corner_radius: f32,
    @location(2) sigma: f32,
    @location(3) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
    @location(1) @interpolate(flat) rect: vec4<f32>,
    @location(2) @interpolate(flat) corner_radius: f32,
    @location(3) @interpolate(flat) sigma: f32,
    @location(4) @interpolate(flat) color: vec4<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: ShadowInstance,
) -> VertexOutput {
    let pad = instance.sigma * 3.0;
    let min_p = instance.rect.xy - vec2<f32>(pad);
    let max_p = instance.rect.zw + vec2<f32>(pad);

    var corner = vec2<f32>(0.0);
    switch vertex_index {
        case 0u: { corner = vec2<f32>(min_p.x, min_p.y); }
        case 1u: { corner = vec2<f32>(max_p.x, min_p.y); }
        case 2u: { corner = vec2<f32>(min_p.x, max_p.y); }
        case 3u: { corner = vec2<f32>(min_p.x, max_p.y); }
        case 4u: { corner = vec2<f32>(max_p.x, min_p.y); }
        default: { corner = vec2<f32>(max_p.x, max_p.y); }
    }

    var out: VertexOutput;
    let ndc_x = (corner.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (corner.y / uniforms.viewport_size.y) * 2.0;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_pos = corner;
    out.rect = instance.rect;
    out.corner_radius = instance.corner_radius;
    out.sigma = instance.sigma;
    out.color = instance.color;
    return out;
}

// Abramowitz & Stegun erf approximation (error < 5e-4)
fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let p = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    let p2 = p * p;
    return s * (1.0 - 1.0 / (p2 * p2));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.pixel_pos;
    let r_min = in.rect.xy;
    let r_max = in.rect.zw;
    let sigma = max(in.sigma, 0.5);

    let center = (r_min + r_max) * 0.5;
    let half_size = (r_max - r_min) * 0.5;
    let rad = min(in.corner_radius, min(half_size.x, half_size.y));
    let inner_half = half_size - vec2<f32>(rad);

    let d_box = abs(p - center) - inner_half;
    let q = max(d_box, vec2<f32>(0.0));
    let dist = length(q) - rad;

    let coverage = 0.5 - 0.5 * erf(dist / (1.41421356 * sigma));
    let alpha = in.color.a * clamp(coverage, 0.0, 1.0);
    return vec4<f32>(in.color.rgb, alpha);
}
"#;

pub struct InstancedShadowPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl InstancedShadowPipeline {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow Viewport Uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow Pipeline Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow Pipeline Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Instanced Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADOW_WGSL)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Instanced Shadow Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ShadowInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 16,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 20,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 24,
                            shader_location: 3,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            bind_group,
        }
    }
}
