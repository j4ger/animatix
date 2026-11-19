use super::{CONTEXT_SIZE, FORMAT, ScratchSet, Target};
use animatix_core::effect::{EffectParamKind, EffectParamUnit};

pub(super) fn create_effect_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Animatix Portable Effect Bind Group Layout"),
        entries: &[
            texture_entry(0, wgpu::ShaderStages::FRAGMENT),
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(CONTEXT_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture_entry(5, wgpu::ShaderStages::FRAGMENT),
        ],
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn create_compute_pipeline_layout(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, wgpu::PipelineLayout) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Animatix Native Compute Effect Bind Group Layout"),
        entries: &[
            texture_entry(0, wgpu::ShaderStages::COMPUTE),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: FORMAT,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            uniform_entry(2, None),
            uniform_entry(3, wgpu::BufferSize::new(CONTEXT_SIZE)),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            texture_entry(5, wgpu::ShaderStages::COMPUTE),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Animatix Native Compute Effect Pipeline Layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    (layout, pipeline_layout)
}

#[cfg(not(target_arch = "wasm32"))]
fn uniform_entry(
    binding: u32,
    min_binding_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

pub(super) fn create_alpha_pipelines(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Animatix Alpha Conversion Layout"),
        entries: &[texture_entry(0, wgpu::ShaderStages::FRAGMENT)],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Animatix Alpha Conversion Pipeline Layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Animatix Alpha Conversion Shader"),
        source: wgpu::ShaderSource::Wgsl(ALPHA_CONVERSION_WGSL.into()),
    });
    let make_pipeline = |entry_point| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("fullscreen_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    (layout, make_pipeline("to_straight"), make_pipeline("to_premultiplied"))
}

pub(super) const ALPHA_CONVERSION_WGSL: &str = concat!(
    r#"
struct FullscreenVertex {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn fullscreen_vs(@builtin(vertex_index) vertex: u32) -> FullscreenVertex {
    let xy = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return FullscreenVertex(
        vec4<f32>(xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0),
    );
}

@group(0) @binding(0) var src: texture_2d<f32>;

@fragment
fn to_straight(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(src, vec2<i32>(position.xy), 0);
    if (color.a > 0.0) {
        return vec4<f32>(color.rgb / color.a, color.a);
    }
    return vec4<f32>(0.0);
}

@fragment
fn to_premultiplied(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(src, vec2<i32>(position.xy), 0);
    return vec4<f32>(color.rgb * color.a, color.a);
}
"#
);

pub(super) fn encode_fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Animatix Portable Effect Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    pass.set_scissor_rect(0, 0, width, height);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

pub(super) fn encode_effect_pass(
    encoder: &mut wgpu::CommandEncoder,
    bind_group: &wgpu::BindGroup,
    pipeline: &wgpu::RenderPipeline,
    target: &wgpu::TextureView,
    width: u32,
    height: u32,
    author_offset: u32,
    context_offset: u32,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Animatix Built-in Fragment Effect"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    pass.set_scissor_rect(0, 0, width, height);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[author_offset, context_offset]);
    pass.draw(0..3, 0..1);
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn encode_compute_effect_pass(
    encoder: &mut wgpu::CommandEncoder,
    bind_group: &wgpu::BindGroup,
    pipeline: &wgpu::ComputePipeline,
    width: u32,
    height: u32,
    author_offset: u32,
    context_offset: u32,
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("Animatix Native Compute Plugin Effect"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[author_offset, context_offset]);
    pass.dispatch_workgroups(width.div_ceil(16), height.div_ceil(16), 1);
}

pub(super) fn create_scratch_set(device: &wgpu::Device, width: u32, height: u32) -> ScratchSet {
    ScratchSet {
        input: target(device, width, height, "Animatix Portable Effect Input"),
        ping: target(device, width, height, "Animatix Portable Effect Ping"),
        original: target(device, width, height, "Animatix Portable Effect Original"),
        output: target(device, width, height, "Animatix Portable Effect Output"),
    }
}

fn target(device: &wgpu::Device, width: u32, height: u32, label: &'static str) -> Target {
    let usage = wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    #[cfg(not(target_arch = "wasm32"))]
    let usage = usage | wgpu::TextureUsages::STORAGE_BINDING;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Target { texture, view }
}

pub(super) fn scale_pixel_parameters(
    params: &[animatix_core::effect::EffectParamSpec],
    scale: f32,
    uniforms: &mut [u8],
) {
    if scale == 1.0 {
        return;
    }
    for spec in params.iter().filter(|spec| spec.unit == EffectParamUnit::Pixel) {
        match spec.kind {
            EffectParamKind::F32 => scale_f32(uniforms, spec.offset, scale),
            EffectParamKind::Vec2 => {
                scale_f32(uniforms, spec.offset, scale);
                scale_f32(uniforms, spec.offset + 4, scale);
            },
            _ => tracing::warn!(
                parameter = %spec.name,
                kind = ?spec.kind,
                "pixel effect parameter must be F32 or Vec2; retaining its authored value"
            ),
        }
    }
}

fn scale_f32(uniforms: &mut [u8], offset: u32, scale: f32) {
    let start = offset as usize;
    if let Some(bytes) = uniforms.get_mut(start..start + 4) {
        let mut value = [0u8; 4];
        value.copy_from_slice(bytes);
        bytes.copy_from_slice(&(f32::from_le_bytes(value) * scale).to_le_bytes());
    }
}
