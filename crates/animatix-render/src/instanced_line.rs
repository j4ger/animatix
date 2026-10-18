//! Instanced GPU line rendering pipeline.
//!
//! Renders anti-aliased 2D line strips and segments directly on the GPU without
//! CPU mesh triangulation. Vertex shaders expand segment instances into dilated
//! bounding quads, and fragment shaders evaluate signed distance functions (SDF)
//! with sub-pixel smoothstep anti-aliasing and analytical rounded end caps.

use std::borrow::Cow;

/// Instance data for one line segment, 40 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineSegmentInstance {
    /// Start point in pixel coordinates: [x, y].
    pub p0: [f32; 2],
    /// End point in pixel coordinates: [x, y].
    pub p1: [f32; 2],
    /// Stroke width in pixels.
    pub width: f32,
    /// Feather width for anti-aliasing (typically 1.0 px).
    pub feather: f32,
    /// RGBA line color [r, g, b, a].
    pub color: [f32; 4],
}

const LINE_WGSL: &str = r#"
struct LineSegmentInstance {
    @location(0) p0: vec2<f32>,
    @location(1) p1: vec2<f32>,
    @location(2) width: f32,
    @location(3) feather: f32,
    @location(4) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) pixel_pos: vec2<f32>,
    @location(1) @interpolate(flat) p0: vec2<f32>,
    @location(2) @interpolate(flat) p1: vec2<f32>,
    @location(3) @interpolate(flat) width: f32,
    @location(4) @interpolate(flat) feather: f32,
    @location(5) @interpolate(flat) color: vec4<f32>,
};

struct Uniforms {
    viewport_size: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: LineSegmentInstance,
) -> VertexOutput {
    let delta = instance.p1 - instance.p0;
    let len = length(delta);
    let dir = select(vec2<f32>(1.0, 0.0), delta / len, len > 1e-5);
    let normal = vec2<f32>(-dir.y, dir.x);
    let expansion = instance.width * 0.5 + max(instance.feather, 1.0) + 1.0;

    let v0 = instance.p0 - dir * expansion - normal * expansion;
    let v1 = instance.p1 + dir * expansion - normal * expansion;
    let v2 = instance.p0 - dir * expansion + normal * expansion;
    let v3 = instance.p1 + dir * expansion + normal * expansion;

    var pos = vec2<f32>(0.0);
    switch vertex_index {
        case 0u: { pos = v0; }
        case 1u: { pos = v1; }
        case 2u: { pos = v2; }
        case 3u: { pos = v2; }
        case 4u: { pos = v1; }
        default: { pos = v3; }
    }

    var out: VertexOutput;
    let ndc_x = (pos.x / uniforms.viewport_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pos.y / uniforms.viewport_size.y) * 2.0;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.pixel_pos = pos;
    out.p0 = instance.p0;
    out.p1 = instance.p1;
    out.width = instance.width;
    out.feather = instance.feather;
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.pixel_pos;
    let ba = in.p1 - in.p0;
    let pa = p - in.p0;
    let len_sq = dot(ba, ba);
    let h = clamp(select(0.0, dot(pa, ba) / len_sq, len_sq > 1e-5), 0.0, 1.0);
    let dist = length(pa - ba * h);

    let radius = in.width * 0.5;
    let feather = max(in.feather, 0.5);
    let d = dist - radius;
    let coverage = 1.0 - smoothstep(0.0, feather, d);
    let alpha = in.color.a * clamp(coverage, 0.0, 1.0);
    return vec4<f32>(in.color.rgb, alpha);
}
"#;

/// GPU rendering pipeline for instanced line strips and segments.
pub struct InstancedLinePipeline {
    /// Render pipeline for drawing quad-expanded line instances.
    pub pipeline: wgpu::RenderPipeline,
    /// Bind group layout for viewport uniforms.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer containing `[width, height]`.
    pub uniform_buffer: wgpu::Buffer,
    /// Bind group for uniform buffer.
    pub bind_group: wgpu::BindGroup,
}

impl InstancedLinePipeline {
    /// Create a new instanced line pipeline targeting `target_format`.
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Line Viewport Uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animatix Line Pipeline Bind Group Layout"),
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
            label: Some("Animatix Line Pipeline Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Line Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Instanced Line Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(LINE_WGSL)),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Animatix Instanced Line Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineSegmentInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 16,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 20,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 24,
                            shader_location: 4,
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

    /// Render line instances into `target_view` using the provided command encoder.
    pub fn render_lines_with_encoder(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        viewport_size: [f32; 2],
        instances: &[LineSegmentInstance],
    ) {
        if instances.is_empty() {
            return;
        }

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&viewport_size));

        let instance_bytes = bytemuck::cast_slice(instances);
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Line Instance Buffer"),
            size: instance_bytes.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&instance_buffer, 0, instance_bytes);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Animatix Instanced Line Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        pass.draw(0..6, 0..instances.len() as u32);
    }

    /// Render line instances into `target_view` and submit immediately.
    pub fn render_lines(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        viewport_size: [f32; 2],
        instances: &[LineSegmentInstance],
    ) {
        if instances.is_empty() {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Instanced Line Encoder"),
        });
        self.render_lines_with_encoder(
            device,
            queue,
            &mut encoder,
            target_view,
            viewport_size,
            instances,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Convert a polyline strip of 2D points into connected line segment instances.
    pub fn segments_from_strip(
        points: &[[f32; 2]],
        width: f32,
        feather: f32,
        color: [f32; 4],
    ) -> Vec<LineSegmentInstance> {
        if points.len() < 2 {
            return Vec::new();
        }
        let mut instances = Vec::with_capacity(points.len() - 1);
        for pair in points.windows(2) {
            instances.push(LineSegmentInstance {
                p0: pair[0],
                p1: pair[1],
                width,
                feather,
                color,
            });
        }
        instances
    }

    /// Convert a closed polygon contour into connected line segment instances.
    pub fn segments_from_closed_strip(
        points: &[[f32; 2]],
        width: f32,
        feather: f32,
        color: [f32; 4],
    ) -> Vec<LineSegmentInstance> {
        if points.len() < 2 {
            return Vec::new();
        }
        let mut instances = Vec::with_capacity(points.len());
        for pair in points.windows(2) {
            instances.push(LineSegmentInstance {
                p0: pair[0],
                p1: pair[1],
                width,
                feather,
                color,
            });
        }
        if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            if first != last {
                instances.push(LineSegmentInstance {
                    p0: last,
                    p1: first,
                    width,
                    feather,
                    color,
                });
            }
        }
        instances
    }

    /// Sample a parametric curve `f(t)` over `[t_min, t_max]` into connected line segment instances.
    pub fn segments_from_fn<F: FnMut(f32) -> [f32; 2]>(
        mut f: F,
        t_range: [f32; 2],
        steps: usize,
        width: f32,
        feather: f32,
        color: [f32; 4],
    ) -> Vec<LineSegmentInstance> {
        let n = steps.max(2);
        let mut points = Vec::with_capacity(n);
        let [t_min, t_max] = t_range;
        for i in 0..n {
            let u = i as f32 / (n - 1) as f32;
            let t = t_min + (t_max - t_min) * u;
            points.push(f(t));
        }
        Self::segments_from_strip(&points, width, feather, color)
    }
}
