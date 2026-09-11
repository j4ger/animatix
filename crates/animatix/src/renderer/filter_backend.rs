//! GPU effect backend for preview and export renderers.
//!
//! Both [`crate::renderer::offscreen::OffscreenRenderer`] and the GUI's
//! `PreviewSurface` need identical offscreen → effect → composite behaviour for
//! [`Filter`](crate::timeline::ActorKindId::Filter) scopes. This module provides
//! a single [`GpuFilterBackend`] that runs an [`EffectChain`] as an ordered list
//! of compute passes over host-owned ping-pong textures.
//!
//! The pass/uniform contract lives in `docs/effects.md`. Each pass is submitted
//! in its own encoder: back-to-back compute passes sharing ping-pong textures in
//! one encoder do not make a storage write visible to the next pass on every
//! driver (probe 009), and a submit boundary is a portable synchronisation
//! point.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::renderer::core::RendererCore;
use crate::timeline::SceneDimensions;
use crate::timeline::filter::{
    EffectChain, EffectDescriptor, EffectId, EffectRegion, FilterBackend, PendingComposite,
    descriptor,
};
use crate::timeline::image::SceneImage;

// ── Uniform structs ─────────────────────────────────────────────────────────

/// Host-owned per-pass context (bind group 0, binding 3).
///
/// Layout must match the `EffectContext` WGSL struct in `timeline/filter.rs`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EffectContextUniform {
    tex_size: [u32; 2],
    _pad0: [u32; 2],
    inv_size: [f32; 2],
    _pad1: [f32; 2],
    pass_index: u32,
    pass_count: u32,
    time_ms: f32,
    _pad2: f32,
}

const EFFECT_CONTEXT_SIZE: u64 = std::mem::size_of::<EffectContextUniform>() as u64;

// ── Backend struct ──────────────────────────────────────────────────────────

/// GPU-backed effect backend that owns its own temporary targets and a
/// dedicated [`RendererCore`] so it never contends with the main renderer.
struct EffectPipeline {
    /// One compute pipeline per pass, index-aligned with the descriptor.
    passes: Vec<wgpu::ComputePipeline>,
    /// Author parameter uniform buffer (binding 2).
    uniform_buffer: wgpu::Buffer,
}

/// GPU-backed filter backend that owns its own temporary targets and a
/// dedicated [`RendererCore`] so it never contends with the main renderer.
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
    // Shared effect binding layout (bindings 0-4) and pipeline layout.
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    /// Lazily built pipelines, keyed by effect identity.
    pipelines: HashMap<EffectId, EffectPipeline>,
    /// Linear clamp sampler (binding 4).
    sampler: wgpu::Sampler,
    /// Per-pass host context uniform (binding 3).
    context_buffer: wgpu::Buffer,
    /// Which internal texture holds the most recent filtered result.
    last_filtered_source: FilteredSource,
    /// Effect-space dimensions of the most recent run (region size when a
    /// region of interest was used, otherwise the full scene).
    last_effect_dims: SceneDimensions,
    /// Pending zero-readback filter textures to be composited after scene render.
    pending_composites: Vec<PendingComposite>,
}

/// Identifies which internal texture holds the filtered result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilteredSource {
    /// The render texture (fast path, no effects applied).
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

        let texture_size = wgpu::Extent3d {
            width: dimensions.width,
            height: dimensions.height,
            depth_or_array_layers: 1,
        };

        // Render target: needs RENDER_ATTACHMENT for Vello + STORAGE_BINDING for compute
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
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

        let ping_pong_usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let tex_a = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter PingPong A"),
            view_formats: &[],
        });
        let tex_a_view = tex_a.create_view(&wgpu::TextureViewDescriptor::default());

        let tex_b = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
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

        // Fixed layout shared by every effect (docs/effects.md §4.2).
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animatix Effect Bind Group Layout"),
            entries: &[
                // 0: input texture
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
                // 1: output storage texture
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
                // 2: author parameters
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
                // 3: host context
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(EFFECT_CONTEXT_SIZE),
                    },
                    count: None,
                },
                // 4: linear sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Effect Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Effect Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let context_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Effect Context Uniforms"),
            size: EFFECT_CONTEXT_SIZE,
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
            bind_group_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            sampler,
            context_buffer,
            last_filtered_source: FilteredSource::Render,
            last_effect_dims: dimensions,
            pending_composites: Vec::new(),
        })
    }

    /// Build (once) the pipelines and uniform buffer for `desc`.
    fn ensure_effect_pipeline(&mut self, desc: &EffectDescriptor) {
        if self.pipelines.contains_key(&desc.id) {
            return;
        }
        let mut passes = Vec::with_capacity(desc.passes.len());
        for pass in desc.passes {
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(pass.label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(pass.wgsl)),
            });
            let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(pass.label),
                layout: Some(&self.pipeline_layout),
                module: &module,
                entry_point: Some(pass.entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
            passes.push(pipeline);
        }
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Effect Uniforms"),
            size: u64::from(desc.author_uniform_size.max(16)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.pipelines.insert(
            desc.id,
            EffectPipeline {
                passes,
                uniform_buffer,
            },
        );
    }

    /// Dispatch one effect pass, src → dst, in its own submitted encoder.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_effect_pass(
        &self,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        pipeline: &wgpu::ComputePipeline,
        uniform_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        pass_index: u32,
        pass_count: u32,
        time_ms: f32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        let context = EffectContextUniform {
            tex_size: [width, height],
            _pad0: [0, 0],
            inv_size: [1.0 / width as f32, 1.0 / height as f32],
            _pad1: [0.0, 0.0],
            pass_index,
            pass_count,
            time_ms,
            _pad2: 0.0,
        };
        self.queue.write_buffer(&self.context_buffer, 0, bytemuck::bytes_of(&context));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Effect Bind Group"),
            layout: &self.bind_group_layout,
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
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.context_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Effect Encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Animatix Effect Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a scene and apply `chain`, keeping the result on the GPU.
    ///
    /// Returns the [`wgpu::TextureView`] holding the final image (the render
    /// texture when the chain is empty). `self.last_filtered_source` records
    /// which texture it is so callers can read it back or copy it. When
    /// `region` is `Some`, the seed copy crops that sub-rect and the chain
    /// dispatches at the region size; the textures themselves stay at the full
    /// scene capacity, so varying regions never reallocate (PF-7).
    fn render_and_filter_scene_to_view(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<&wgpu::TextureView, String> {
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

        if chain.is_empty() {
            self.last_filtered_source = FilteredSource::Render;
            return Ok(&self.render_view);
        }

        // Region of interest: crop the seed copy to `region` and run the chain
        // at the region size. `EffectContext.tex_size` tells the shaders to
        // write only that sub-rect of the (full-size) storage views.
        let (seed_origin, effect_dims) = match region {
            Some(region) => (
                region.origin,
                SceneDimensions {
                    width: region.size.width,
                    height: region.size.height,
                },
            ),
            None => ([0.0, 0.0], dimensions),
        };
        let width = effect_dims.width.max(1);
        let height = effect_dims.height.max(1);
        self.last_effect_dims = effect_dims;

        // Copy the render texture into ping-pong A as the starting point.
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Effect Seed Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.render_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: seed_origin[0].max(0.0) as u32,
                    y: seed_origin[1].max(0.0) as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.tex_a,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let mut current = FilteredSource::TexA;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let effect = descriptor(instance.id);
            self.ensure_effect_pipeline(effect);
            let Some(pipeline) = self.pipelines.get(&effect.id) else {
                continue;
            };

            let mut uniforms = vec![0u8; effect.author_uniform_size as usize];
            (effect.pack)(&instance.params, &mut uniforms);
            self.queue.write_buffer(&pipeline.uniform_buffer, 0, &uniforms);

            let pass_count = pipeline.passes.len() as u32;
            for (index, pass_pipeline) in pipeline.passes.iter().enumerate() {
                let (src_view, dst_view, next) = match current {
                    FilteredSource::TexA => {
                        (&self.tex_a_view, &self.tex_b_view, FilteredSource::TexB)
                    },
                    FilteredSource::TexB => {
                        (&self.tex_b_view, &self.tex_a_view, FilteredSource::TexA)
                    },
                    FilteredSource::Render => {
                        unreachable!("render texture is never a ping-pong source")
                    },
                };
                self.dispatch_effect_pass(
                    src_view,
                    dst_view,
                    pass_pipeline,
                    &pipeline.uniform_buffer,
                    width,
                    height,
                    index as u32,
                    pass_count,
                    chain.time_ms,
                );
                current = next;
            }
        }

        self.last_filtered_source = current;
        Ok(match current {
            FilteredSource::TexA => &self.tex_a_view,
            FilteredSource::TexB => &self.tex_b_view,
            FilteredSource::Render => unreachable!("effects always write a ping-pong texture"),
        })
    }

    /// Read a texture back into a [`SceneImage`].
    fn readback_to_scene_image(
        &self,
        texture: &wgpu::Texture,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        let output_buffer = &self.output_buffer;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            tx.send(result).ok();
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
            let dst_row = &mut rgba
                [y * dimensions.width as usize * 4..(y + 1) * dimensions.width as usize * 4];
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

    /// Copy the most recent filtered view to a dedicated texture for deferred
    /// compositing.
    fn copy_last_filtered_to_pending(
        &self,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
    ) -> Result<PendingComposite, String> {
        let source = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("Animatix Pending Filter Composite Texture"),
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Pending Filter Composite Copy Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
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
        self.queue.submit(std::iter::once(encoder.finish()));

        Ok(PendingComposite {
            texture,
            view,
            alpha,
            origin,
        })
    }
}

impl FilterBackend for GpuFilterBackend {
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String> {
        self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;

        let texture = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        self.readback_to_scene_image(texture, self.last_effect_dims)
    }

    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        let origin = region.map_or([0.0, 0.0], |region| region.origin);
        self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;
        let composite = self.copy_last_filtered_to_pending(self.last_effect_dims, alpha, origin)?;
        self.pending_composites.push(composite);
        Ok(())
    }

    fn take_pending_composites(&mut self) -> Vec<PendingComposite> {
        std::mem::take(&mut self.pending_composites)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::filter::{EffectInstance, EffectParamValue, EffectParams};

    fn blur_chain(radius: f32) -> EffectChain {
        EffectChain {
            instances: vec![EffectInstance {
                id: EffectId::Blur,
                enabled: true,
                params: EffectParams {
                    values: vec![EffectParamValue::F32(radius)],
                },
            }],
            time_ms: 0.0,
        }
    }

    fn color_grade_chain(
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
    ) -> EffectChain {
        EffectChain {
            instances: vec![EffectInstance {
                id: EffectId::ColorGrade,
                enabled: true,
                params: EffectParams {
                    values: vec![
                        EffectParamValue::F32(brightness),
                        EffectParamValue::F32(contrast),
                        EffectParamValue::F32(saturate),
                        EffectParamValue::F32(hue_rotate),
                        EffectParamValue::F32(sepia),
                    ],
                },
            }],
            time_ms: 0.0,
        }
    }

    fn chromatic_aberration_chain(offset: f32) -> EffectChain {
        EffectChain {
            instances: vec![EffectInstance {
                id: EffectId::ChromaticAberration,
                enabled: true,
                params: EffectParams {
                    values: vec![EffectParamValue::F32(offset)],
                },
            }],
            time_ms: 0.0,
        }
    }

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

    /// Smoke test: an empty chain returns a valid image of the right size.
    #[test]
    fn gpu_filter_backend_empty_chain_produces_image() {
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
                &scene,
                dims,
                None,
                &EffectChain::default(),
            );
            assert!(result.is_ok(), "empty chain path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }

    /// Smoke test: the blur chain produces a valid SceneImage.
    #[test]
    fn gpu_filter_backend_blur_chain_produces_image() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let scene = vello::Scene::new();
            let result =
                backend.render_scene_to_image_gpu_filtered(&scene, dims, None, &blur_chain(5.0));
            assert!(result.is_ok(), "GPU blur chain path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }

    /// Smoke test: the colour-grade chain produces a valid SceneImage.
    #[test]
    fn gpu_filter_backend_color_grade_produces_image() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let scene = vello::Scene::new();
            let chain = color_grade_chain(1.5, 1.2, 0.5, 45.0, 0.3);
            let result = backend.render_scene_to_image_gpu_filtered(&scene, dims, None, &chain);
            assert!(result.is_ok(), "GPU color-grade path should succeed");
            let image = result.unwrap();
            assert_eq!(image.natural_size[0], 64.0);
            assert_eq!(image.natural_size[1], 64.0);
        }
    }

    /// Content-level check that chromatic aberration separates channels across
    /// a hard edge: with radial offset, pixels just outside a white region pick
    /// up blue from the far sample while red (sampled further out) stays dark.
    #[test]
    fn chromatic_aberration_fringes_a_hard_edge() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            // White half-plane for x < 32 on a transparent background.
            let mut scene = vello::Scene::new();
            use kurbo::Shape;
            let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
            scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::Color::WHITE,
                None,
                &rect,
            );

            let chain = chromatic_aberration_chain(8.0);
            let image = backend
                .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
                .expect("chromatic aberration path should succeed");
            let w = image.natural_size[0] as usize;
            let raw = image.data.data.data();
            // Pixel right of the boundary, on the horizontal through the
            // centre: blue samples back into the white half-plane, red
            // samples further out into the empty half.
            let (x, y) = (36usize, 32usize);
            let r = raw[(y * w + x) * 4];
            let b = raw[(y * w + x) * 4 + 2];
            assert!(b > r + 60, "expected a blue-dominant fringe at ({x},{y}), got r={r} b={b}");
        }
    }

    /// Region-scoped chains crop the seed copy, dispatch at the region size,
    /// and return a region-sized image: the world sub-rect maps to
    /// region-local coordinates.
    #[test]
    fn region_scoped_chain_crops_and_filters() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            // White half-plane for x < 32 on a transparent background.
            let mut scene = vello::Scene::new();
            use kurbo::Shape;
            let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
            scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::Color::WHITE,
                None,
                &rect,
            );

            let region = EffectRegion {
                origin: [16.0, 16.0],
                size: SceneDimensions {
                    width: 32,
                    height: 32,
                },
            };
            let image = backend
                .render_scene_to_image_gpu_filtered(&scene, dims, Some(region), &blur_chain(4.0))
                .expect("region-scoped chain should succeed");
            assert_eq!(image.natural_size[0], 32.0);
            assert_eq!(image.natural_size[1], 32.0);

            let w = image.natural_size[0] as usize;
            let raw = image.data.data.data();
            // Region-local: world x < 32 maps to local x < 16, so (8, 16) sits
            // well inside the white half-plane and stays opaque.
            let a = raw[(16 * w + 8) * 4 + 3];
            assert!(a > 200, "expected opaque white inside the region, got a={a}");
            // World x = 32 maps to local x = 16; blur must soften the edge.
            let mut soft = false;
            for lx in 10..22usize {
                let a = raw[(16 * w + lx) * 4 + 3];
                if a > 40 && a < 215 {
                    soft = true;
                    break;
                }
            }
            assert!(soft, "blur should soften the world boundary inside the region");
        }
    }

    /// Content-level check that blur actually softens a hard boundary.
    ///
    /// Probe 009 root cause: back-to-back compute passes sharing ping-pong
    /// textures in one encoder did not synchronise; the passes are now split by
    /// submit. This test reads pixels back and verifies a sharp black/white edge
    /// becomes a gradient (intermediate alpha) inside the blur radius.
    #[test]
    fn gpu_filter_blur_softens_a_hard_boundary() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            // Sharp boundary at x=32: left half white, right half empty.
            let mut scene = vello::Scene::new();
            use kurbo::Shape;
            let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
            scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::Color::WHITE,
                None,
                &rect,
            );

            let image = backend
                .render_scene_to_image_gpu_filtered(&scene, dims, None, &blur_chain(8.0))
                .expect("GPU blur path should succeed");
            let w = image.natural_size[0] as usize;
            let raw = image.data.data.data();
            // Middle row: within ±8px of the boundary (x=32), expect an
            // intermediate alpha (soft edge), not a hard 0/255 step.
            let y = 32usize;
            let mut soft = false;
            for x in 20..44usize {
                let a = raw[(y * w + x) * 4 + 3];
                if a > 40 && a < 215 {
                    soft = true;
                    break;
                }
            }
            assert!(
                soft,
                "blur should soften the boundary at x=32, but all sampled pixels look hard"
            );
        }
    }

    /// Check that the colour-grade pass mutates pixels: a red rectangle with
    /// `saturate: 0` must desaturate to gray.
    #[test]
    fn color_matrix_actually_desaturates() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let dims = SceneDimensions {
                width: 64,
                height: 64,
            };
            let mut backend = GpuFilterBackend::new(device, queue, dims)
                .expect("GpuFilterBackend should initialise");

            let mut scene = vello::Scene::new();
            use kurbo::Shape;
            let rect = kurbo::Rect::new(16.0, 16.0, 48.0, 48.0).to_path(1e-3);
            scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::Color::from_rgba8(255, 0, 0, 255),
                None,
                &rect,
            );

            let chain = color_grade_chain(1.0, 1.0, 0.0, 0.0, 0.0);
            let image = backend
                .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
                .expect("color-grade path should succeed");
            let w = image.natural_size[0] as usize;
            let raw = image.data.data.data();
            let (r, g, b) =
                (raw[(32 * w + 32) * 4], raw[(32 * w + 32) * 4 + 1], raw[(32 * w + 32) * 4 + 2]);
            // Desaturated red → channels roughly equal (gray).
            assert!(
                r > 30 && (r as i32 - g as i32).abs() < 40 && (r as i32 - b as i32).abs() < 40,
                "saturate=0 should desaturate red to gray (Rec.709 luma ≈54, not empty), got r={r} g={g} b={b}"
            );
        }
    }
}
