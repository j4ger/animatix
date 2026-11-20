//! Shared Vello GPU rasterizer for engine-owned scenes.
//!
//! The renderer and its `Resources` are a persistent pair. Every encoded render
//! must be submitted before the pair is reused, because Vello GPU recycles
//! buffers and intermediate resources between calls.

use std::collections::HashMap;

use animatix::scene::{Draw, Scene};
use animatix_core::error::RenderError;
use glifo::Glyph as GpuGlyph;
use kurbo::Affine;
use peniko::{Brush, Compose, ImageAlphaType, ImageFormat};
use vello_gpu::{
    ClearSettings, Image, ImageSource, RenderOptions, RenderTarget, RenderTargetConfig, Resources,
    Scene as GpuScene, TargetInit, TextureBindings, TextureId, TextureRegion,
};
use wgpu::{CommandEncoder, Device, Queue, TextureFormat, TextureView};

/// RGBA8 2D texture views keyed by the symbolic `Draw::Texture` ID.
///
/// Vello GPU samples these as filterable non-array 2D textures; the underlying
/// textures must have `TEXTURE_BINDING` usage.
pub type ExternalTextures = HashMap<u64, TextureView>;

const MAX_CACHED_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_CACHED_IMAGE_COUNT: usize = 256;

/// Persistent Vello GPU renderer state and uploaded raster images.
pub struct RasterGpu {
    device: Device,
    queue: Queue,
    renderer: vello_gpu::Renderer,
    resources: Resources,
    image_views: ImageViewCache,
}

impl RasterGpu {
    /// Create a renderer for `Rgba8Unorm` render targets.
    ///
    /// The initial allocation is one pixel; Vello GPU grows its working
    /// textures to match the first real target. Target formats are fixed for a
    /// renderer instance, so callers should raster into an internal RGBA8
    /// texture and present through a format-specific compositor.
    pub fn new(device: &Device, queue: &Queue) -> Self {
        let (renderer, resources) = vello_gpu::Renderer::new(
            device,
            queue,
            &RenderTargetConfig {
                format: TextureFormat::Rgba8Unorm,
                width: 1,
                height: 1,
            },
        );
        Self {
            device: device.clone(),
            queue: queue.clone(),
            renderer,
            resources,
            image_views: ImageViewCache::default(),
        }
    }

    /// Render all draws from `scene`, clearing the target to transparent.
    pub fn render_scene(
        &mut self,
        scene: &Scene,
        dst: &TextureView,
        raster: Affine,
        external: Option<&ExternalTextures>,
    ) -> Result<(), RenderError> {
        self.render_draws(
            scene.draws(),
            dst,
            raster,
            external,
            TargetInit::Clear(ClearSettings::default()),
        )
    }

    /// Encode all draws from `scene` into a caller-owned encoder.
    ///
    /// This leaves submission to the caller, allowing a composition pass to
    /// follow the raster pass in the same command buffer.
    pub fn encode_scene(
        &mut self,
        scene: &Scene,
        dst: &TextureView,
        raster: Affine,
        external: Option<&ExternalTextures>,
        target_init: TargetInit<'_>,
        encoder: &mut CommandEncoder,
    ) -> Result<(), RenderError> {
        self.encode_draws(scene.draws(), dst, raster, external, target_init, encoder)
    }

    /// Render a contiguous run of ordinary draws using the requested target
    /// initialization policy.
    pub fn render_draws(
        &mut self,
        draws: &[Draw],
        dst: &TextureView,
        raster: Affine,
        external: Option<&ExternalTextures>,
        target_init: TargetInit<'_>,
    ) -> Result<(), RenderError> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Vello GPU raster encoder"),
        });
        let result = self.encode_draws(draws, dst, raster, external, target_init, &mut encoder);
        // Vello GPU reuses resources across calls and requires every render's
        // encoder to be submitted, including a render that reports an error.
        self.queue.submit([encoder.finish()]);
        result
    }

    /// Encode a contiguous run of ordinary draws into `encoder`.
    ///
    /// `TargetInit::Clear(ClearSettings::default())` clears to transparent and
    /// `TargetInit::SrcOver` preserves existing target contents. The encoder
    /// must be submitted before this renderer is used again.
    pub fn encode_draws(
        &mut self,
        draws: &[Draw],
        dst: &TextureView,
        raster: Affine,
        external: Option<&ExternalTextures>,
        target_init: TargetInit<'_>,
        encoder: &mut CommandEncoder,
    ) -> Result<(), RenderError> {
        let texture = dst.texture();
        if texture.format() != TextureFormat::Rgba8Unorm
            || !texture.usage().contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(RenderError::FrameRender(
                "Vello GPU target must be an Rgba8Unorm render attachment".into(),
            ));
        }
        let max_dimension = self.device.limits().max_texture_dimension_2d.min(u32::from(u16::MAX));
        if texture.width() > max_dimension || texture.height() > max_dimension {
            return Err(RenderError::FrameRender(format!(
                "Vello GPU target {}x{} exceeds device limit {max_dimension}",
                texture.width(),
                texture.height()
            )));
        }
        let width = u16::try_from(texture.width()).map_err(|_| {
            RenderError::FrameRender(format!(
                "Vello GPU target width {} exceeds its u16 limit",
                texture.width()
            ))
        })?;
        let height = u16::try_from(texture.height()).map_err(|_| {
            RenderError::FrameRender(format!(
                "Vello GPU target height {} exceeds its u16 limit",
                texture.height()
            ))
        })?;
        if width == 0 || height == 0 {
            return Err(RenderError::FrameRender(
                "Vello GPU target dimensions must be non-zero".into(),
            ));
        }

        let mut scene = GpuScene::new(width, height);
        let mut bindings = TextureBindings::new();
        if let Some(external) = external {
            for (&id, view) in external {
                if id & (1u64 << 63) != 0 {
                    return Err(RenderError::FrameRender(format!(
                        "external texture id {id} uses the reserved image-cache namespace"
                    )));
                }
                if view.texture().width() > u32::from(u16::MAX)
                    || view.texture().height() > u32::from(u16::MAX)
                {
                    return Err(RenderError::FrameRender(format!(
                        "external texture {id} exceeds Vello GPU's u16 texture limit"
                    )));
                }
                let texture = view.texture();
                if texture.dimension() != wgpu::TextureDimension::D2
                    || texture.depth_or_array_layers() != 1
                    || texture.format() != TextureFormat::Rgba8Unorm
                    || !texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
                {
                    return Err(RenderError::FrameRender(format!(
                        "external texture {id} must be a filterable Rgba8Unorm 2D texture with TEXTURE_BINDING usage"
                    )));
                }
                bindings.insert(TextureId(id), view.clone());
            }
        }
        validate_draws(draws)?;
        encode_draws_to_scene(
            &mut scene,
            &mut self.resources,
            &self.device,
            &self.queue,
            draws,
            raster,
            external,
            &mut bindings,
            &mut self.image_views,
        )?;

        let mut options = RenderOptions::default();
        options.target_init = target_init;
        self.renderer
            .render(
                &scene,
                &mut self.resources,
                encoder,
                RenderTarget {
                    view: dst,
                    depth_view: None,
                },
                &bindings,
                options,
            )
            .map_err(|error| RenderError::FrameRender(format!("{error:?}")))
    }

    /// Clear cached decoded-image textures while retaining glyph and pipeline
    /// state.
    pub fn clear_images(&mut self) {
        self.image_views.clear();
    }

    /// Release Vello GPU allocations and all uploaded images.
    pub fn reset_and_shrink(&mut self) {
        self.renderer.reset_and_shrink(&mut self.resources);
        self.image_views.clear();
    }
}

#[derive(Clone, Copy)]
enum StackItem {
    Clip,
    Layer,
}

#[allow(clippy::too_many_arguments)]
fn encode_draws_to_scene(
    scene: &mut GpuScene,
    resources: &mut Resources,
    device: &Device,
    queue: &Queue,
    draws: &[Draw],
    raster: Affine,
    external: Option<&ExternalTextures>,
    bindings: &mut TextureBindings,
    image_views: &mut ImageViewCache,
) -> Result<(), RenderError> {
    let mut stack = Vec::new();
    for draw in draws {
        match draw {
            Draw::Fill {
                path,
                rule,
                transform,
                brush,
                brush_transform,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(*rule);
                set_brush(scene, brush, *brush_transform, device, queue, bindings, image_views)?;
                scene.fill_path(path);
            },
            Draw::Stroke {
                path,
                style,
                transform,
                brush,
                brush_transform,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_stroke(style.clone());
                set_brush(scene, brush, *brush_transform, device, queue, bindings, image_views)?;
                scene.stroke_path(path);
            },
            Draw::Push {
                clip,
                fill,
                transform,
                blend,
                alpha,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(*fill);
                if let Some(blend) = blend {
                    scene.push_layer(Some(clip), Some(*blend), Some(*alpha), None, None);
                    stack.push(StackItem::Layer);
                } else {
                    scene.push_clip_path(clip);
                    stack.push(StackItem::Clip);
                }
            },
            Draw::Pop => match stack.pop() {
                Some(StackItem::Clip) => scene.pop_clip(),
                Some(StackItem::Layer) => scene.pop_layer(),
                None => {
                    return Err(RenderError::FrameRender(
                        "neutral scene contains an unmatched pop".into(),
                    ));
                },
            },
            Draw::BlurredRect {
                clip,
                transform,
                rect,
                color,
                corner,
                std_dev,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(peniko::Fill::NonZero);
                scene.reset_paint_transform();
                scene.set_paint(*color);
                scene.fill_blurred_rounded_rect_in(clip, rect, *corner, *std_dev, false);
            },
            Draw::Glyphs {
                font,
                size,
                transform,
                glyph_transform,
                coords,
                glyphs,
                hint,
                color,
            } => {
                scene.set_transform(raster * *transform);
                scene.reset_paint_transform();
                scene.set_paint(*color);
                let mut run = scene
                    .glyph_run(resources, font)
                    .font_size(*size)
                    .normalized_coords(coords)
                    .hint(*hint);
                if let Some(transform) = glyph_transform {
                    run = run.glyph_transform(*transform);
                }
                run.fill_glyphs(glyphs.iter().map(|glyph| GpuGlyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                }))
                .map_err(|error| {
                    RenderError::FrameRender(format!("Vello GPU glyph encoding failed: {error}"))
                })?;
            },
            Draw::Texture {
                id,
                rect,
                opacity,
                transform,
            } => {
                let view = external.and_then(|textures| textures.get(id)).ok_or_else(|| {
                    RenderError::FrameRender(format!(
                        "missing external texture binding for scene texture {id}"
                    ))
                })?;
                let width = f64::from(view.texture().width());
                let height = f64::from(view.texture().height());
                if width <= 0.0 || height <= 0.0 {
                    return Err(RenderError::FrameRender(format!(
                        "external texture {id} has an empty extent"
                    )));
                }
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(peniko::Fill::NonZero);
                scene.set_paint_transform(
                    Affine::translate((rect.x0, rect.y0))
                        * Affine::scale_non_uniform(rect.width() / width, rect.height() / height),
                );
                scene.set_paint(Image {
                    image: ImageSource::external_texture(TextureId(*id), TextureRegion::Full, true),
                    sampler: peniko::ImageSampler::new().with_alpha(*opacity),
                });
                scene.fill_rect(rect);
            },
            Draw::Filter { .. } | Draw::Glass { .. } => {
                return Err(RenderError::FrameRender(
                    "scene effects must be lowered by the frame compositor".into(),
                ));
            },
        }
    }
    if !stack.is_empty() {
        return Err(RenderError::FrameRender("neutral scene contains an unclosed layer".into()));
    }
    Ok(())
}

fn set_brush(
    scene: &mut GpuScene,
    brush: &Brush,
    transform: Option<Affine>,
    device: &Device,
    queue: &Queue,
    bindings: &mut TextureBindings,
    image_views: &mut ImageViewCache,
) -> Result<(), RenderError> {
    scene.set_paint_transform(transform.unwrap_or(Affine::IDENTITY));
    match brush {
        Brush::Solid(color) => scene.set_paint(*color),
        Brush::Gradient(gradient) => scene.set_paint(gradient.clone()),
        Brush::Image(brush) => {
            let image = bind_image(device, queue, brush, bindings, image_views)?;
            scene.set_paint(image);
        },
    }
    Ok(())
}

fn bind_image(
    device: &Device,
    queue: &Queue,
    brush: &peniko::ImageBrush,
    bindings: &mut TextureBindings,
    image_views: &mut ImageViewCache,
) -> Result<Image, RenderError> {
    let id = brush.image.data.id() | (1u64 << 63);
    let view = if let Some(view) = image_views.get(id) {
        view
    } else {
        let view = upload_image(device, queue, brush)?;
        let bytes = image_byte_len(brush)?;
        image_views.insert(id, view.clone(), bytes);
        view
    };
    let texture_id = TextureId(id);
    bindings.insert(texture_id, view);
    Ok(Image {
        image: ImageSource::external_texture(texture_id, TextureRegion::Full, true),
        sampler: brush.sampler,
    })
}

fn upload_image(
    device: &Device,
    queue: &Queue,
    brush: &peniko::ImageBrush,
) -> Result<TextureView, RenderError> {
    let image = &brush.image;
    let max_dimension = device.limits().max_texture_dimension_2d.min(u32::from(u16::MAX));
    if image.width == 0 || image.height == 0 || image.width.max(image.height) > max_dimension {
        return Err(RenderError::FrameRender(format!(
            "image dimensions {}x{} exceed Vello GPU texture limits",
            image.width, image.height
        )));
    }
    let mut bytes = image.data.data().to_vec();
    if image.format != ImageFormat::Rgba8 && image.format != ImageFormat::Bgra8 {
        return Err(RenderError::FrameRender(format!(
            "Vello GPU image upload does not support {:?}",
            image.format
        )));
    }
    if Some(bytes.len()) != image.format.size_in_bytes(image.width, image.height) {
        return Err(RenderError::FrameRender(
            "image data byte count does not match its dimensions".into(),
        ));
    }
    if image.format == ImageFormat::Bgra8 || image.alpha_type == ImageAlphaType::Alpha {
        for pixel in bytes.as_chunks_mut::<4>().0 {
            if image.format == ImageFormat::Bgra8 {
                pixel.swap(0, 2);
            }
            if image.alpha_type == ImageAlphaType::Alpha {
                let alpha = u16::from(pixel[3]);
                for channel in &mut pixel[..3] {
                    *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                }
            }
        }
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix Vello GPU image"),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width * 4),
            rows_per_image: Some(image.height),
        },
        wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
    );
    Ok(texture.create_view(&wgpu::TextureViewDescriptor::default()))
}

fn validate_draws(draws: &[Draw]) -> Result<(), RenderError> {
    for draw in draws {
        if let Draw::Push {
            blend: Some(blend), ..
        } = draw
            && blend.compose != Compose::SrcOver
        {
            return Err(RenderError::FrameRender(format!(
                "Vello GPU currently admits isolated SrcOver blend layers only; {:?} composition must be handled by a dedicated compositor",
                blend.compose
            )));
        }
    }
    Ok(())
}

fn image_byte_len(brush: &peniko::ImageBrush) -> Result<usize, RenderError> {
    brush
        .image
        .format
        .size_in_bytes(brush.image.width, brush.image.height)
        .ok_or_else(|| RenderError::FrameRender("image byte size overflows address space".into()))
}

#[derive(Default)]
struct ImageViewCache {
    entries: HashMap<u64, ImageViewEntry>,
    bytes: usize,
    clock: u64,
}

struct ImageViewEntry {
    view: TextureView,
    bytes: usize,
    last_used: u64,
}

impl ImageViewCache {
    fn get(&mut self, id: u64) -> Option<TextureView> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(&id)?;
        entry.last_used = self.clock;
        Some(entry.view.clone())
    }

    fn insert(&mut self, id: u64, view: TextureView, bytes: usize) {
        if bytes > MAX_CACHED_IMAGE_BYTES {
            // The current render bindings keep this view alive through the
            // submission; don't pin an oversized image across source reloads.
            return;
        }
        if let Some(previous) = self.entries.remove(&id) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
        }
        while self.entries.len() >= MAX_CACHED_IMAGE_COUNT
            || self.bytes.saturating_add(bytes) > MAX_CACHED_IMAGE_BYTES
        {
            let Some((&oldest_id, _)) =
                self.entries.iter().min_by_key(|(_, entry)| entry.last_used)
            else {
                break;
            };
            if let Some(oldest) = self.entries.remove(&oldest_id) {
                self.bytes = self.bytes.saturating_sub(oldest.bytes);
            }
        }
        self.clock = self.clock.wrapping_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.insert(
            id,
            ImageViewEntry {
                view,
                bytes,
                last_used: self.clock,
            },
        );
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
        self.clock = 0;
    }
}
