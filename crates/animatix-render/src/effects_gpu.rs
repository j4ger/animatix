//! Portable full-screen effect execution for render-only GPU backends.
//!
//! This executor uses each built-in effect's fragment shader, so WebGL2 can
//! run the same effect chain through render attachments. On native targets,
//! compute-only plugin effects use their original compute passes and ABI.
//! WebAssembly rejects those plugins explicitly because WebGL2 cannot run
//! compute shaders.
//!
//! Inputs and outputs use premultiplied RGBA8, as produced by Vello GPU. The
//! effect ABI remains straight-alpha, so the executor unpremultiplies once at
//! chain entry and premultiplies once at exit. Each effect pass then observes
//! the same pixels and binding semantics as the existing compute backend.

use std::collections::{HashMap, VecDeque};

use animatix::timeline::effects::{Effect, EffectChain, effect};
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

mod helpers;
#[cfg(test)]
mod tests;

use helpers::{
    create_alpha_pipelines, create_effect_bind_group_layout, create_scratch_set,
    encode_effect_pass, encode_fullscreen_pass, scale_pixel_parameters,
};
#[cfg(not(target_arch = "wasm32"))]
use helpers::{create_compute_pipeline_layout, encode_compute_effect_pass};
const MAX_CHAIN_PASSES: usize = 64;
const CONTEXT_STRIDE: u64 = 256;
const MAX_SCRATCH_SETS: usize = 16;
const MAX_BIND_GROUPS: usize = 256;
const AUTHOR_STRIDE_MIN: u64 = 16;
const FULLSCREEN_VERTEX: &str = "fullscreen_vs";
const FRAGMENT_ENTRY: &str = "fragment_main";

/// Raster dimensions and top-left origin of the already-cropped effect input.
/// Effect shaders currently operate in the crop's local pixel coordinates;
/// origin is retained so callers can keep compositing and diagnostics tied to
/// their scene-space ROI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectsExtent {
    pub width: u32,
    pub height: u32,
    pub origin: [f32; 2],
}

/// The filtered premultiplied result. Its texture is owned and reused by this
/// executor; copy or consume it before the next `apply` call if it must remain
/// available across calls.
pub struct EffectsGpuOutput {
    /// Premultiplied RGBA8 view, with `COPY_SRC` usage, valid until its scratch
    /// size is reused by a later call to [`EffectsGpu::apply`].
    pub view: wgpu::TextureView,
}

/// Executes the built-in effect catalog as ordered full-screen render passes.
///
/// One instance belongs to one `(Device, Queue)` pair and can be reused across
/// frames. Scratch sets are keyed by exact extent and bounded to sixteen sizes.
pub struct EffectsGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    context_buffer: wgpu::Buffer,
    alpha_bind_group_layout: wgpu::BindGroupLayout,
    alpha_to_straight: wgpu::RenderPipeline,
    alpha_to_premultiplied: wgpu::RenderPipeline,
    #[cfg(not(target_arch = "wasm32"))]
    compute_bind_group_layout: wgpu::BindGroupLayout,
    #[cfg(not(target_arch = "wasm32"))]
    compute_pipeline_layout: wgpu::PipelineLayout,
    #[cfg(not(target_arch = "wasm32"))]
    compute_pipelines: HashMap<Box<str>, ComputeEffectPipeline>,
    #[cfg(not(target_arch = "wasm32"))]
    compute_bind_group_cache: HashMap<EffectBindGroupCacheKey, wgpu::BindGroup>,
    pipelines: HashMap<Box<str>, EffectPipeline>,
    bind_group_cache: HashMap<EffectBindGroupCacheKey, wgpu::BindGroup>,
    scratch: HashMap<(u32, u32), ScratchSet>,
    scratch_order: VecDeque<(u32, u32)>,
}

struct EffectPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    author_uniform_size: u64,
    author_uniform_stride: u64,
    capacity: usize,
}

#[cfg(not(target_arch = "wasm32"))]
struct ComputeEffectPipeline {
    pipelines: Vec<wgpu::ComputePipeline>,
    uniform_buffer: wgpu::Buffer,
    author_uniform_size: u64,
    author_uniform_stride: u64,
    capacity: usize,
}

#[derive(Clone)]
struct ScratchSet {
    input: Target,
    ping: Target,
    original: Target,
    output: Target,
}

#[derive(Clone)]
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct EffectBindGroupCacheKey {
    src: wgpu::TextureView,
    dst: wgpu::TextureView,
    original: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    author_uniform_size: u64,
}

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

const CONTEXT_SIZE: u64 = std::mem::size_of::<EffectContextUniform>() as u64;

struct PlannedPass {
    effect_type: Box<str>,
    pass_index: u32,
    pass_count: u32,
    author_slot: usize,
    kind: EffectPassKind,
}

#[derive(Clone, Copy)]
enum EffectPassKind {
    Fragment,
    #[cfg(not(target_arch = "wasm32"))]
    Compute,
}

impl EffectsGpu {
    /// Creates a reusable executor for one GPU device and queue.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let bind_group_layout = create_effect_bind_group_layout(&device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Portable Effect Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Portable Effect Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let context_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Portable Effect Context Uniforms"),
            size: CONTEXT_STRIDE * MAX_CHAIN_PASSES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (alpha_bind_group_layout, alpha_to_straight, alpha_to_premultiplied) =
            create_alpha_pipelines(&device);
        #[cfg(not(target_arch = "wasm32"))]
        let (compute_bind_group_layout, compute_pipeline_layout) =
            create_compute_pipeline_layout(&device);
        Self {
            device,
            queue,
            bind_group_layout,
            pipeline_layout,
            sampler,
            context_buffer,
            alpha_bind_group_layout,
            alpha_to_straight,
            alpha_to_premultiplied,
            #[cfg(not(target_arch = "wasm32"))]
            compute_bind_group_layout,
            #[cfg(not(target_arch = "wasm32"))]
            compute_pipeline_layout,
            #[cfg(not(target_arch = "wasm32"))]
            compute_pipelines: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            compute_bind_group_cache: HashMap::new(),
            pipelines: HashMap::new(),
            bind_group_cache: HashMap::new(),
            scratch: HashMap::new(),
            scratch_order: VecDeque::new(),
        }
    }

    /// Applies `chain` to a premultiplied RGBA8 input and returns a
    /// premultiplied RGBA8 output. `raster_scale` scales the seven scene-pixel
    /// effect parameters exactly as the compute executor does; `chain.time_ms`
    /// and ordered pass indices populate the shared host context.
    ///
    /// The input must be a sampleable RGBA8 view on this executor's device. The
    /// caller supplies an already-cropped ROI, so `extent.width` and
    /// `extent.height` must match its texture dimensions. The `origin` is
    /// retained for region ownership and does not alter today's local-space
    /// effect equations.
    pub fn apply(
        &mut self,
        input: &wgpu::TextureView,
        chain: &EffectChain,
        extent: EffectsExtent,
        raster_scale: f32,
    ) -> Result<EffectsGpuOutput, String> {
        validate_extent(input, extent)?;
        if chain.is_empty() {
            return Ok(EffectsGpuOutput {
                view: input.clone(),
            });
        }

        let pass_plan = self.plan_effect_passes(chain, raster_scale)?;
        if pass_plan.is_empty() {
            return Ok(EffectsGpuOutput {
                view: input.clone(),
            });
        }
        let scratch = self.scratch_for(extent.width, extent.height);
        self.write_contexts(&pass_plan, extent.width, extent.height, chain.time_ms);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Portable Effect Encoder"),
        });
        encode_fullscreen_pass(
            &mut encoder,
            &self.alpha_to_straight,
            &self.alpha_bind_group(input),
            &scratch.input.view,
            extent.width,
            extent.height,
        );
        encoder.copy_texture_to_texture(
            scratch.input.texture.as_image_copy(),
            scratch.original.texture.as_image_copy(),
            wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            },
        );

        let mut current = &scratch.input;
        let mut destination = &scratch.ping;
        for (slot, pass) in pass_plan.iter().enumerate() {
            #[cfg(target_arch = "wasm32")]
            let _ = pass.kind;
            #[cfg(not(target_arch = "wasm32"))]
            if let EffectPassKind::Compute = pass.kind {
                let (pipeline, uniform_buffer, author_uniform_size, author_offset) = {
                    let cached = self
                        .compute_pipelines
                        .get(pass.effect_type.as_ref())
                        .expect("compute pipeline is created while planning the chain");
                    (
                        cached.pipelines[pass.pass_index as usize].clone(),
                        cached.uniform_buffer.clone(),
                        cached.author_uniform_size,
                        cached.author_uniform_offset(pass.author_slot)?,
                    )
                };
                let bind_group = self.compute_bind_group_for(
                    &current.view,
                    &destination.view,
                    &scratch.original.view,
                    &uniform_buffer,
                    author_uniform_size,
                );
                let context_offset = u32::try_from(slot as u64 * CONTEXT_STRIDE)
                    .map_err(|error| error.to_string())?;
                encode_compute_effect_pass(
                    &mut encoder,
                    &bind_group,
                    &pipeline,
                    extent.width,
                    extent.height,
                    author_offset,
                    context_offset,
                );
                std::mem::swap(&mut current, &mut destination);
                continue;
            }
            let (pipeline, uniform_buffer, author_uniform_size, author_offset) = {
                let cached = self
                    .pipelines
                    .get(pass.effect_type.as_ref())
                    .expect("pipeline is created while planning the chain");
                (
                    cached.pipeline.clone(),
                    cached.uniform_buffer.clone(),
                    cached.author_uniform_size,
                    cached
                        .author_uniform_offset(pass.author_slot)
                        .map_err(|error| format!("{}: {error}", pass.effect_type))?,
                )
            };
            let bind_group = self.effect_bind_group_for(
                &current.view,
                &destination.view,
                &scratch.original.view,
                &uniform_buffer,
                author_uniform_size,
            );
            let context_offset =
                u32::try_from(slot as u64 * CONTEXT_STRIDE).map_err(|error| error.to_string())?;
            encode_effect_pass(
                &mut encoder,
                &bind_group,
                &pipeline,
                &destination.view,
                extent.width,
                extent.height,
                author_offset,
                context_offset,
            );
            std::mem::swap(&mut current, &mut destination);
        }

        encode_fullscreen_pass(
            &mut encoder,
            &self.alpha_to_premultiplied,
            &self.alpha_bind_group(&current.view),
            &scratch.output.view,
            extent.width,
            extent.height,
        );
        self.queue.submit([encoder.finish()]);
        Ok(EffectsGpuOutput {
            view: scratch.output.view.clone(),
        })
    }

    fn plan_effect_passes(
        &mut self,
        chain: &EffectChain,
        raster_scale: f32,
    ) -> Result<Vec<PlannedPass>, String> {
        let scale = if raster_scale.is_finite() {
            raster_scale.clamp(0.05, 2.0)
        } else {
            1.0
        };
        let mut instances: Vec<(Box<str>, usize, usize, Vec<u8>, EffectPassKind)> = Vec::new();
        let mut slots_by_type: HashMap<Box<str>, usize> = HashMap::new();
        let mut total_passes = 0usize;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let handle = effect(&instance.id).ok_or_else(|| {
                format!("effect '{}' is no longer registered", instance.id.as_str())
            })?;
            let descriptor = handle.as_effect();
            let passes = descriptor.passes();
            if passes.is_empty() {
                continue;
            }
            let kind = if descriptor.fragment_shader().is_some() {
                self.ensure_pipeline(descriptor)?;
                EffectPassKind::Fragment
            } else {
                #[cfg(target_arch = "wasm32")]
                return Err(format!(
                    "effect '{}' uses compute shaders, which are unavailable on WebGL2",
                    descriptor.type_name()
                ));

                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.ensure_compute_pipeline(descriptor)?;
                    EffectPassKind::Compute
                }
            };
            total_passes = total_passes
                .checked_add(passes.len())
                .ok_or_else(|| "effect pass count overflowed".to_string())?;
            let author_slot = slots_by_type.entry(descriptor.type_name().into()).or_default();
            let slot = *author_slot;
            *author_slot += 1;
            let mut uniforms = vec![0u8; descriptor.author_uniform_size().max(16) as usize];
            descriptor.pack(&instance.params, &mut uniforms);
            scale_pixel_parameters(descriptor.params(), scale, &mut uniforms);
            instances.push((descriptor.type_name().into(), passes.len(), slot, uniforms, kind));
        }
        if total_passes > MAX_CHAIN_PASSES {
            return Err(format!(
                "effect chain requires {total_passes} fragment passes; the host context buffer supports {MAX_CHAIN_PASSES}"
            ));
        }

        for (effect_type, slots) in &slots_by_type {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(pipeline) = self.compute_pipelines.get_mut(effect_type.as_ref()) {
                if pipeline.ensure_capacity(&self.device, *slots)? {
                    self.compute_bind_group_cache.clear();
                }
                continue;
            }
            let pipeline = self
                .pipelines
                .get_mut(effect_type.as_ref())
                .expect("pipeline is created before planning uniforms");
            if pipeline.ensure_capacity(&self.device, *slots)? {
                self.bind_group_cache.clear();
            }
        }

        let mut plan = Vec::with_capacity(total_passes);
        for (effect_type, pass_count, author_slot, uniforms, kind) in instances {
            #[cfg(not(target_arch = "wasm32"))]
            if let EffectPassKind::Compute = kind {
                let pipeline = self
                    .compute_pipelines
                    .get(effect_type.as_ref())
                    .expect("compute pipeline is created before writing uniforms");
                let offset = pipeline.author_uniform_offset(author_slot)?;
                self.queue.write_buffer(&pipeline.uniform_buffer, u64::from(offset), &uniforms);
                for pass_index in 0..pass_count {
                    plan.push(PlannedPass {
                        effect_type: effect_type.clone(),
                        pass_index: pass_index as u32,
                        pass_count: pass_count as u32,
                        author_slot,
                        kind,
                    });
                }
                continue;
            }
            let pipeline = self
                .pipelines
                .get(effect_type.as_ref())
                .expect("pipeline is created before writing uniforms");
            let offset = pipeline.author_uniform_offset(author_slot)?;
            self.queue.write_buffer(&pipeline.uniform_buffer, u64::from(offset), &uniforms);
            for pass_index in 0..pass_count {
                plan.push(PlannedPass {
                    effect_type: effect_type.clone(),
                    pass_index: pass_index as u32,
                    pass_count: pass_count as u32,
                    author_slot,
                    kind,
                });
            }
        }
        Ok(plan)
    }

    fn ensure_pipeline(&mut self, effect: &(dyn Effect + 'static)) -> Result<(), String> {
        if self.pipelines.contains_key(effect.type_name()) {
            return Ok(());
        }
        let shader_source = effect.fragment_shader().ok_or_else(|| {
            format!("effect '{}' has no portable fragment shader", effect.type_name())
        })?;
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(effect.type_name()),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(effect.type_name()),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(FULLSCREEN_VERTEX),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(FRAGMENT_ENTRY),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let author_uniform_size = u64::from(effect.author_uniform_size().max(16));
        let alignment = u64::from(self.device.limits().min_uniform_buffer_offset_alignment.max(1));
        let author_uniform_stride =
            author_uniform_size.max(AUTHOR_STRIDE_MIN).div_ceil(alignment) * alignment;
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Portable Effect Instance Uniforms"),
            size: author_uniform_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.pipelines.insert(
            effect.type_name().into(),
            EffectPipeline {
                pipeline,
                uniform_buffer,
                author_uniform_size,
                author_uniform_stride,
                capacity: 1,
            },
        );
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_compute_pipeline(&mut self, effect: &(dyn Effect + 'static)) -> Result<(), String> {
        if self.compute_pipelines.contains_key(effect.type_name()) {
            return Ok(());
        }
        let mut pipelines = Vec::with_capacity(effect.passes().len());
        for pass in effect.passes() {
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(pass.label.as_ref()),
                source: wgpu::ShaderSource::Wgsl(pass.wgsl.clone()),
            });
            pipelines.push(self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(pass.label.as_ref()),
                layout: Some(&self.compute_pipeline_layout),
                module: &module,
                entry_point: Some(pass.entry.as_ref()),
                compilation_options: Default::default(),
                cache: None,
            }));
        }
        let author_uniform_size = u64::from(effect.author_uniform_size().max(16));
        let alignment = u64::from(self.device.limits().min_uniform_buffer_offset_alignment.max(1));
        let author_uniform_stride =
            author_uniform_size.max(AUTHOR_STRIDE_MIN).div_ceil(alignment) * alignment;
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Compute Plugin Instance Uniforms"),
            size: author_uniform_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.compute_pipelines.insert(
            effect.type_name().into(),
            ComputeEffectPipeline {
                pipelines,
                uniform_buffer,
                author_uniform_size,
                author_uniform_stride,
                capacity: 1,
            },
        );
        Ok(())
    }

    fn write_contexts(&self, plan: &[PlannedPass], width: u32, height: u32, time_ms: f32) {
        if plan.is_empty() {
            return;
        }
        let mut bytes = vec![0u8; plan.len() * CONTEXT_STRIDE as usize];
        for (slot, pass) in plan.iter().enumerate() {
            let context = EffectContextUniform {
                tex_size: [width, height],
                _pad0: [0, 0],
                inv_size: [1.0 / width as f32, 1.0 / height as f32],
                _pad1: [0.0, 0.0],
                pass_index: pass.pass_index,
                pass_count: pass.pass_count,
                time_ms,
                _pad2: 0.0,
            };
            let offset = slot * CONTEXT_STRIDE as usize;
            bytes[offset..offset + CONTEXT_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&context));
        }
        self.queue.write_buffer(&self.context_buffer, 0, &bytes);
    }

    fn scratch_for(&mut self, width: u32, height: u32) -> ScratchSet {
        let key = (width, height);
        if !self.scratch.contains_key(&key) {
            while self.scratch.len() >= MAX_SCRATCH_SETS {
                if let Some(evict) = self.scratch_order.pop_front() {
                    self.scratch.remove(&evict);
                    self.bind_group_cache.clear();
                } else {
                    break;
                }
            }
            self.scratch.insert(key, create_scratch_set(&self.device, width, height));
            self.scratch_order.push_back(key);
        } else {
            self.scratch_order.retain(|candidate| *candidate != key);
            self.scratch_order.push_back(key);
        }
        self.scratch.get(&key).expect("scratch set inserted").clone()
    }

    fn alpha_bind_group(&self, src: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Alpha Conversion Bind Group"),
            layout: &self.alpha_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
            }],
        })
    }

    fn effect_bind_group_for(
        &mut self,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        original: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
        author_uniform_size: u64,
    ) -> wgpu::BindGroup {
        let key = EffectBindGroupCacheKey {
            src: src.clone(),
            dst: dst.clone(),
            original: original.clone(),
            uniform_buffer: uniform_buffer.clone(),
            author_uniform_size,
        };
        if let Some(cached) = self.bind_group_cache.get(&key) {
            return cached.clone();
        }
        if self.bind_group_cache.len() >= MAX_BIND_GROUPS {
            self.bind_group_cache.clear();
        }
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Portable Effect Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(author_uniform_size),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.context_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(CONTEXT_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(original),
                },
            ],
        });
        self.bind_group_cache.insert(key, bind_group.clone());
        bind_group
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn compute_bind_group_for(
        &mut self,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        original: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
        author_uniform_size: u64,
    ) -> wgpu::BindGroup {
        let key = EffectBindGroupCacheKey {
            src: src.clone(),
            dst: dst.clone(),
            original: original.clone(),
            uniform_buffer: uniform_buffer.clone(),
            author_uniform_size,
        };
        if let Some(cached) = self.compute_bind_group_cache.get(&key) {
            return cached.clone();
        }
        if self.compute_bind_group_cache.len() >= MAX_BIND_GROUPS {
            self.compute_bind_group_cache.clear();
        }
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Native Compute Effect Bind Group"),
            layout: &self.compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dst),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(author_uniform_size),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.context_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(CONTEXT_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(original),
                },
            ],
        });
        self.compute_bind_group_cache.insert(key, bind_group.clone());
        bind_group
    }
}

impl EffectPipeline {
    fn ensure_capacity(&mut self, device: &wgpu::Device, slots: usize) -> Result<bool, String> {
        if slots <= self.capacity {
            return Ok(false);
        }
        let capacity = slots
            .checked_next_power_of_two()
            .ok_or_else(|| "portable effect uniform capacity overflowed".to_string())?;
        let size = self
            .author_uniform_stride
            .checked_mul(capacity as u64)
            .ok_or_else(|| "portable effect uniform buffer size overflowed".to_string())?;
        self.uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Portable Effect Instance Uniforms"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = capacity;
        Ok(true)
    }

    fn author_uniform_offset(&self, slot: usize) -> Result<u32, String> {
        let offset = self
            .author_uniform_stride
            .checked_mul(slot as u64)
            .ok_or_else(|| "portable effect uniform offset overflowed".to_string())?;
        u32::try_from(offset).map_err(|_| "portable effect uniform offset exceeds u32".into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ComputeEffectPipeline {
    fn ensure_capacity(&mut self, device: &wgpu::Device, slots: usize) -> Result<bool, String> {
        if slots <= self.capacity {
            return Ok(false);
        }
        let capacity = slots
            .checked_next_power_of_two()
            .ok_or_else(|| "compute effect uniform capacity overflowed".to_string())?;
        let size = self
            .author_uniform_stride
            .checked_mul(capacity as u64)
            .ok_or_else(|| "compute effect uniform buffer size overflowed".to_string())?;
        self.uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Compute Plugin Instance Uniforms"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.capacity = capacity;
        Ok(true)
    }

    fn author_uniform_offset(&self, slot: usize) -> Result<u32, String> {
        let offset = self
            .author_uniform_stride
            .checked_mul(slot as u64)
            .ok_or_else(|| "compute effect uniform offset overflowed".to_string())?;
        u32::try_from(offset).map_err(|_| "compute effect uniform offset exceeds u32".into())
    }
}

fn validate_extent(input: &wgpu::TextureView, extent: EffectsExtent) -> Result<(), String> {
    let texture = input.texture();
    if texture.format() != FORMAT {
        return Err("portable effects require an RGBA8 input".into());
    }
    if !texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING) {
        return Err("portable effect input must support texture sampling".into());
    }
    if extent.width == 0 || extent.height == 0 {
        return Err("portable effect extent must be nonzero".into());
    }
    if (texture.width(), texture.height()) != (extent.width, extent.height) {
        return Err(format!(
            "portable effect input is {}x{}, but extent is {}x{}",
            texture.width(),
            texture.height(),
            extent.width,
            extent.height
        ));
    }
    Ok(())
}
