use super::error::RenderError;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

pub struct RendererCore {
    pub renderer: Renderer,
}

impl RendererCore {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue) -> Result<Self, RenderError> {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                pipeline_cache: None,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        )
        .map_err(|e| RenderError::VelloInit(format!("{e:?}")))?;

        Ok(Self { renderer })
    }

    pub fn render_vello_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        scene: &Scene,
    ) -> Result<(), RenderError> {
        let render_params = RenderParams {
            base_color: Color::BLACK,
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };

        self.renderer
            .render_to_texture(device, queue, scene, texture_view, &render_params)
            .map_err(|e| RenderError::FrameRender(format!("{e:?}")))
    }
}
