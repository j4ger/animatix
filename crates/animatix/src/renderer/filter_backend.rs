//! Shared GPU filter backend for preview and export renderers.
//!
//! Both [`crate::renderer::offscreen::OffscreenRenderer`] and the GUI's
//! [`PreviewSurface`](crate::preview_surface::PreviewSurface) need identical
//! offscreen → GPU filter → readback behaviour for [`Filter`](crate::timeline::ActorKindId::Filter)
//! actors.  This module provides a single [`GpuFilterBackend`] implementation
//! that can be instantiated by any renderer that owns (or can borrow) a
//! [`wgpu::Device`] and [`wgpu::Queue`].
//!
//! Phase 8.6a: filter operations (blur + color matrix) run on the GPU via
//! WGSL compute shaders.  One CPU readback per filter actor still occurs so
//! the result can be drawn back into the parent Vello scene.

use crate::renderer::core::RendererCore;
use crate::timeline::filter::FilterBackend;
use crate::timeline::image::SceneImage;
use crate::timeline::SceneDimensions;
use std::borrow::Cow;

// ── WGSL compute shaders ────────────────────────────────────────────────────

const BLUR_SHADER_WGSL: &str = r#"
struct BlurParams {
    radius: f32,
    direction: i32,
    tex_size: vec2<u32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: BlurParams;

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let size = vec2<i32>(i32(params.tex_size.x), i32(params.tex_size.y));

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    if (params.radius < 0.5) {
        let texel = textureLoad(src, coord, 0);
        textureStore(dst, coord, texel);
        return;
    }

    let sigma = params.radius / 3.0;
    let radius = i32(ceil(params.radius));

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    if (params.direction == 0) {
        for (var i = -radius; i <= radius; i = i + 1) {
            let sample_coord = clamp(coord + vec2<i32>(i, 0), vec2<i32>(0), size - vec2<i32>(1));
            let w = gaussian_weight(f32(i), sigma);
            color = color + textureLoad(src, sample_coord, 0) * w;
            weight_sum = weight_sum + w;
        }
    } else {
        for (var i = -radius; i <= radius; i = i + 1) {
            let sample_coord = clamp(coord + vec2<i32>(0, i), vec2<i32>(0), size - vec2<i32>(1));
            let w = gaussian_weight(f32(i), sigma);
            color = color + textureLoad(src, sample_coord, 0) * w;
            weight_sum = weight_sum + w;
        }
    }

    color = color / weight_sum;
    textureStore(dst, coord, color);
}
"#;

const COLOR_MATRIX_SHADER_WGSL: &str = r#"
struct ColorMatrixParams {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    m3: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: ColorMatrixParams;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = vec2<u32>(textureDimensions(src));
    
    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }
    
    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let rgba = vec4<f32>(texel.r, texel.g, texel.b, texel.a);

    let r = dot(params.m0, rgba);
    let g = dot(params.m1, rgba);
    let b = dot(params.m2, rgba);
    let a = dot(params.m3, rgba);

    let out = vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), clamp(a, 0.0, 1.0));
    textureStore(dst, coord, out);
}
"#;

// ── Uniform structs ─────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    radius: f32,
    direction: i32,
    tex_size: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ColorMatrixParams {
    m0: [f32; 4],
    m1: [f32; 4],
    m2: [f32; 4],
    m3: [f32; 4],
}

// ── Backend struct ──────────────────────────────────────────────────────────

/// GPU-backed filter backend that owns its own temporary targets and a
/// dedicated [`RendererCore`] so it never contends with the main renderer.
///
/// Created on-demand per evaluation by cloning the caller's `Device`/`Queue`
/// and spinning up a fresh Vello renderer.  The overhead is acceptable because
/// filter evaluation is only triggered when a `Filter` actor is present and
/// its properties are non-identity.
pub struct GpuFilterBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    core: RendererCore,
    // Render target (Vello draws here)
    render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    // Ping-pong textures for compute shaders
    tex_a: wgpu::Texture,
    tex_a_view: wgpu::TextureView,
    tex_b: wgpu::Texture,
    tex_b_view: wgpu::TextureView,
    // Readback buffer
    output_buffer: wgpu::Buffer,
    bytes_per_row: u32,
    _dimensions: SceneDimensions,
    // Compute pipelines
    blur_pipeline: wgpu::ComputePipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,
    color_matrix_pipeline: wgpu::ComputePipeline,
    color_matrix_bind_group_layout: wgpu::BindGroupLayout,
    // Uniform buffers
    blur_uniform_buffer: wgpu::Buffer,
    color_matrix_uniform_buffer: wgpu::Buffer,
    /// GPU texture view of the most recent filtered result (zero-readback path).
    last_filtered_view: Option<wgpu::TextureView>,
    /// Which internal texture `last_filtered_view` points to, for readback.
    last_filtered_source: FilteredSource,
}

/// Identifies which internal texture holds the filtered result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilteredSource {
    /// The render texture (fast path, no filters applied).
    Render,
    /// Ping-pong texture A.
    TexA,
    /// Ping-pong texture B.
    TexB,
}

impl GpuFilterBackend {
    /// Create a new backend from a cloned device/queue pair.
    ///
    /// # Errors
    /// Returns an error if the inner [`RendererCore`] fails to initialise.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        dimensions: SceneDimensions,
    ) -> Result<Self, String> {
        let core = RendererCore::new(&device, &queue)
            .map_err(|e| format!("Failed to create filter renderer core: {e}"))?;
        let bytes_per_row = (dimensions.width * 4 + 255) & !255;

        // Render target: needs RENDER_ATTACHMENT for Vello + STORAGE_BINDING for compute
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some("Animatix Filter Render Texture"),
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Ping-pong texture A: read by compute (TEXTURE_BINDING), written by compute (STORAGE_BINDING)
        let tex_a = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            label: Some("Animatix Filter PingPong A"),
            view_formats: &[],
        });
        let tex_a_view = tex_a.create_view(&wgpu::TextureViewDescriptor::default());

        // Ping-pong texture B
        let tex_b = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            label: Some("Animatix Filter PingPong B"),
            view_formats: &[],
        });
        let tex_b_view = tex_b.create_view(&wgpu::TextureViewDescriptor::default());

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: (bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Filter Output Buffer"),
            mapped_at_creation: false,
        });

        // ── Blur compute pipeline ──
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(BLUR_SHADER_WGSL)),
        });

        let blur_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Animatix Blur Bind Group Layout"),
                entries: &[
                    // src texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // dst storage texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    // uniform buffer
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Blur Pipeline Layout"),
            bind_group_layouts: &[Some(&blur_bind_group_layout)],
            immediate_size: 0,
        });

        let blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Animatix Blur Pipeline"),
            layout: Some(&blur_pipeline_layout),
            module: &blur_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let blur_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Blur Uniforms"),
            size: std::mem::size_of::<BlurParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Color matrix compute pipeline ──
        let color_matrix_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Color Matrix Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(COLOR_MATRIX_SHADER_WGSL)),
        });

        let color_matrix_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Animatix Color Matrix Bind Group Layout"),
                entries: &[
                    // src texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // dst storage texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    // uniform buffer
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let color_matrix_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Animatix Color Matrix Pipeline Layout"),
                bind_group_layouts: &[Some(&color_matrix_bind_group_layout)],
                immediate_size: 0,
            });

        let color_matrix_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Animatix Color Matrix Pipeline"),
                layout: Some(&color_matrix_pipeline_layout),
                module: &color_matrix_shader,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });

        let color_matrix_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Color Matrix Uniforms"),
            size: std::mem::size_of::<ColorMatrixParams>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            core,
            render_texture,
            render_view,
            tex_a,
            tex_a_view,
            tex_b,
            tex_b_view,
            output_buffer,
            bytes_per_row,
            _dimensions: dimensions,
            blur_pipeline,
            blur_bind_group_layout,
            color_matrix_pipeline,
            color_matrix_bind_group_layout,
            blur_uniform_buffer,
            color_matrix_uniform_buffer,
            last_filtered_view: None,
            last_filtered_source: FilteredSource::Render,
        })
    }

    /// Dispatch a single blur compute pass.
    fn dispatch_blur(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        radius: f32,
        direction: i32,
        width: u32,
        height: u32,
    ) {
        let params = BlurParams {
            radius,
            direction,
            tex_size: [width, height],
        };
        self.queue
            .write_buffer(&self.blur_uniform_buffer, 0, bytemuck::bytes_of(&params));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Blur Bind Group"),
            layout: &self.blur_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.blur_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Animatix Blur Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let dispatch_x = (width + 15) / 16;
            let dispatch_y = (height + 15) / 16;
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }

    /// Dispatch a single color-matrix compute pass.
    fn dispatch_color_matrix(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        matrix: &[[f32; 4]; 4],
    ) {
        let params = ColorMatrixParams {
            m0: matrix[0],
            m1: matrix[1],
            m2: matrix[2],
            m3: matrix[3],
        };
        self.queue.write_buffer(
            &self.color_matrix_uniform_buffer,
            0,
            bytemuck::bytes_of(&params),
        );

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Color Matrix Bind Group"),
            layout: &self.color_matrix_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.color_matrix_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Animatix Color Matrix Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.color_matrix_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Dimensions come from src_view which matches scene dimensions
            let width = self._dimensions.width;
            let height = self._dimensions.height;
            let dispatch_x = (width + 15) / 16;
            let dispatch_y = (height + 15) / 16;
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }
    }

    /// Read a texture back into a [`SceneImage`].
    fn readback_to_scene_image(
        &self,
        texture: &wgpu::Texture,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        let output_buffer = &self.output_buffer;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Filter Readback Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.bytes_per_row),
                    rows_per_image: Some(dimensions.height),
                },
            },
            wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|err| format!("Failed to poll GPU device: {err}"))?;
        rx.recv()
            .map_err(|err| format!("Failed to receive mapped frame: {err}"))?
            .map_err(|err| format!("Failed to map filter frame: {err}"))?;

        let data = buffer_slice.get_mapped_range();
        let mut rgba = vec![0; (dimensions.width * dimensions.height * 4) as usize];
        for y in 0..dimensions.height as usize {
            let src_row = &data[y * self.bytes_per_row as usize
                ..y * self.bytes_per_row as usize + dimensions.width as usize * 4];
            let dst_row = &mut rgba[y * dimensions.width as usize * 4
                ..(y + 1) * dimensions.width as usize * 4];
            dst_row.copy_from_slice(src_row);
        }
        drop(data);
        output_buffer.unmap();

        let data = vello::peniko::ImageData {
            data: rgba.into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: dimensions.width,
            height: dimensions.height,
        };

        Ok(SceneImage {
            data,
            natural_size: [dimensions.width as f32, dimensions.height as f32],
        })
    }
}

impl GpuFilterBackend {
    /// Render and filter a scene, keeping the result on the GPU.
    ///
    /// Returns the [`wgpu::TextureView`] that holds the final filtered image.
    /// The view is also stored in `self.last_filtered_view` so callers can
    /// retrieve it later via [`take_last_filtered_view`](Self::take_last_filtered_view).
    pub fn render_and_filter_scene_to_view(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        blur: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
    ) -> Result<&wgpu::TextureView, String> {
        // 1. Render Vello scene to render texture
        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &self.render_view,
                dimensions.width,
                dimensions.height,
                scene,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;

        let needs_blur = blur > 0.5;
        let needs_color_matrix = (brightness - 1.0).abs() > 0.001
            || (contrast - 1.0).abs() > 0.001
            || (saturate - 1.0).abs() > 0.001
            || hue_rotate.abs() > 0.5
            || sepia > 0.001;

        if !needs_blur && !needs_color_matrix {
            // Fast path: no filters needed, return the render view directly
            self.last_filtered_view = Some(self.render_view.clone());
            self.last_filtered_source = FilteredSource::Render;
            return Ok(self.last_filtered_view.as_ref().unwrap());
        }

        let tex_a_view = &self.tex_a_view;
        let tex_b_view = &self.tex_b_view;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix GPU Filter Encoder"),
            });

        let render_texture = &self.render_texture;
        let tex_a = &self.tex_a;

        // Copy render texture to tex_a as the starting point
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: render_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: tex_a,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
        );

        let mut src_view = tex_a_view;
        let mut dst_view = tex_b_view;

        // Blur passes
        if needs_blur {
            self.dispatch_blur(
                &mut encoder,
                src_view,
                dst_view,
                blur,
                0, // horizontal
                dimensions.width,
                dimensions.height,
            );
            std::mem::swap(&mut src_view, &mut dst_view);

            self.dispatch_blur(
                &mut encoder,
                src_view,
                dst_view,
                blur,
                1, // vertical
                dimensions.width,
                dimensions.height,
            );
            std::mem::swap(&mut src_view, &mut dst_view);
        }

        // Color matrix pass
        if needs_color_matrix {
            let matrix = crate::timeline::filter::compose_color_matrix(
                brightness, contrast, saturate, hue_rotate, sepia,
            );
            self.dispatch_color_matrix(&mut encoder, src_view, dst_view, &matrix);
            std::mem::swap(&mut src_view, &mut dst_view);
        }

        // After all swaps, src_view holds the final result
        self.queue.submit(std::iter::once(encoder.finish()));

        // Determine which texture view is the final result
        let (final_view, source) = if src_view as *const _ == tex_a_view as *const _ {
            (tex_a_view.clone(), FilteredSource::TexA)
        } else {
            (tex_b_view.clone(), FilteredSource::TexB)
        };

        self.last_filtered_view = Some(final_view);
        self.last_filtered_source = source;
        Ok(self.last_filtered_view.as_ref().unwrap())
    }

    /// Take the most recent filtered GPU texture view, if any.
    ///
    /// This clears the internal slot so subsequent calls return `None` until
    /// the next filter render pass completes.
    pub fn take_last_filtered_view(&mut self) -> Option<wgpu::TextureView> {
        self.last_filtered_view.take()
    }
}

impl FilterBackend for GpuFilterBackend {
    fn render_scene_to_image(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &self.render_view,
                dimensions.width,
                dimensions.height,
                scene,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;

        self.readback_to_scene_image(&self.render_texture, dimensions)
    }

    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        blur: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
    ) -> Result<SceneImage, String> {
        self.render_and_filter_scene_to_view(
            scene, dimensions, blur, brightness, contrast, saturate, hue_rotate, sepia,
        )?;

        // Readback from the final texture
        let texture = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        self.readback_to_scene_image(texture, dimensions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: true,
            })
            .await
            .ok()?;
        let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Animatix Filter Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: needed_limits,
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .ok()?;
        Some((device, queue))
    }

    /// Smoke test: GpuFilterBackend can be created and the GPU filter path
    /// produces a valid SceneImage for identity filters.
    #[test]
    fn gpu_filter_backend_identity_filter_produces_image() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let scene = vello::Scene::new();
            let result = backend.render_scene_to_image_gpu_filtered(
                &scene, dims, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0,
            );
            assert!(result.is_ok(), "GPU identity filter path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }

    /// Smoke test: GpuFilterBackend GPU filter path with non-identity blur
    /// produces a valid SceneImage.
    #[test]
    fn gpu_filter_backend_blur_filter_produces_image() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let scene = vello::Scene::new();
            let result = backend.render_scene_to_image_gpu_filtered(
                &scene, dims, 5.0, 1.0, 1.0, 1.0, 0.0, 0.0,
            );
            assert!(result.is_ok(), "GPU blur filter path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }

    /// Smoke test: GpuFilterBackend GPU filter path with non-identity color
    /// matrix produces a valid SceneImage.
    #[test]
    fn gpu_filter_backend_color_matrix_produces_image() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let scene = vello::Scene::new();
            let result = backend.render_scene_to_image_gpu_filtered(
                &scene, dims, 0.0, 1.5, 1.2, 0.5, 45.0, 0.3,
            );
            assert!(result.is_ok(), "GPU color-matrix path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }
}
