//! Shared Vello GPU rendering of evaluated engine scenes.

mod composite;

use animatix::scene::{CompositePlacement, Draw, Scene};
use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{EffectChain, EffectRegion, FilterBackend, PendingBackdrop};
use animatix::timeline::image::SceneImage;
use kurbo::{Affine, Rect};
use vello_gpu::{ClearSettings, TargetInit};

use crate::effects_gpu::{EffectsExtent, EffectsGpu};
use crate::raster_gpu::{ExternalTextures, RasterGpu};
use composite::{Compositor, Parameters};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const TARGET_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

/// Requests effect commands on the frame scene without doing GPU work during evaluation.
pub struct SceneRecorder;

impl FilterBackend for SceneRecorder {
    fn records_scene_effects(&self) -> bool {
        true
    }

    fn render_scene_to_image_gpu_filtered(
        &mut self,
        _: &Scene,
        _: SceneDimensions,
        _: Option<EffectRegion>,
        _: &EffectChain,
    ) -> Result<SceneImage, String> {
        Err("effect commands must be recorded before rasterization".into())
    }
}

/// Rasterizer, effects and texture scratch owned by a single device and queue.
pub struct FrameGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    raster: RasterGpu,
    effects: EffectsGpu,
    compositor: Compositor,
    output: Option<wgpu::TextureView>,
    scratch: Vec<wgpu::TextureView>,
    used: usize,
}

impl FrameGpu {
    /// Set up a shared renderer for the given device and queue.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            raster: RasterGpu::new(device, queue),
            effects: EffectsGpu::new(device.clone(), queue.clone()),
            compositor: Compositor::new(device),
            output: None,
            scratch: Vec::new(),
            used: 0,
        }
    }

    /// Rasterize into a reusable premultiplied RGBA8 texture.
    ///
    /// Consume the texture before calling this method again on the same renderer.
    pub fn render(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        raster: Affine,
    ) -> Result<wgpu::TextureView, String> {
        self.validate_dimensions(width, height)?;
        let slot = self.output.get_or_insert_with(|| target(&self.device, width, height));
        if dimensions(slot) != (width, height) {
            *slot = target(&self.device, width, height);
        }
        let view = slot.clone();
        self.render_frame_into(scene, &view, raster)?;
        Ok(view)
    }

    /// Rasterize into an independent RGBA8 render target on this device.
    ///
    /// The target must support render attachment, sampling and copies in both directions.
    pub fn render_into(
        &mut self,
        scene: &Scene,
        destination: &wgpu::TextureView,
        raster: Affine,
    ) -> Result<(), String> {
        self.render_with_clear(scene, destination, raster, peniko::Color::TRANSPARENT)
    }

    /// Rasterize a complete frame, including the existing opaque black backing.
    pub fn render_frame_into(
        &mut self,
        scene: &Scene,
        destination: &wgpu::TextureView,
        raster: Affine,
    ) -> Result<(), String> {
        self.render_with_clear(scene, destination, raster, peniko::Color::BLACK)
    }

    /// Rasterize a complete scene and present it in the same submission when
    /// no effect needs to consume an intermediate frame.
    pub fn render_and_present(
        &mut self,
        scene: &Scene,
        intermediate: &wgpu::TextureView,
        surface: &wgpu::TextureView,
        raster: Affine,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        if contains_effects(scene.draws()) {
            self.render_frame_into(scene, intermediate, raster)?;
            self.blit(intermediate, surface, format, false);
            return Ok(());
        }
        self.validate_target(intermediate)?;
        let mut encoder = self.encoder();
        let clear = TargetInit::Clear(ClearSettings::Viewport {
            color: peniko::Color::BLACK,
        });
        self.raster
            .encode_scene(scene, intermediate, raster, None, clear, &mut encoder)
            .map_err(|error| error.to_string())?;
        self.compositor.encode(
            &self.device,
            &mut encoder,
            intermediate,
            intermediate,
            surface,
            format,
            Parameters::default(),
        );
        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    fn render_with_clear(
        &mut self,
        scene: &Scene,
        destination: &wgpu::TextureView,
        raster: Affine,
        color: peniko::Color,
    ) -> Result<(), String> {
        self.validate_target(destination)?;
        self.used = 0;
        self.paint(scene, destination, raster, color)
    }

    /// Copy a premultiplied frame into a surface or an export target.
    ///
    /// Set `straight_alpha` when the destination requires unassociated color.
    pub fn blit(
        &mut self,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        straight_alpha: bool,
    ) {
        let mut params = Parameters::default();
        params.settings[0] = if straight_alpha { 1.0 } else { 0.0 };
        let mut encoder = self.encoder();
        self.compositor.encode(
            &self.device,
            &mut encoder,
            source,
            source,
            destination,
            format,
            params,
        );
        self.queue.submit([encoder.finish()]);
    }

    /// Composite a premultiplied layer onto an independent target of the same dimensions.
    pub fn over(
        &mut self,
        destination: &wgpu::TextureView,
        source: &wgpu::TextureView,
        opacity: f32,
    ) -> Result<(), String> {
        if dimensions(destination) != dimensions(source)
            || source.texture() == destination.texture()
        {
            return Err(
                "Vello GPU overlay requires two independent targets of the same size".into()
            );
        }
        let area = PixelRect::full(destination);
        self.composite(destination, source, area, area, [3.0, opacity, 0.0, 0.0]);
        Ok(())
    }

    fn paint(
        &mut self,
        scene: &Scene,
        destination: &wgpu::TextureView,
        raster: Affine,
        color: peniko::Color,
    ) -> Result<(), String> {
        let draws = scene.draws();
        let clear = TargetInit::Clear(ClearSettings::Viewport { color });
        if !contains_effects(draws) {
            return self
                .raster
                .render_draws(draws, destination, raster, None, clear)
                .map_err(|error| error.to_string());
        }

        let mut base = Vec::with_capacity(draws.len());
        let mut textures = ExternalTextures::new();
        let mut deferred = Vec::new();
        let base_checkpoint = self.used;
        for draw in draws {
            match draw {
                Draw::Filter {
                    scene,
                    region,
                    chain,
                    opacity,
                    placement: CompositePlacement::Inline,
                } => {
                    let Some((view, area)) =
                        self.filtered_scene(scene, *region, chain, destination, raster)?
                    else {
                        continue;
                    };
                    let id = u64::try_from(base.len()).map_err(|error| error.to_string())?;
                    textures.insert(id, view);
                    base.push(Draw::Texture {
                        id,
                        rect: area.rect(),
                        opacity: *opacity,
                        transform: Affine::IDENTITY,
                    });
                },
                Draw::Filter { .. } | Draw::Glass { .. } => deferred.push(draw),
                draw => base.push(transform_draw(draw.clone(), raster)),
            }
        }
        self.raster
            .render_draws(&base, destination, Affine::IDENTITY, Some(&textures), clear)
            .map_err(|error| error.to_string())?;
        self.used = base_checkpoint;
        let deferred_checkpoint = self.used;
        for draw in deferred {
            match draw {
                Draw::Filter {
                    scene,
                    region,
                    chain,
                    opacity,
                    ..
                } => {
                    if let Some((view, area)) =
                        self.filtered_scene(scene, *region, chain, destination, raster)?
                    {
                        self.composite(destination, &view, area, area, [3.0, *opacity, 0.0, 0.0]);
                    }
                },
                Draw::Glass { backdrop, children } => {
                    self.backdrop(destination, backdrop, raster)?;
                    let area = PixelRect::full(destination);
                    let layer = self.scratch(area.width, area.height);
                    self.paint(children, &layer, raster, peniko::Color::TRANSPARENT)?;
                    self.composite(destination, &layer, area, area, [3.0, 1.0, 0.0, 0.0]);
                },
                _ => unreachable!("only effect draws are deferred"),
            }
            self.used = deferred_checkpoint;
        }
        Ok(())
    }

    fn filtered_scene(
        &mut self,
        scene: &Scene,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        destination: &wgpu::TextureView,
        raster: Affine,
    ) -> Result<Option<(wgpu::TextureView, PixelRect)>, String> {
        let area = match region {
            Some(region) => PixelRect::rasterized(region, raster, destination),
            None => Some(PixelRect::full(destination)),
        };
        let Some(area) = area else {
            return Ok(None);
        };
        let result = self.scratch(area.width, area.height);
        let checkpoint = self.used;
        let shifted = Affine::translate((-f64::from(area.x), -f64::from(area.y))) * raster;
        if chain.is_empty() {
            self.paint(scene, &result, shifted, peniko::Color::TRANSPARENT)?;
            self.used = checkpoint;
            return Ok(Some((result, area)));
        }
        let seed = self.scratch(area.width, area.height);
        self.paint(scene, &seed, shifted, peniko::Color::TRANSPARENT)?;
        let filtered = self.run_effects(&seed, area, chain, raster)?;
        // A later inline Filter can reuse the effect executor's scratch while
        // this result is still referenced by the main raster pass.
        self.copy(&filtered, PixelRect::full(&result), &result);
        self.used = checkpoint;
        Ok(Some((result, area)))
    }

    fn backdrop(
        &mut self,
        destination: &wgpu::TextureView,
        backdrop: &PendingBackdrop,
        raster: Affine,
    ) -> Result<(), String> {
        let Some(region) = PixelRect::rasterized(backdrop.region, raster, destination) else {
            return Ok(());
        };
        let Some(clip) = PixelRect::rasterized(backdrop.clip, raster, destination) else {
            return Ok(());
        };
        let seed = self.scratch(region.width, region.height);
        self.copy(destination, region, &seed);
        let filtered = self.run_effects(&seed, region, &backdrop.chain, raster)?;
        self.composite(
            destination,
            &filtered,
            region,
            clip,
            [
                2.0,
                backdrop.alpha,
                backdrop.corner_radius * raster_scale(raster),
                0.0,
            ],
        );
        Ok(())
    }

    fn run_effects(
        &mut self,
        seed: &wgpu::TextureView,
        area: PixelRect,
        chain: &EffectChain,
        raster: Affine,
    ) -> Result<wgpu::TextureView, String> {
        if chain.is_empty() {
            return Ok(seed.clone());
        }
        Ok(self
            .effects
            .apply(
                seed,
                chain,
                EffectsExtent {
                    width: area.width,
                    height: area.height,
                    origin: [area.x as f32, area.y as f32],
                },
                raster_scale(raster),
            )?
            .view)
    }

    fn composite(
        &mut self,
        destination: &wgpu::TextureView,
        source: &wgpu::TextureView,
        source_area: PixelRect,
        clip: PixelRect,
        settings: [f32; 4],
    ) {
        let checkpoint = self.used;
        let (width, height) = dimensions(destination);
        let temporary = self.scratch(width, height);
        let params = Parameters {
            clip: [
                clip.x as f32,
                clip.y as f32,
                (clip.x + clip.width) as f32,
                (clip.y + clip.height) as f32,
            ],
            source: [
                source_area.x as f32,
                source_area.y as f32,
                source_area.width as f32,
                source_area.height as f32,
            ],
            settings,
        };
        let mut encoder = self.encoder();
        self.compositor.encode(
            &self.device,
            &mut encoder,
            destination,
            source,
            &temporary,
            FORMAT,
            params,
        );
        encode_copy(&mut encoder, &temporary, PixelRect::full(&temporary), destination);
        self.queue.submit([encoder.finish()]);
        self.used = checkpoint;
    }

    fn copy(&self, source: &wgpu::TextureView, area: PixelRect, destination: &wgpu::TextureView) {
        let mut encoder = self.encoder();
        encode_copy(&mut encoder, source, area, destination);
        self.queue.submit([encoder.finish()]);
    }

    fn encoder(&self) -> wgpu::CommandEncoder {
        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Vello GPU frame"),
        })
    }

    fn scratch(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        let index = self.used;
        self.used += 1;
        if let Some(view) = self.scratch.get_mut(index) {
            if dimensions(view) != (width, height) {
                *view = target(&self.device, width, height);
            }
            return view.clone();
        }
        let view = target(&self.device, width, height);
        self.scratch.push(view.clone());
        view
    }

    fn validate_dimensions(&self, width: u32, height: u32) -> Result<(), String> {
        let cap = self.device.limits().max_texture_dimension_2d.min(u32::from(u16::MAX));
        if width == 0 || height == 0 || width.max(height) > cap {
            return Err(format!(
                "Vello GPU target {width}×{height} exceeds device limits (1–{cap})"
            ));
        }
        Ok(())
    }

    fn validate_target(&self, view: &wgpu::TextureView) -> Result<(), String> {
        let (width, height) = dimensions(view);
        self.validate_dimensions(width, height)?;
        if view.texture().format() != FORMAT || !view.texture().usage().contains(TARGET_USAGES) {
            return Err(
                "Vello GPU frame target must be a sampleable and copyable RGBA8 attachment".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl PixelRect {
    fn full(view: &wgpu::TextureView) -> Self {
        let (width, height) = dimensions(view);
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn rasterized(region: EffectRegion, raster: Affine, view: &wgpu::TextureView) -> Option<Self> {
        let [x, y] = region.origin.map(f64::from);
        let rect = raster.transform_rect_bbox(Rect::new(
            x,
            y,
            x + f64::from(region.size.width),
            y + f64::from(region.size.height),
        ));
        let (width, height) = dimensions(view);
        let x = rect.x0.floor().clamp(0.0, f64::from(width)) as u32;
        let y = rect.y0.floor().clamp(0.0, f64::from(height)) as u32;
        let right = rect.x1.ceil().clamp(0.0, f64::from(width)) as u32;
        let bottom = rect.y1.ceil().clamp(0.0, f64::from(height)) as u32;
        (right > x && bottom > y).then(|| Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    fn rect(self) -> Rect {
        Rect::new(
            self.x.into(),
            self.y.into(),
            (self.x + self.width).into(),
            (self.y + self.height).into(),
        )
    }
}

fn dimensions(view: &wgpu::TextureView) -> (u32, u32) {
    (view.texture().width(), view.texture().height())
}

fn contains_effects(draws: &[Draw]) -> bool {
    draws
        .iter()
        .any(|draw| matches!(draw, Draw::Filter { .. } | Draw::Glass { .. }))
}

fn raster_scale(raster: Affine) -> f32 {
    let [a, b, ..] = raster.as_coeffs();
    a.hypot(b) as f32
}

fn target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Vello GPU frame target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: TARGET_USAGES,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn encode_copy(
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::TextureView,
    rect: PixelRect,
    destination: &wgpu::TextureView,
) {
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            origin: wgpu::Origin3d {
                x: rect.x,
                y: rect.y,
                z: 0,
            },
            ..source.texture().as_image_copy()
        },
        destination.texture().as_image_copy(),
        wgpu::Extent3d {
            width: rect.width,
            height: rect.height,
            depth_or_array_layers: 1,
        },
    );
}

fn transform_draw(mut draw: Draw, raster: Affine) -> Draw {
    match &mut draw {
        Draw::Fill { transform, .. }
        | Draw::Stroke { transform, .. }
        | Draw::Push { transform, .. }
        | Draw::BlurredRect { transform, .. }
        | Draw::Glyphs { transform, .. }
        | Draw::Texture { transform, .. } => *transform = raster * *transform,
        Draw::Filter { .. } | Draw::Glass { .. } | Draw::Pop => {},
    }
    draw
}
