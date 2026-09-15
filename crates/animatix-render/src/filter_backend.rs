//! GPU effect backend for preview and export renderers.
//!
//! Both [`crate::offscreen::OffscreenRenderer`] and the GUI's
//! `PreviewSurface` need identical offscreen → effect → composite behaviour for
//! `Filter` scopes. This module provides a single [`GpuFilterBackend`] that runs
//! an [`EffectChain`] as an ordered list of compute passes over host-owned
//! ping-pong textures.
//!
//! The pass/uniform contract lives in `docs/effects.md`. Each pass is submitted
//! in its own encoder: back-to-back compute passes sharing ping-pong textures in
//! one encoder do not make a storage write visible to the next pass on every
//! driver (probe 009), and a submit boundary is a portable synchronisation
//! point.

use std::collections::HashMap;

use crate::core::RendererCore;
use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{
    Effect, EffectChain, EffectRegion, FilterBackend, PendingComposite, effect,
};
use animatix::timeline::image::SceneImage;

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

/// Dynamic-offset stride between per-pass context uniforms. 256 covers every
/// backend's `min_uniform_buffer_offset_alignment` and keeps offsets aligned.
const CONTEXT_STRIDE: u32 = 256;

/// Maximum effect passes dispatchable in one chain (context slots available).
const MAX_CHAIN_PASSES: u32 = 64;

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
    /// Lazily built pipelines, keyed by the effect's authored type name.
    ///
    /// The key is owned because plugin effect names come from the registry, not
    /// from string literals.
    pipelines: HashMap<Box<str>, EffectPipeline>,
    /// Linear clamp sampler (binding 4).
    sampler: wgpu::Sampler,
    /// Per-pass host context uniform (binding 3).
    context_buffer: wgpu::Buffer,
    /// Which internal texture holds the most recent filtered result.
    last_filtered_source: FilteredSource,
    /// Effect-space dimensions of the most recent run (region size when a
    /// region of interest was used, otherwise the full scene).
    last_effect_dims: SceneDimensions,
    /// The region the caller will harvest (full canvas when `None`); the chain
    /// always runs over the full canvas, this only scopes the readback/copy.
    last_region: Option<EffectRegion>,
    /// Pending zero-readback filter textures to be composited after scene render.
    pending_composites: Vec<PendingComposite>,
    /// Region-scoped scratch targets, keyed by quantized size. Region paths
    /// render, seed, ping-pong, and harvest entirely inside these textures so
    /// per-scope cost scales with the scope, not the canvas (PF-7's
    /// no-reallocation guarantee is preserved by quantizing sizes and capping
    /// the cache: steady-state scenes reuse one entry per distinct ROI).
    region_scratch: HashMap<(u32, u32), RegionScratch>,
    /// Ping-pong texture of the most recent *region-scoped* run, when one was
    /// used. Its contents cover `last_effect_dims` starting at (0, 0).
    last_region_pp: Option<(wgpu::Texture, wgpu::TextureView)>,
}

/// Region-scoped scratch targets for one quantized ROI size.
#[derive(Clone)]
struct RegionScratch {
    render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    pp_a: wgpu::Texture,
    pp_a_view: wgpu::TextureView,
    pp_b: wgpu::Texture,
    pp_b_view: wgpu::TextureView,
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
    /// TEMPORARY stage dump for debugging the multi-scope video loss.
    fn dump_stage(&self, label: &str, texture: &wgpu::Texture, dims: SceneDimensions) {
        if std::env::var_os("ANIMATIX_DUMP_STAGES").is_none() {
            return;
        }
        let img = self
            .readback_to_scene_image_at(texture, wgpu::Origin3d::ZERO, dims)
            .expect("stage dump readback");
        let data = img.data.data.data();
        let n = data.len() / 4;
        let step = (n / 300).max(1);
        let (mut opaque, mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u64, 0u64, 0u64);
        for i in (0..n).step_by(step) {
            let (r, g, b, a) = (
                data[i * 4] as u32,
                data[i * 4 + 1] as u32,
                data[i * 4 + 2] as u32,
                data[i * 4 + 3] as u32,
            );
            if a > 100 {
                opaque += 1;
                sum_r += r as u64;
                sum_g += g as u64;
                sum_b += b as u64;
            }
        }
        let total = (n as u32).div_ceil(step as u32);
        let _ = std::fs::create_dir_all("/tmp/stages");
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/stages/{seq:03}_{label}.png");
        let _ = image::save_buffer(&path, data, dims.width, dims.height, image::ColorType::Rgba8);
        eprintln!(
            "[stages] {seq:03} {label}: opaque={opaque}/{total} avg_rgb=({},{},{})",
            if opaque > 0 { sum_r / opaque as u64 } else { 0 },
            if opaque > 0 { sum_g / opaque as u64 } else { 0 },
            if opaque > 0 { sum_b / opaque as u64 } else { 0 },
        );
    }

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
                // 3: host context (dynamic offset — one buffer serves every
                // pass of a chain so seed + passes encode in one submit)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
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
            size: u64::from(CONTEXT_STRIDE * MAX_CHAIN_PASSES),
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
            last_region: None,
            pending_composites: Vec::new(),
            region_scratch: HashMap::new(),
            last_region_pp: None,
        })
    }

    /// Build (once) the pipelines and uniform buffer for `effect`.
    fn ensure_effect_pipeline(&mut self, effect: &dyn Effect) {
        if self.pipelines.contains_key(effect.type_name()) {
            return;
        }
        let mut passes = Vec::with_capacity(effect.passes().len());
        for pass in effect.passes() {
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(pass.label.as_ref()),
                source: wgpu::ShaderSource::Wgsl(pass.wgsl.clone()),
            });
            let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(pass.label.as_ref()),
                layout: Some(&self.pipeline_layout),
                module: &module,
                entry_point: Some(pass.entry.as_ref()),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
            passes.push(pipeline);
        }
        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Effect Uniforms"),
            size: u64::from(effect.author_uniform_size().max(16)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.pipelines.insert(
            effect.type_name().into(),
            EffectPipeline {
                passes,
                uniform_buffer,
            },
        );
    }

    /// Dispatch one effect pass, src → dst, in its own submitted encoder.
    #[allow(clippy::too_many_arguments)]
    /// Bind group for one pass: views plus the context uniform at `slot`'s
    /// dynamic offset in the shared context buffer.
    fn effect_bind_group_for_slot(
        &self,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.context_buffer,
                        offset: 0,
                        size: Some(
                            std::num::NonZeroU64::new(EFFECT_CONTEXT_SIZE).expect("nonzero"),
                        ),
                    }),
                    // The dynamic offset itself is supplied in set_bind_group.
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Encode one effect pass into `encoder` (dispatch at `width`×`height`,
    /// context uniform at the bind group's dynamic offset).
    fn encode_effect_pass(
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        pass_pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        slot_offset: u32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Animatix Effect Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pass_pipeline);
            pass.set_bind_group(0, bind_group, &[slot_offset]);
            pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
        }
    }

    /// Render a scene and apply `chain`, keeping the result on the GPU.
    ///
    /// Returns the [`wgpu::TextureView`] holding the final image. The result
    /// location is recorded on `self`: `last_region_pp` for the region-scoped
    /// path (contents cover `last_effect_dims` from (0, 0)), or
    /// `last_filtered_source` + `last_effect_dims` for the full-canvas path.
    ///
    /// Region-scoped scopes render, seed, and dispatch entirely at region
    /// size, so per-scope cost scales with the scope instead of the canvas.
    /// Dispatch extent == seed extent == the shaders' UV space, which makes
    /// the stale-texel hazard that originally forced full-canvas dispatch (a
    /// cropped seed read through region-independent UVs; found by the
    /// effects-wave1 dogfood when a moving `MotionBlur` card dragged opaque
    /// backdrop garbage with it) impossible by construction. Shaders address
    /// `tex_size`-relative pixels, so pixel-denominated parameters (blur
    /// radius, offsets) keep their meaning at region size.
    fn render_and_filter_scene_to_view(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<wgpu::TextureView, String> {
        let mut wrapped;
        let (dispatch_dims, scene_ref, scratch) = match region {
            Some(r) => {
                let (width, height) = Self::quantize_region_size(r.size, dimensions);
                wrapped = vello::Scene::new();
                wrapped.append(
                    scene,
                    Some(kurbo::Affine::translate(kurbo::Vec2::new(
                        -f64::from(r.origin[0]),
                        -f64::from(r.origin[1]),
                    ))),
                );
                (
                    SceneDimensions { width, height },
                    &wrapped,
                    Some(self.region_scratch_for(width, height)),
                )
            },
            None => (dimensions, scene, None),
        };
        let scratch = scratch.map(std::sync::Arc::new);
        let (render_view, seed_texture, pp_a, pp_a_view, pp_b, pp_b_view) = match &scratch {
            Some(s) => (
                s.render_view.clone(),
                s.render_texture.clone(),
                s.pp_a.clone(),
                s.pp_a_view.clone(),
                s.pp_b.clone(),
                s.pp_b_view.clone(),
            ),
            None => (
                self.render_view.clone(),
                self.render_texture.clone(),
                self.tex_a.clone(),
                self.tex_a_view.clone(),
                self.tex_b.clone(),
                self.tex_b_view.clone(),
            ),
        };

        // TEMPORARY perf probe (env-gated): per-stage cost of one scope.
        let timing = std::env::var_os("ANIMATIX_FILTER_TIMING").is_some();
        let t_all = std::time::Instant::now();

        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &render_view,
                dispatch_dims.width,
                dispatch_dims.height,
                scene_ref,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;
        let t_render = t_all.elapsed();

        self.dump_stage("render_view", &seed_texture, dispatch_dims);

        // Harvest extent: the true region size (dispatch may be quantized
        // larger; the padding is never harvested).
        self.last_effect_dims = match region {
            Some(r) => r.size,
            None => dimensions,
        };
        self.last_region = region;
        self.last_region_pp = None;

        if chain.is_empty() {
            self.last_filtered_source = FilteredSource::Render;
            if region.is_some() {
                // The unfiltered result lives in the region render target.
                self.last_region_pp = Some((seed_texture.clone(), render_view.clone()));
            }
            return Ok(render_view);
        }

        // Seed the ping-pong with the rendered frame, then run every pass —
        // all in ONE command encoder and ONE submit. Same-queue submissions are
        // ordered, and each pass reads its own context through a dynamic
        // offset, so the batch is correct and the chain costs one submit
        // instead of one per stage.
        let width = dispatch_dims.width.max(1);
        let height = dispatch_dims.height.max(1);
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Effect Chain Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &pp_a,
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
        self.dump_stage("tex_a_seed", &pp_a, dispatch_dims);
        let t_seed = t_all.elapsed() - t_render;

        // Plan the passes, write every context once (one slot per pass), then
        // encode seed + passes into a single encoder.
        // Owns cloned wgpu handles so plan entries never borrow `self` across
        // the mutable pipeline-ensure calls.
        let mut pass_plan: Vec<(wgpu::ComputePipeline, wgpu::Buffer, u32, u32)> = Vec::new();
        let mut current = FilteredSource::TexA;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let Some(effect) = effect(&instance.id) else {
                tracing::warn!("chain effect has no registered descriptor; skipping");
                continue;
            };
            self.ensure_effect_pipeline(effect.as_effect());
            let Some(pipeline) = self.pipelines.get(effect.type_name()) else {
                continue;
            };

            let mut uniforms = vec![0u8; effect.author_uniform_size() as usize];
            effect.pack(&instance.params, &mut uniforms);
            // Author-parameter writes land before the encoder's commands on
            // submit; each effect has its own uniform buffer, so writes cannot
            // collide across effects.
            self.queue.write_buffer(&pipeline.uniform_buffer, 0, &uniforms);

            let pass_count = pipeline.passes.len() as u32;
            for (index, pass_pipeline) in pipeline.passes.iter().enumerate() {
                pass_plan.push((
                    pass_pipeline.clone(),
                    pipeline.uniform_buffer.clone(),
                    index as u32,
                    pass_count,
                ));
            }
        }

        let mut context_bytes = vec![0u8; (CONTEXT_STRIDE * MAX_CHAIN_PASSES) as usize];
        for (slot, (pass_pipeline, uniform_buffer, pass_index, pass_count)) in
            pass_plan.iter().enumerate()
        {
            let context = EffectContextUniform {
                tex_size: [width, height],
                _pad0: [0, 0],
                inv_size: [1.0 / width as f32, 1.0 / height as f32],
                _pad1: [0.0, 0.0],
                pass_index: *pass_index,
                pass_count: *pass_count,
                time_ms: chain.time_ms,
                _pad2: 0.0,
            };
            let base = (slot as u32 * CONTEXT_STRIDE) as usize;
            context_bytes[base..base + EFFECT_CONTEXT_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&context));
            let bind_group = self.effect_bind_group_for_slot(
                if matches!(current, FilteredSource::TexA) {
                    &pp_a_view
                } else {
                    &pp_b_view
                },
                if matches!(current, FilteredSource::TexA) {
                    &pp_b_view
                } else {
                    &pp_a_view
                },
                uniform_buffer,
            );
            Self::encode_effect_pass(
                &mut encoder,
                &bind_group,
                pass_pipeline,
                width,
                height,
                slot as u32 * CONTEXT_STRIDE,
            );
            current = match current {
                FilteredSource::TexA => FilteredSource::TexB,
                _ => FilteredSource::TexA,
            };
        }
        self.queue.write_buffer(&self.context_buffer, 0, &context_bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        let (result_texture, result_view) = match current {
            FilteredSource::TexA => (pp_a, pp_a_view),
            FilteredSource::TexB => (pp_b, pp_b_view),
            FilteredSource::Render => unreachable!("effects always write a ping-pong texture"),
        };
        self.dump_stage("tex_b_fx", &result_texture, dispatch_dims);
        let t_passes = t_all.elapsed() - t_seed - t_render;
        if timing {
            eprintln!(
                "[filter-timing] dims={}x{} stages={} render={:.2}ms seed={:.2}ms passes={:.2}ms",
                dispatch_dims.width,
                dispatch_dims.height,
                chain.instances.len(),
                t_render.as_secs_f64() * 1000.0,
                t_seed.as_secs_f64() * 1000.0,
                t_passes.as_secs_f64() * 1000.0,
            );
        }

        self.last_filtered_source = current;
        if region.is_some() {
            // Region results live in the region scratch ping-pong, covering
            // `last_effect_dims` from (0, 0).
            self.last_region_pp = Some((result_texture.clone(), result_view.clone()));
        }
        Ok(result_view)
    }

    /// Quantize a region size up to 64-px steps (clamped to the canvas) so a
    /// moving scope reuses one scratch allocation instead of churning.
    fn quantize_region_size(size: SceneDimensions, canvas: SceneDimensions) -> (u32, u32) {
        let step = |v: u32, max: u32| (v.div_ceil(64).max(1) * 64).min(max.max(1));
        (step(size.width, canvas.width), step(size.height, canvas.height))
    }

    /// Get (creating on first use) the region-scoped scratch targets for a
    /// quantized size. Textures are cloned out (refcount bumps) so the cache
    /// stays owned by the backend across the mutable render call.
    fn region_scratch_for(&mut self, width: u32, height: u32) -> RegionScratch {
        const MAX_REGION_SCRATCH_ENTRIES: usize = 12;
        if self.region_scratch.len() > MAX_REGION_SCRATCH_ENTRIES {
            // A scene uses a handful of distinct ROIs; if churn ever exceeds
            // the cap, start over rather than growing unbounded.
            self.region_scratch.clear();
        }
        if let Some(scratch) = self.region_scratch.get(&(width, height)) {
            return scratch.clone();
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let make = |label: &str| {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                label: Some(label),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let (render_texture, render_view) = make("Animatix Region Render Target");
        let (pp_a, pp_a_view) = make("Animatix Region Ping A");
        let (pp_b, pp_b_view) = make("Animatix Region Ping B");
        let scratch = RegionScratch {
            render_texture,
            render_view,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
        };
        self.region_scratch.insert((width, height), scratch.clone());
        scratch
    }

    /// Debug instrumentation: read a texture region back as a [`SceneImage`].
    pub fn debug_readback_texture(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
        dims: SceneDimensions,
    ) -> SceneImage {
        self.readback_to_scene_image_at(texture, origin, dims)
            .expect("debug texture readback")
    }

    /// Read a texture back into a [`SceneImage`].
    fn readback_to_scene_image_at(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
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
                origin,
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

    /// Copy the most recent filtered result to a dedicated texture for
    /// deferred compositing.
    fn copy_last_filtered_to_pending(
        &self,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
        source: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
    ) -> Result<PendingComposite, String> {
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
                origin: source_origin,
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

        if let Some((texture, _)) = &self.last_region_pp {
            // Region-scoped result: contents cover `last_effect_dims` from
            // (0, 0) inside the scratch texture.
            let t = std::time::Instant::now();
            let image = self.readback_to_scene_image_at(
                texture,
                wgpu::Origin3d::ZERO,
                self.last_effect_dims,
            );
            if std::env::var_os("ANIMATIX_FILTER_TIMING").is_some() {
                eprintln!(
                    "[filter-timing] readback dims={}x{} took={:.2}ms",
                    self.last_effect_dims.width,
                    self.last_effect_dims.height,
                    t.elapsed().as_secs_f64() * 1000.0,
                );
            }
            return image;
        }

        let texture = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        let (origin, dims) = match self.last_region {
            Some(region) => (
                wgpu::Origin3d {
                    x: region.origin[0].max(0.0) as u32,
                    y: region.origin[1].max(0.0) as u32,
                    z: 0,
                },
                region.size,
            ),
            None => (wgpu::Origin3d::ZERO, self.last_effect_dims),
        };
        self.readback_to_scene_image_at(texture, origin, dims)
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
        let t_copy = std::time::Instant::now();
        self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;
        let t_after_render = t_copy.elapsed();
        let harvest = self.last_effect_dims;
        let (source, source_origin) = if let Some((texture, _)) = &self.last_region_pp {
            // Region-scoped result: the scratch ping-pong covers
            // `last_effect_dims` from (0, 0).
            (texture, wgpu::Origin3d::ZERO)
        } else {
            let texture = match self.last_filtered_source {
                FilteredSource::Render => &self.render_texture,
                FilteredSource::TexA => &self.tex_a,
                FilteredSource::TexB => &self.tex_b,
            };
            let harvest_origin = self.last_region.map_or([0.0, 0.0], |region| region.origin);
            (
                texture,
                wgpu::Origin3d {
                    x: harvest_origin[0].max(0.0) as u32,
                    y: harvest_origin[1].max(0.0) as u32,
                    z: 0,
                },
            )
        };
        let composite =
            self.copy_last_filtered_to_pending(harvest, alpha, origin, source, source_origin)?;
        if std::env::var_os("ANIMATIX_FILTER_TIMING").is_some() {
            eprintln!(
                "[filter-timing] pending dims={}x{} render+chain={:.2}ms copy_alloc={:.2}ms",
                harvest.width,
                harvest.height,
                t_after_render.as_secs_f64() * 1000.0,
                (t_copy.elapsed() - t_after_render).as_secs_f64() * 1000.0,
            );
        }
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
    use animatix::timeline::effects::{EffectId, EffectInstance, EffectParamValue, EffectParams};

    fn blur_chain(radius: f32) -> EffectChain {
        EffectChain {
            instances: vec![EffectInstance {
                id: EffectId::new("Blur"),
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
                id: EffectId::new("ColorGrade"),
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
                id: EffectId::new("ChromaticAberration"),
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

    /// One-stage chain for any built-in effect, addressed by its authored name.
    fn effect_chain(name: &str, values: Vec<EffectParamValue>) -> EffectChain {
        EffectChain {
            instances: vec![EffectInstance {
                id: EffectId::new(name),
                enabled: true,
                params: EffectParams { values },
            }],
            time_ms: 0.0,
        }
    }

    fn filled_scene(color: vello::peniko::Color, rect: kurbo::Rect) -> vello::Scene {
        let mut scene = vello::Scene::new();
        let path = kurbo::Shape::to_path(&rect, 1e-3);
        scene.fill(vello::peniko::Fill::NonZero, kurbo::Affine::IDENTITY, color, None, &path);
        scene
    }

    /// Vignette must darken the corner while leaving the center untouched.
    #[test]
    fn vignette_darkens_corners_not_center() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let scene =
            filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
        let chain = effect_chain(
            "Vignette",
            vec![
                EffectParamValue::F32(1.0),  // amount
                EffectParamValue::F32(0.15), // radius (start close to center)
                EffectParamValue::F32(0.9),  // softness
                EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
            ],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("vignette path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();

        let center = raw[(32 * w + 32) * 4];
        let corner = raw[(2 * w + 2) * 4];
        assert!(center > 240, "center must keep its luminance, got {center}");
        assert!(corner < 120, "corner must fall into the vignette color, got {corner}");
    }

    /// Levels: lifting the black point above the input gray must floor it to
    /// the (raised) black output — the classic contrast-crush check.
    #[test]
    fn levels_black_point_maps_below_floor_to_black() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 32,
            height: 32,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // 25% gray fills the canvas.
        let scene = filled_scene(
            vello::peniko::Color::from_rgba8(64, 64, 64, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );
        let chain = effect_chain(
            "Levels",
            vec![
                EffectParamValue::F32(0.3), // in_black above the input gray
                EffectParamValue::F32(1.0), // in_white
                EffectParamValue::F32(1.0), // gamma
                EffectParamValue::F32(0.0), // out_black
                EffectParamValue::F32(1.0), // out_white
            ],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("levels path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();
        let r = raw[(16 * w + 16) * 4];
        assert!(r < 20, "0.25 gray under in_black=0.3 must crush to black, got {r}");

        // gamma = 2 brightens mid-gray: 0.5^(1/2) ≈ 0.707.
        let chain = effect_chain(
            "Levels",
            vec![
                EffectParamValue::F32(0.0),
                EffectParamValue::F32(1.0),
                EffectParamValue::F32(2.0),
                EffectParamValue::F32(0.0),
                EffectParamValue::F32(1.0),
            ],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(
                &filled_scene(
                    vello::peniko::Color::from_rgba8(128, 128, 128, 255),
                    kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
                ),
                dims,
                None,
                &chain,
            )
            .expect("levels gamma path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();
        let r = raw[(16 * w + 16) * 4];
        assert!(
            (150..=210).contains(&r),
            "gamma 2 on 0.5 gray must land near 0.707·255 ≈ 180, got {r}"
        );
    }

    /// Sharpen: along a gray→black edge, the gray side must overshoot its
    /// interior value (the high-frequency residual is added back).
    #[test]
    fn sharpen_overshoots_at_hard_edges() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // Left half 50% gray, right half black: one hard vertical edge at x=32.
        let mut scene = vello::Scene::new();
        use kurbo::Shape;
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::from_rgba8(128, 128, 128, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3),
        );
        let chain =
            effect_chain("Sharpen", vec![EffectParamValue::F32(2.0), EffectParamValue::F32(2.0)]);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("sharpen path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();
        let interior = raw[(32 * w + 8) * 4];
        let edge = raw[(32 * w + 30) * 4];
        assert_eq!(interior, 128, "interior gray must survive sharpening");
        assert!(
            edge > interior + 30,
            "gray side of the edge must overshoot the interior gray, got edge={edge} interior={interior}"
        );
    }

    /// Grain must actually perturb a flat field, and be deterministic for a
    /// given (params, time) pair (§6 determinism contract).
    #[test]
    fn grain_perturbs_flat_field_deterministically() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let make_backend = || {
            GpuFilterBackend::new(device.clone(), queue.clone(), dims)
                .expect("GpuFilterBackend should initialise")
        };

        let scene = filled_scene(
            vello::peniko::Color::from_rgba8(128, 128, 128, 255),
            kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
        );
        let params = vec![
            EffectParamValue::F32(0.6), // amount
            EffectParamValue::F32(0.0), // seed
            EffectParamValue::Bool(true),
        ];

        let image = make_backend()
            .render_scene_to_image_gpu_filtered(
                &scene,
                dims,
                None,
                &effect_chain("Grain", params.clone()),
            )
            .expect("grain path should succeed");
        let raw = image.data.data.data();
        let touched = raw.chunks_exact(4).filter(|px| px[0] != 128).count();
        assert!(
            touched > 64,
            "grain should perturb most pixels of a flat field, only {touched} changed"
        );

        let image_again = make_backend()
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &effect_chain("Grain", params))
            .expect("second grain render should succeed");
        assert_eq!(raw, image_again.data.data.data(), "grain must be deterministic");
    }

    /// MotionBlur with angle 0 must smear horizontally: intermediate alpha
    /// appears up to `length` pixels right of the shape's edge.
    #[test]
    fn motion_blur_smears_horizontally() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // Opaque white rect over the left third.
        let scene =
            filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 21.0, 64.0));
        let chain = effect_chain(
            "MotionBlur",
            vec![EffectParamValue::F32(12.0), EffectParamValue::F32(0.0)],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("motion-blur path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();
        let y = 32usize;
        let mut smeared = false;
        for x in 22..33usize {
            let a = raw[(y * w + x) * 4 + 3];
            if a > 20 && a < 235 {
                smeared = true;
                break;
            }
        }
        assert!(smeared, "motion blur must smear the trailing edge to intermediate alpha");

        // Far outside the smear extent the frame must stay empty.
        let a = raw[(y * w + 45) * 4 + 3];
        assert!(a < 20, "smear must not reach beyond length pixels, got a={a}");
    }

    /// Duotone must map black onto the shadow colour and white onto the
    /// highlight colour at full strength.
    #[test]
    fn duotone_maps_luma_onto_the_ramp() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let mut scene = filled_scene(
            vello::peniko::Color::from_rgba8(0, 0, 0, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
        );
        let white = filled_scene(
            vello::peniko::Color::from_rgba8(255, 255, 255, 255),
            kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
        );
        scene.encoding_mut().append(white.encoding(), &None);

        let chain = effect_chain(
            "Duotone",
            vec![
                EffectParamValue::F32(1.0),                   // amount
                EffectParamValue::Vec4([0.0, 0.0, 1.0, 1.0]), // shadow: blue
                EffectParamValue::Vec4([1.0, 0.5, 0.0, 1.0]), // highlight: orange
            ],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("duotone path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();

        let dark = &raw[(32 * w + 8) * 4..(32 * w + 8) * 4 + 3];
        assert!(
            dark[2] > 200 && dark[0] < 60,
            "black input must map to the shadow colour, got {dark:?}"
        );
        let light = &raw[(32 * w + 56) * 4..(32 * w + 56) * 4 + 3];
        assert!(
            light[0] > 200 && light[2] < 80,
            "white input must map to the highlight colour, got {light:?}"
        );
    }

    /// Posterize with two levels must push 0.4 to black and 0.6 to white.
    #[test]
    fn posterize_quantises_midtones() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let mut scene = filled_scene(
            vello::peniko::Color::from_rgba8(102, 102, 102, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
        );
        let lighter = filled_scene(
            vello::peniko::Color::from_rgba8(153, 153, 153, 255),
            kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
        );
        scene.encoding_mut().append(lighter.encoding(), &None);

        let chain = effect_chain("Posterize", vec![EffectParamValue::F32(2.0)]);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("posterize path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();

        let low = raw[(32 * w + 8) * 4];
        let high = raw[(32 * w + 56) * 4];
        assert!(low < 40, "0.4 must quantise down to black, got {low}");
        assert!(high > 200, "0.6 must quantise up to white, got {high}");
    }

    /// Edge must light the step boundary and darken flat interiors.
    #[test]
    fn edge_lights_the_step_boundary() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let mut scene = filled_scene(
            vello::peniko::Color::from_rgba8(0, 0, 0, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
        );
        let white = filled_scene(
            vello::peniko::Color::from_rgba8(255, 255, 255, 255),
            kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
        );
        scene.encoding_mut().append(white.encoding(), &None);

        let chain =
            effect_chain("Edge", vec![EffectParamValue::F32(1.0), EffectParamValue::F32(0.0)]);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("edge path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();

        let boundary = raw[(32 * w + 32) * 4];
        let flat = raw[(32 * w + 50) * 4];
        assert!(boundary > 150, "the step edge must light up, got {boundary}");
        assert!(flat < 40, "a flat region must stay dark, got {flat}");
    }

    /// LensDistortion must displace samples: a band that survives at the
    /// control setting must move out of a near-edge pixel at high `amount`,
    /// while the centre (radius 0) is untouched either way.
    #[test]
    fn lens_distortion_displaces_samples_toward_the_centre() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // White band over the leftmost 16 columns.
        let scene =
            filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 16.0, 64.0));

        let control = effect_chain("LensDistortion", vec![EffectParamValue::F32(0.0)]);
        let control_image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &control)
            .expect("control path should succeed");
        let cw = control_image.natural_size[0] as usize;
        let control_raw = control_image.data.data.data();
        assert!(control_raw[(32 * cw + 8) * 4] > 200, "control must show the white band at x=8");

        let warped = effect_chain("LensDistortion", vec![EffectParamValue::F32(130.0)]);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &warped)
            .expect("lens-distortion path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();
        assert!(
            raw[(32 * w + 8) * 4] < 60,
            "the warp must pull the band out of the near-edge pixel, got {}",
            raw[(32 * w + 8) * 4]
        );
    }

    /// DropShadow must paint the silhouette offset behind the content, and
    /// leave the content itself untouched.
    #[test]
    fn drop_shadow_offsets_the_silhouette() {
        let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
            return;
        };
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let scene =
            filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
        let chain = effect_chain(
            "DropShadow",
            vec![
                EffectParamValue::Vec2([-8.0, 0.0]), // offset (shadow to the left)
                EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]), // opaque black shadow
            ],
        );
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("drop-shadow path should succeed");
        let w = image.natural_size[0] as usize;
        let raw = image.data.data.data();

        let shadow = &raw[(32 * w + 20) * 4..(32 * w + 20) * 4 + 4];
        assert!(
            shadow[0] < 40 && shadow[3] > 200,
            "the offset silhouette must be dark and opaque, got {shadow:?}"
        );
        let content = raw[(32 * w + 40) * 4];
        assert!(content > 200, "the content must stay white, got {content}");
        let clear = raw[(32 * w + 4) * 4 + 3];
        assert!(clear < 20, "outside the shadow extent the frame stays empty, got {clear}");
    }
}
