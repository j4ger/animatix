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

/// Fragment params, 48 bytes, written by [`FullscreenBlitPipeline::write_params`].
///
/// A plain composite only uses `alpha`; a backdrop composite also samples a
/// sub-rectangle of its texture and clips the result to a rounded rect. Both
/// live in one uniform so the plain path costs nothing extra (its `radius` and
/// `use_src_rect` stay zero, and the shader then behaves exactly as before).
const FULLSCREEN_BLIT_PARAMS_WGSL: &str = r#"
struct BlitParams {
    alpha: f32,
    radius: f32,
    dst_size: vec2<f32>,
    src_rect: vec4<f32>,
    use_src_rect: f32,
};
"#;

const FULLSCREEN_BLIT_FS: &str = r#"
@group(0) @binding(0) var src_sampler: sampler;
@group(0) @binding(1) var src_texture: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: BlitParams;

@fragment
fn fs_main(@location(0) tex_coord: vec2<f32>) -> @location(0) vec4<f32> {
    // Sample window: the whole texture by default, or the caller's sub-rect.
    let dims = vec2<f32>(textureDimensions(src_texture));
    let full = vec4<f32>(0.0, 0.0, dims.x, dims.y);
    let rect = select(full, params.src_rect, params.use_src_rect > 0.5);
    let uv = mix(rect.xy, rect.zw, tex_coord) / dims;
    let color = textureSample(src_texture, src_sampler, uv);

    var coverage = 1.0;
    if (params.radius > 0.0) {
        // Signed distance to a rounded box, in destination pixels. `tex_coord`
        // spans the render viewport, which is the composite's rect.
        let half = params.dst_size * 0.5;
        let r = min(params.radius, min(half.x, half.y));
        let q = abs(tex_coord * params.dst_size - half) - half + vec2<f32>(r);
        let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
        coverage = 1.0 - smoothstep(0.0, 1.0, d);
    }
    return vec4<f32>(color.rgb, color.a * params.alpha * coverage);
}
"#;

/// How a composite is clipped and which part of its texture it samples.
///
/// Both halves exist for the backdrop pass: the blurred texture covers a region
/// *larger* than the panel (the blur needs pixels from outside the edge), while
/// only the panel rect is painted, in the panel's own rounded shape.
pub struct BlitMask {
    /// Sub-rectangle of the source texture to sample, in texture pixels
    /// `[x0, y0, x1, y1]`.
    pub src_rect: [f32; 4],
    /// Corner radius of the destination rect, in destination pixels.
    pub corner_radius: f32,
}

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
    /// Create the blit pipeline on the given device, targeting `target_format`.
    ///
    /// WebGPU requires a render pipeline's color-target format to match the
    /// render pass attachment exactly. Internal targets are ours (always
    /// `Rgba8Unorm`), but a browser canvas surface reports whichever format
    /// its backend prefers — Firefox's wgpu orders `Bgra8Unorm` first — so
    /// callers presenting to a surface must compile for that format (see
    /// [`RendererCore`](super::core::RendererCore)'s per-format variants).
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
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
                        min_binding_size: Some(std::num::NonZero::new(48).unwrap()),
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
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!(
                "{}\n{}",
                FULLSCREEN_BLIT_VS,
                &format!("{FULLSCREEN_BLIT_PARAMS_WGSL}{FULLSCREEN_BLIT_FS}")
            ))),
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
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            size: 48, // sizeof(BlitParams), padded to the uniform alignment
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
        self.blit_rect_with_encoder(
            device,
            queue,
            encoder,
            src_view,
            dst_view,
            [0.0, 0.0],
            None,
            alpha,
        );
    }

    /// Write the 48-byte fragment params.
    ///
    /// Layout is `alpha, radius, dst_size, src_rect, use_src_rect` — a
    /// `[f32; 12]` written verbatim, so the WGSL struct and this array must stay
    /// in the same order.
    fn write_params(
        &self,
        queue: &wgpu::Queue,
        alpha: f32,
        mask: Option<&BlitMask>,
        dst_size: Option<[u32; 2]>,
    ) {
        let mut p = [0f32; 12];
        p[0] = alpha;
        if let Some(m) = mask {
            p[1] = m.corner_radius;
            if let Some([w, h]) = dst_size {
                p[2] = w as f32;
                p[3] = h as f32;
            }
            p[4] = m.src_rect[0];
            p[5] = m.src_rect[1];
            p[6] = m.src_rect[2];
            p[7] = m.src_rect[3];
            p[8] = 1.0;
        }
        queue.write_buffer(&self.alpha_buffer, 0, bytemuck::cast_slice(&p));
    }

    /// Blit `src_view` into `dst_view` at `dst_origin`, covering `dst_size`
    /// pixels (defaults to the full target when `None`). The quad is clipped
    /// and mapped through a render-pass viewport, so no shader change is
    /// needed for region-scoped compositing.
    pub fn blit_rect_with_encoder(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: Option<[u32; 2]>,
        alpha: f32,
    ) {
        self.blit_rect_masked_with_encoder(
            device, queue, encoder, src_view, dst_view, dst_origin, dst_size, alpha, None,
        );
    }

    /// Like [`Self::blit_rect_with_encoder`], with an optional rounded-rect
    /// clip and a sub-rectangle of the source texture to sample. A `Glass`
    /// backdrop uses both: its texture covers a padded region while only the
    /// panel's own rect is painted, and the panel's `corner_radius` shapes the
    /// result.
    pub fn blit_rect_masked_with_encoder(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: Option<[u32; 2]>,
        alpha: f32,
        mask: Option<&BlitMask>,
    ) {
        self.write_params(queue, alpha, mask, dst_size);

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

            if let Some([w, h]) = dst_size {
                pass.set_viewport(dst_origin[0], dst_origin[1], w as f32, h as f32, 0.0, 1.0);
            }
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
    }

    /// Blit with a rounded-clip and an optional source sub-rect, creating its own
    /// encoder and submitting immediately. See
    /// [`Self::blit_rect_masked_with_encoder`].
    pub fn blit_rect_masked(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: [u32; 2],
        alpha: f32,
        mask: Option<&BlitMask>,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Masked Blit Encoder"),
        });
        self.blit_rect_masked_with_encoder(
            device,
            queue,
            &mut encoder,
            src_view,
            dst_view,
            dst_origin,
            Some(dst_size),
            alpha,
            mask,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Blit `src_view` into `dst_view` at `dst_origin`, covering `dst_size`.
    /// Creates its own encoder and submits immediately.
    pub fn blit_rect(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: [u32; 2],
        alpha: f32,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Fullscreen Blit Encoder"),
        });
        self.blit_rect_with_encoder(
            device,
            queue,
            &mut encoder,
            src_view,
            dst_view,
            dst_origin,
            Some(dst_size),
            alpha,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Blit `src_view` into `dst_view` with the given alpha. Both formats must
    /// match what this pipeline was compiled for (the source is sampled as a
    /// texture — any float-sampleable format works; the *target* format is the
    /// strict one). Creates its own encoder and submits immediately; for
    /// batching use [`Self::blit_with_encoder`].
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
        self.blit_with_encoder(
            device,
            queue,
            &mut encoder,
            src_view,
            dst_view,
            width,
            height,
            alpha,
        );
        queue.submit(std::iter::once(encoder.finish()));
    }
}
