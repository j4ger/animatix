use animatix::timeline::effects::Effect;

// ── Uniform structs ─────────────────────────────────────────────────────────

/// Host-owned per-pass context (bind group 0, binding 3).
///
/// Layout must match the `EffectContext` WGSL struct in `timeline/filter.rs`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct EffectContextUniform {
    pub(super) tex_size: [u32; 2],
    pub(super) _pad0: [u32; 2],
    pub(super) inv_size: [f32; 2],
    pub(super) _pad1: [f32; 2],
    pub(super) pass_index: u32,
    pub(super) pass_count: u32,
    pub(super) time_ms: f32,
    pub(super) _pad2: f32,
}

pub(super) const EFFECT_CONTEXT_SIZE: u64 = std::mem::size_of::<EffectContextUniform>() as u64;

/// Dynamic-offset stride between per-pass context uniforms. 256 covers every
/// backend's `min_uniform_buffer_offset_alignment` and keeps offsets aligned.
pub(super) const CONTEXT_STRIDE: u32 = 256;

/// Maximum effect passes dispatchable in one chain (context slots available).
pub(super) const MAX_CHAIN_PASSES: u32 = 64;

// ── Backend struct ──────────────────────────────────────────────────────────

/// GPU-backed effect backend that owns its own temporary targets and a
/// dedicated [`crate::core::RendererCore`] so it never contends with the main renderer.
pub(super) struct EffectPipeline {
    /// One compute pipeline per pass, index-aligned with the descriptor.
    pub(super) passes: Vec<wgpu::ComputePipeline>,
    /// Author parameter uniform buffer (binding 2).
    pub(super) uniform_buffer: wgpu::Buffer,
}

/// Fixed layout shared by every effect (docs/effects.md §4.2).
pub(super) fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            // 5: the pre-chain original. ABI v2 (docs/effects.md §4.2). A
            // pass that does not declare it is unaffected — a layout may
            // bind more than a shader reads.
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

pub(super) fn create_pipeline_layout(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Animatix Effect Pipeline Layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    })
}

/// Build (once) the pipelines and uniform buffer for `effect`.
pub(super) fn create_effect_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    effect: &dyn Effect,
) -> EffectPipeline {
    let mut passes = Vec::with_capacity(effect.passes().len());
    for pass in effect.passes() {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(pass.label.as_ref()),
            source: wgpu::ShaderSource::Wgsl(pass.wgsl.clone()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(pass.label.as_ref()),
            layout: Some(pipeline_layout),
            module: &module,
            entry_point: Some(pass.entry.as_ref()),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        passes.push(pipeline);
    }
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Animatix Effect Uniforms"),
        size: u64::from(effect.author_uniform_size().max(16)),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    EffectPipeline {
        passes,
        uniform_buffer,
    }
}

/// Bind group for one pass: views plus the context uniform at `slot`'s
/// dynamic offset in the shared context buffer.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_effect_bind_group(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    context_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    src_view: &wgpu::TextureView,
    dst_view: &wgpu::TextureView,
    original_view: &wgpu::TextureView,
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Animatix Effect Bind Group"),
        layout: bind_group_layout,
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
                    buffer: context_buffer,
                    offset: 0,
                    size: Some(std::num::NonZeroU64::new(EFFECT_CONTEXT_SIZE).expect("nonzero")),
                }),
                // The dynamic offset itself is supplied in set_bind_group.
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(original_view),
            },
        ],
    })
}

/// Encode one effect pass into `encoder` (dispatch at `width`×`height`,
/// context uniform at the bind group's dynamic offset).
pub(super) fn encode_effect_pass(
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
