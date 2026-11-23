use super::{OffscreenRenderer, SceneDimensions};

#[derive(Clone, Copy)]
pub(super) struct CanvasMapping {
    pub(super) logical: SceneDimensions,
    pub(super) output: SceneDimensions,
    pub(super) raster: SceneDimensions,
    pub(super) transform: kurbo::Affine,
}

impl CanvasMapping {
    pub(super) fn new(logical: SceneDimensions, output: SceneDimensions) -> Self {
        if logical == output {
            return Self {
                logical,
                output,
                raster: output,
                transform: kurbo::Affine::IDENTITY,
            };
        }
        let scale = (f64::from(output.width) / f64::from(logical.width))
            .min(f64::from(output.height) / f64::from(logical.height));
        let raster = SceneDimensions {
            width: ((f64::from(logical.width) * scale).round() as u32).max(1),
            height: ((f64::from(logical.height) * scale).round() as u32).max(1),
        };
        Self {
            logical,
            output,
            raster,
            transform: kurbo::Affine::scale(scale),
        }
    }
}

pub(super) struct FittedTargets {
    size: SceneDimensions,
    pub(super) output: wgpu::TextureView,
    pub(super) sides: Option<[wgpu::TextureView; 2]>,
}

impl OffscreenRenderer {
    pub(super) fn prepare_canvas(
        &mut self,
        mapping: CanvasMapping,
        transition: bool,
    ) -> Result<(), String> {
        let output = mapping.output;
        let limits = self.device.limits();
        if output.width == 0 || output.height == 0 {
            return Err("Output dimensions must be greater than zero".to_string());
        }
        let row_bytes = (u64::from(output.width) * 4).next_multiple_of(256);
        if output.width > limits.max_texture_dimension_2d
            || output.height > limits.max_texture_dimension_2d
            || row_bytes * u64::from(output.height) > limits.max_buffer_size
        {
            return Err(format!(
                "Output dimensions {}×{} exceed GPU limits",
                output.width, output.height
            ));
        }
        self.ensure_targets(output);
        if mapping.raster != output {
            if self
                .fitted_targets
                .as_ref()
                .is_none_or(|targets| targets.size != mapping.raster)
            {
                self.fitted_targets = Some(FittedTargets {
                    size: mapping.raster,
                    output: fitted_view(&self.device, mapping.raster),
                    sides: None,
                });
            }
            if let Some(targets) = &mut self.fitted_targets
                && transition
                && targets.sides.is_none()
            {
                targets.sides =
                    Some(std::array::from_fn(|_| fitted_view(&self.device, mapping.raster)));
            }
        }
        Ok(())
    }

    pub(super) fn canvas_views(&self, mapping: CanvasMapping) -> Result<CanvasViews<'_>, String> {
        if mapping.raster != mapping.output {
            let targets = self.fitted_targets.as_ref().ok_or("Missing fitted render target")?;
            Ok(CanvasViews {
                output: &targets.output,
                sides: targets.sides.as_ref().map(|[a, b]| [a, b]),
            })
        } else {
            let output = self.output_view.as_ref().ok_or("Missing offscreen output view")?;
            let sides = self.view_a.as_ref().zip(self.view_b.as_ref()).map(|(a, b)| [a, b]);
            Ok(CanvasViews { output, sides })
        }
    }

    pub(super) fn present_canvas(&self, mapping: CanvasMapping) -> Result<(), String> {
        if mapping.raster == mapping.output {
            return Ok(());
        }
        let source = self.canvas_views(mapping)?.output;
        let destination = self.output_view.as_ref().ok_or("Missing offscreen output view")?;
        let blit = self.core.blit.as_ref().ok_or("Missing offscreen blit pipeline")?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        blit.blit_rect_with_encoder(
            &self.device,
            &self.queue,
            &mut encoder,
            source,
            destination,
            [
                ((mapping.output.width - mapping.raster.width) / 2) as f32,
                ((mapping.output.height - mapping.raster.height) / 2) as f32,
            ],
            Some([mapping.raster.width, mapping.raster.height]),
            1.0,
        );
        self.queue.submit([encoder.finish()]);
        Ok(())
    }
}

pub(super) struct CanvasViews<'a> {
    pub(super) output: &'a wgpu::TextureView,
    pub(super) sides: Option<[&'a wgpu::TextureView; 2]>,
}

fn fitted_view(device: &wgpu::Device, dimensions: SceneDimensions) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Fitted Canvas"),
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
