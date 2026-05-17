use std::borrow::Cow;

/// GPU-based compositor for scene transition effects.
///
/// Renders a fullscreen quad with a WGSL shader that blends two input textures
/// based on transition progress and type. Supports fade and directional wipe
/// transitions.
pub struct TransitionCompositor {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionUniforms {
    progress: f32,
    transition_type: u32,
    _padding: [f32; 2],
}

impl TransitionUniforms {
    fn new(progress: f32, transition_id: &str) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            transition_type: crate::transition_registry::shader_case(transition_id),
            _padding: [0.0; 2],
        }
    }
}

const TRANSITION_SHADER_WGSL: &str = r#"
struct Uniforms {
    progress: f32,
    transition_type: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var from_texture: texture_2d<f32>;
@group(0) @binding(1) var to_texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let from_color = textureSample(from_texture, texture_sampler, in.uv);
    let to_color = textureSample(to_texture, texture_sampler, in.uv);

    var alpha: f32;
    switch uniforms.transition_type {
        case 0u: { // Cut
            alpha = select(0.0, 1.0, uniforms.progress >= 0.5);
        }
        case 1u: { // Fade
            alpha = uniforms.progress;
        }
        case 2u: { // WipeLeft
            alpha = 1.0 - step(uniforms.progress, in.uv.x);
        }
        case 3u: { // WipeRight
            alpha = step(1.0 - uniforms.progress, in.uv.x);
        }
        case 4u: { // WipeUp
            alpha = 1.0 - step(uniforms.progress, in.uv.y);
        }
        case 5u: { // WipeDown
            alpha = step(1.0 - uniforms.progress, in.uv.y);
        }
        default: {
            alpha = uniforms.progress;
        }
    }

    return mix(from_color, to_color, alpha);
}
"#;

impl TransitionCompositor {
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Transition Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TRANSITION_SHADER_WGSL)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animatix Transition Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Transition Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Animatix Transition Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Transition Uniforms"),
            size: std::mem::size_of::<TransitionUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Transition Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            sampler,
        })
    }

    /// Composite two scene textures into the output texture using the given
    /// transition progress, type, and easing.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from_view: &wgpu::TextureView,
        to_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        _width: u32,
        _height: u32,
        progress: f32,
        transition_id: &str,
        easing: crate::easing::Easing,
    ) -> Result<(), String> {
        // Apply easing to progress
        let eased_progress = crate::easing::apply_easing(progress, easing);
        // Update uniform buffer
        let uniforms = TransitionUniforms::new(eased_progress, transition_id);
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        // Create bind group for this frame
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix Transition Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(from_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(to_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Transition Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Animatix Transition Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}
