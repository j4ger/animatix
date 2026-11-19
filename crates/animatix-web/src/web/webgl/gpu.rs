use std::collections::HashMap;

use animatix::composition::TransitionBlend;
use animatix_render::transition::TransitionCompositor;
use kurbo::Affine;
use web_sys::HtmlCanvasElement;
use wgpu::rwh::{DisplayHandle, HandleError, HasDisplayHandle};

use super::images::Images;

use super::{
    effects::{Inline, Layer, blur_radius},
    encode,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug)]
struct WebDisplay;

impl HasDisplayHandle for WebDisplay {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::web())
    }
}

pub(in crate::web) struct Gpu {
    pub(in crate::web) device: wgpu::Device,
    pub(in crate::web) queue: wgpu::Queue,
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    textures: [Option<wgpu::TextureView>; 6],
    inline_textures: Vec<wgpu::TextureView>,
    images: Images,
    compositor: Compositor,
    surface_compositor: Option<Compositor>,
    transitions: Option<TransitionCompositor>,
}

impl Gpu {
    pub(super) async fn new(
        canvas: HtmlCanvasElement,
    ) -> Result<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration, Self), String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(WebDisplay))
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("WebGL2 canvas: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("WebGL2 adapter: {e}"))?;
        if adapter.get_info().backend != wgpu::Backend::Gl {
            return Err("expected wgpu GL adapter".into());
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits {
                    max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
                    max_buffer_size: adapter.limits().max_buffer_size,
                    ..wgpu::Limits::downlevel_webgl2_defaults()
                },
                ..Default::default()
            })
            .await
            .map_err(|e| format!("WebGL2 device: {e}"))?;
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or("no WebGL2 surface configuration")?;
        let capabilities = surface.get_capabilities(&adapter);
        if capabilities.formats.contains(&FORMAT) {
            config.format = FORMAT;
        }
        if capabilities.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        }
        surface.configure(&device, &config);
        let (renderer, resources) = vello_gpu::Renderer::new(
            &device,
            &queue,
            &vello_gpu::RenderTargetConfig {
                format: FORMAT,
                width: u16::try_from(width).map_err(|e| e.to_string())?,
                height: u16::try_from(height).map_err(|e| e.to_string())?,
            },
        );
        let compositor = Compositor::new(&device, FORMAT);
        let surface_compositor =
            (config.format != FORMAT).then(|| Compositor::new(&device, config.format));
        Ok((
            surface,
            config,
            Self {
                device,
                queue,
                renderer,
                resources,
                textures: Default::default(),
                inline_textures: Vec::new(),
                images: Images::default(),
                compositor,
                surface_compositor,
                transitions: None,
            },
        ))
    }

    pub(super) fn clear_images(&mut self) {
        self.images.clear();
    }

    pub(super) fn texture(
        &mut self,
        slot: usize,
        size: (u16, u16),
    ) -> Result<wgpu::TextureView, String> {
        let (width, height) = (u32::from(size.0), u32::from(size.1));
        if width.max(height) > self.device.limits().max_texture_dimension_2d {
            return Err("WebGL2 raster exceeds device texture limits".into());
        }
        let texture = &mut self.textures[slot];
        if let Some(view) = texture
            && view.texture().width() == width
            && view.texture().height() == height
        {
            return Ok(view.clone());
        }
        self.transitions
            .as_ref()
            .inspect(|transition| transition.clear_cached_textures());
        let view = new_target(&self.device, size);
        *texture = Some(view.clone());
        Ok(view)
    }

    pub(super) fn draw_inline(
        &mut self,
        layers: Vec<Inline>,
        scale: f32,
    ) -> Result<HashMap<vello_gpu::TextureId, wgpu::TextureView>, String> {
        let mut bindings = HashMap::new();
        self.inline_textures.truncate(layers.len());
        for (index, layer) in layers.into_iter().enumerate() {
            let width = ((layer.region.size.width as f32 * scale).round() as u32).max(1);
            let height = ((layer.region.size.height as f32 * scale).round() as u32).max(1);
            let size = (
                u16::try_from(width).map_err(|error| error.to_string())?,
                u16::try_from(height).map_err(|error| error.to_string())?,
            );
            let first = self.texture(3, size)?;
            let second = self.texture(4, size)?;
            let view = match self.inline_textures.get_mut(index) {
                Some(texture) => {
                    if texture.texture().width() != width || texture.texture().height() != height {
                        *texture = new_target(&self.device, size);
                    }
                    texture.clone()
                },
                None => {
                    let texture = new_target(&self.device, size);
                    self.inline_textures.push(texture.clone());
                    texture
                },
            };
            let [x, y] = layer.region.origin.map(f64::from);
            let raster = Affine::scale(f64::from(scale)) * Affine::translate((-x, -y));
            let radius = layer.radius * scale;
            let seed = if radius >= 0.5 { &first } else { &view };
            self.render(&layer.scene, seed, size, raster, &bindings)?;
            if radius >= 0.5 {
                self.blur(&first, &second, &view, radius);
            }
            bindings.insert(layer.id, view);
        }
        Ok(bindings)
    }

    pub(super) fn rasterize(
        &mut self,
        scene: &vello::Scene,
        dst: &wgpu::TextureView,
        size: (u16, u16),
        scale: f32,
    ) -> Result<(), String> {
        self.render(scene, dst, size, Affine::scale(f64::from(scale)), &HashMap::new())
    }

    pub(super) fn render(
        &mut self,
        scene: &vello::Scene,
        dst: &wgpu::TextureView,
        size: (u16, u16),
        raster: Affine,
        inline: &HashMap<vello_gpu::TextureId, wgpu::TextureView>,
    ) -> Result<(), String> {
        let mut bindings = vello_gpu::TextureBindings::new();
        for (id, view) in inline {
            bindings.insert(*id, view.clone());
        }
        let scene =
            encode::scene(scene, size, raster, &mut self.resources, inline, &mut |image| {
                self.images.bind(&self.device, &self.queue, image, &mut bindings)
            })?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer
            .render(
                &scene,
                &mut self.resources,
                &mut encoder,
                vello_gpu::RenderTarget {
                    view: dst,
                    depth_view: None,
                },
                &bindings,
                vello_gpu::RenderOptions::default(),
            )
            .map_err(|e| e.to_string())?;
        // The renderer reuses GPU resources after submission.
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    pub(super) fn transition(
        &mut self,
        from: &wgpu::TextureView,
        to: &wgpu::TextureView,
        output: &wgpu::TextureView,
        size: (u16, u16),
        blend: &TransitionBlend,
    ) -> Result<(), String> {
        if self.transitions.is_none() {
            self.transitions = Some(TransitionCompositor::new(&self.device)?);
        }
        self.transitions.as_ref().ok_or("no transition compositor")?.render(
            &self.device,
            &self.queue,
            from,
            to,
            output,
            size.0.into(),
            size.1.into(),
            blend.progress as f32,
            &blend.id,
            blend.easing,
        )
    }

    pub(super) fn draw_effects(
        &mut self,
        base: &wgpu::TextureView,
        size: (u16, u16),
        scale: f32,
        layers: Vec<Layer>,
        inline: &HashMap<vello_gpu::TextureId, wgpu::TextureView>,
    ) -> Result<(), String> {
        if layers.is_empty() {
            return Ok(());
        }
        let first = self.texture(3, size)?;
        let second = self.texture(4, size)?;
        let third = self.texture(5, size)?;
        for layer in layers {
            let (input, params) = match layer {
                Layer::Backdrop(backdrop) => {
                    let radius = blur_radius(&backdrop.chain)? * scale;
                    if radius < 0.5 {
                        continue;
                    }
                    self.blur(base, &first, &second, radius);
                    let [x, y] = backdrop.clip.origin;
                    let (w, h) =
                        (backdrop.clip.size.width as f32, backdrop.clip.size.height as f32);
                    (
                        &second,
                        Parameters {
                            clip: [x * scale, y * scale, (x + w) * scale, (y + h) * scale],
                            settings: [3.0, 0.0, backdrop.corner_radius * scale, backdrop.alpha],
                        },
                    )
                },
                Layer::Scene {
                    scene,
                    radius,
                    alpha,
                } => {
                    self.render(&scene, &first, size, Affine::scale(f64::from(scale)), inline)?;
                    let input = if radius * scale >= 0.5 {
                        self.blur(&first, &third, &second, radius * scale);
                        &second
                    } else {
                        &first
                    };
                    (input, Parameters::overlay(alpha))
                },
            };
            self.compositor.draw(&self.device, &self.queue, base, input, &third, params);
            self.compositor.draw(
                &self.device,
                &self.queue,
                &third,
                &third,
                base,
                Parameters::default(),
            );
        }
        Ok(())
    }

    fn blur(
        &self,
        src: &wgpu::TextureView,
        scratch: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        radius: f32,
    ) {
        self.compositor.draw(
            &self.device,
            &self.queue,
            src,
            src,
            scratch,
            Parameters::blur(1.0, radius),
        );
        self.compositor.draw(
            &self.device,
            &self.queue,
            scratch,
            scratch,
            dst,
            Parameters::blur(2.0, radius),
        );
    }

    pub(super) fn draw_overlay(
        &self,
        base: &wgpu::TextureView,
        overlay: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        alpha: f32,
    ) {
        self.compositor.draw(
            &self.device,
            &self.queue,
            base,
            overlay,
            dst,
            Parameters::overlay(alpha),
        );
    }

    pub(super) fn present(
        &self,
        surface: &wgpu::Surface<'_>,
        config: &wgpu::SurfaceConfiguration,
        view: &wgpu::TextureView,
    ) -> Result<bool, String> {
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface.configure(&self.device, config);
                return Ok(false);
            },
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            },
            error => return Err(format!("WebGL2 canvas unavailable: {error:?}")),
        };
        let dst = frame.texture.create_view(&Default::default());
        self.surface_compositor.as_ref().unwrap_or(&self.compositor).draw(
            &self.device,
            &self.queue,
            view,
            view,
            &dst,
            Parameters::default(),
        );
        self.queue.present(frame);
        Ok(true)
    }
}

fn new_target(device: &wgpu::Device, size: (u16, u16)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix WebGL2 offscreen"),
            size: wgpu::Extent3d {
                width: size.0.into(),
                height: size.1.into(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

#[derive(Default)]
struct Parameters {
    clip: [f32; 4],
    settings: [f32; 4],
}

impl Parameters {
    fn blur(mode: f32, radius: f32) -> Self {
        Self {
            settings: [mode, radius, 0.0, 0.0],
            ..Self::default()
        }
    }

    fn overlay(alpha: f32) -> Self {
        Self {
            settings: [4.0, 0.0, 0.0, alpha],
            ..Self::default()
        }
    }
}

struct Compositor {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    sampler: wgpu::Sampler,
}

impl Compositor {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix WebGL2 composition"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Animatix WebGL2 composition"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix WebGL2 composite parameters"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            uniform,
            sampler,
        }
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        first: &wgpu::TextureView,
        second: &wgpu::TextureView,
        dst: &wgpu::TextureView,
        params: Parameters,
    ) {
        let mut bytes = [0; 32];
        for (value, chunk) in
            params.clip.into_iter().chain(params.settings).zip(bytes.chunks_exact_mut(4))
        {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&self.uniform, 0, &bytes);
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Animatix WebGL2 compositor inputs"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(first),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(second),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}
