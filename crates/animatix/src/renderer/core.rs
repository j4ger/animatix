use super::msdf::FontAtlas;
use super::types::{CameraUniform, INDICES, SdfInstance, TextInstance, VERTICES, Vertex};
use wgpu::util::DeviceExt;

pub struct RendererCore {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub render_pipeline: wgpu::RenderPipeline,
    pub text_render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub instance_buffer: Option<wgpu::Buffer>,
    pub text_instance_buffer: Option<wgpu::Buffer>,
    pub camera_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub text_bind_group: wgpu::BindGroup,
    pub num_indices: u32,
    pub num_instances: u32,
    pub num_text_instances: u32,
    pub camera_uniform: CameraUniform,
    pub font_atlas: FontAtlas,
}

impl RendererCore {
    pub async fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        instances: &[SdfInstance],
        text_instances: &[TextInstance],
        font_atlas: FontAtlas,
    ) -> Self {
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_proj(width as f32, height as f32);

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let instance_buffer = if !instances.is_empty() {
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instance Buffer"),
                    contents: bytemuck::cast_slice(instances),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                }),
            )
        } else {
            None
        };

        let dummy_buffer = if instance_buffer.is_none() {
            Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dummy Instance Buffer"),
                size: std::mem::size_of::<SdfInstance>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }))
        } else {
            None
        };
        let bind_group_instance_buffer = instance_buffer
            .as_ref()
            .unwrap_or_else(|| dummy_buffer.as_ref().unwrap());

        let text_instance_buffer = if !text_instances.is_empty() {
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Text Instance Buffer"),
                    contents: bytemuck::cast_slice(text_instances),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                }),
            )
        } else {
            None
        };

        let dummy_text_buffer = if text_instance_buffer.is_none() {
            Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dummy Text Instance Buffer"),
                size: std::mem::size_of::<TextInstance>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }))
        } else {
            None
        };
        let bind_group_text_instance_buffer = text_instance_buffer
            .as_ref()
            .unwrap_or_else(|| dummy_text_buffer.as_ref().unwrap());

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
            label: Some("bind_group_layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bind_group_instance_buffer.as_entire_binding(),
                },
            ],
            label: Some("bind_group"),
        });

        let text_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bind_group_text_instance_buffer.as_entire_binding(),
                },
            ],
            label: Some("text_bind_group"),
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Engine Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/engine_shader.wgsl").into(),
            ),
        });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../assets/shaders/text_shader.wgsl").into(),
            ),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let text_render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Text Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout, &font_atlas.bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
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
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let text_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Render Pipeline"),
            layout: Some(&text_render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
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
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            device,
            queue,
            render_pipeline,
            text_render_pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            text_instance_buffer,
            camera_buffer,
            bind_group,
            text_bind_group,
            num_indices: INDICES.len() as u32,
            num_instances: instances.len() as u32,
            num_text_instances: text_instances.len() as u32,
            camera_uniform,
            font_atlas,
        }
    }
}
