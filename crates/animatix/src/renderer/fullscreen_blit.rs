//! Fullscreen texture blit for zero-readback filter compositing.
//!
//! Vello's `Scene::draw_image` requires CPU-owned `peniko::ImageData`.
//! This module provides a custom render pass that draws a `wgpu::TextureView`
//! directly onto another render target, avoiding the CPU round-trip.
//!
//! Used by `GpuFilterBackend` to composite GPU-filtered sub-scenes back into
//! the main scene without readback.

use std::borrow::Cow;

const FULLSCREEN_BLIT_VS: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

@vertex
fn main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(vertex_index % 2u); // 0, 1, 0, 1
    let y = f32(vertex_index / 2u); // 0, 0, 1, 1
    out.position = vec4<f32>(x * 2.0 - 1.0, -(y * 2.0 - 1.0), 0.0, 1.0);
    out.tex_coord = vec2<f32>(x, y);
    return out;
}
"#;

const FULLSCREEN_BLIT_FS: &str = r#"
@group(0) @binding(0) var src_sampler: sampler;
@group(0) @binding(1) var src_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> alpha: f32;

@fragment
fn fs_main(@location(0) tex_coord: vec2<f32>) -> @location(0) vec4<f32> {
    let color = textureSample(src_texture, src_sampler, tex_coord);
    return vec4<f32>(color.rgb, color.a * alpha);
}
"#;

/// GPU state for a fullscreen texture blit.
pub struct FullscreenBlitPipeline {
    /// The render pipeline for fullscreen quad blitting.
    pub pipeline: wgpu::RenderPipeline,
    /// Bind group layout for source texture + sampler.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler used for texture sampling during blit.
    pub sampler: wgpu::Sampler,
    /// Pre-allocated uniform buffer for the alpha value.
    alpha_buffer: wgpu::Buffer,
}

impl FullscreenBlitPipeline {
    /// Create the blit pipeline on the given device.
    pub fn new(device: &wgpu::Device) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Fullscreen Blit Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animatix Fullscreen Blit Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(std::num::NonZero::new(4).unwrap()),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Fullscreen Blit Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Fullscreen Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{}\n{}", FULLSCREEN_BLIT_VS, FULLSCREEN_BLIT_FS))),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Animatix Fullscreen Blit Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
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

        let alpha_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Blit Alpha Uniform"),
            size: 4, // sizeof(f32)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            alpha_buffer,
        }
    }

    /// Blit `src_view` into `dst_view` with the given alpha using an external encoder.
    /// Callers can batch multiple blits into a single command buffer.
    pub fn blit_with_encoder(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        _width: u32,
        _height: u32,
        alpha: f32,
    ) {
        queue.write_buffer(&self.alpha_buffer, 0, bytemuck::bytes_of(&alpha));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Animatix Fullscreen Blit Bind Group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.alpha_buffer.as_entire_binding(),
                    },
                ],
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Animatix Fullscreen Blit Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: dst_view,
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

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
    }

    /// Blit `src_view` into `dst_view` with the given alpha. Both must be RGBA8Unorm.
    /// Creates its own encoder and submits immediately; for batching use [`Self::blit_with_encoder`].
    pub fn blit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        alpha: f32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Fullscreen Blit Encoder"),
            });
        self.blit_with_encoder(device, queue, &mut encoder, src_view, dst_view, width, height, alpha);
        queue.submit(std::iter::once(encoder.finish()));
    }
}
