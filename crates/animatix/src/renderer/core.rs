use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

pub struct RendererCore {
    pub renderer: Renderer,
}

impl RendererCore {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue) -> Self {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                pipeline_cache: None,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        )
        .expect("Failed to create Vello renderer");

        Self { renderer }
    }

    pub fn render_vello_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        scene: &Scene,
    ) {
        let render_params = RenderParams {
            base_color: Color::BLACK,
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };

        self.renderer
            .render_to_texture(device, queue, scene, texture_view, &render_params)
            .expect("Failed to render vello scene");
    }
}
