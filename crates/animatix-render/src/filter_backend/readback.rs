use super::*;

impl GpuFilterBackend {
    pub(super) fn quantize_region_size(
        size: SceneDimensions,
        canvas: SceneDimensions,
    ) -> (u32, u32) {
        let step = |v: u32, max: u32| (v.div_ceil(64).max(1) * 64).min(max.max(1));
        (step(size.width, canvas.width), step(size.height, canvas.height))
    }

    /// Get (creating on first use) the region-scoped scratch targets for a
    /// quantized size. Textures are cloned out (refcount bumps) so the cache
    /// stays owned by the backend across the mutable render call.
    pub(super) fn region_scratch_for(&mut self, width: u32, height: u32) -> RegionScratch {
        const MAX_REGION_SCRATCH_ENTRIES: usize = 12;
        if self.region_scratch.len() > MAX_REGION_SCRATCH_ENTRIES {
            // A scene uses a handful of distinct ROIs; if churn ever exceeds
            // the cap, start over rather than growing unbounded.
            self.region_scratch.clear();
        }
        if let Some(scratch) = self.region_scratch.get(&(width, height)) {
            return scratch.clone();
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let make = |label: &str| {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                label: Some(label),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let (render_texture, render_view) = make("Animatix Region Render Target");
        let (pp_a, pp_a_view) = make("Animatix Region Ping A");
        let (pp_b, pp_b_view) = make("Animatix Region Ping B");
        let (pp_original, pp_original_view) = make("Animatix Region Original");
        let scratch = RegionScratch {
            render_texture,
            render_view,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        };
        self.region_scratch.insert((width, height), scratch.clone());
        scratch
    }

    /// Debug instrumentation: read a texture region back as a [`SceneImage`].
    pub fn debug_readback_texture(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
        dims: SceneDimensions,
    ) -> SceneImage {
        self.readback_to_scene_image_at(texture, origin, dims)
            .expect("debug texture readback")
    }

    /// Read a texture back into a [`SceneImage`].
    pub(super) fn readback_to_scene_image_at(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        // This path blocks: `map_async` hands its result to a channel and
        // `device.poll(Wait)` then drains it. Neither can complete on wasm,
        // where the callback only runs when control returns to the browser event
        // loop — so the first `recv()` hangs the tab outright, and the poisoned
        // device left in the shared context takes every later render in the page
        // down with it. The engine's caller degrades to unfiltered rendering on
        // `Err`, which is the honest outcome in a browser until this becomes
        // async end to end.
        if cfg!(target_arch = "wasm32") {
            return Err(
                "synchronous texture readback cannot complete on wasm; the filtered region is \
                 rendered unfiltered"
                    .into(),
            );
        }

        let output_buffer = &self.output_buffer;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Filter Readback Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.bytes_per_row),
                    rows_per_image: Some(dimensions.height),
                },
            },
            wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).ok();
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|err| format!("Failed to poll GPU device: {err}"))?;
        rx.recv()
            .map_err(|err| format!("Failed to receive mapped frame: {err}"))?
            .map_err(|err| format!("Failed to map filter frame: {err}"))?;

        let data = buffer_slice.get_mapped_range();
        let mut rgba = vec![0; (dimensions.width * dimensions.height * 4) as usize];
        for y in 0..dimensions.height as usize {
            let src_row = &data[y * self.bytes_per_row as usize
                ..y * self.bytes_per_row as usize + dimensions.width as usize * 4];
            let dst_row = &mut rgba
                [y * dimensions.width as usize * 4..(y + 1) * dimensions.width as usize * 4];
            dst_row.copy_from_slice(src_row);
        }
        drop(data);
        output_buffer.unmap();

        let data = vello::peniko::ImageData {
            data: rgba.into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: dimensions.width,
            height: dimensions.height,
        };

        Ok(SceneImage {
            data,
            natural_size: [dimensions.width as f32, dimensions.height as f32],
        })
    }

    /// Acquire a pending composite texture from the pool or allocate a new one.
    pub(super) fn acquire_pending_texture(
        &self,
        dimensions: SceneDimensions,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let key = (dimensions.width, dimensions.height);
        if let Some(entry) =
            self.pending_texture_pool.borrow_mut().get_mut(&key).and_then(|v| v.pop())
        {
            entry
        } else {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: dimensions.width,
                    height: dimensions.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                label: Some("Animatix Pending Filter Composite Texture"),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        }
    }

    /// Record a copy from `source` into a pooled pending composite texture inside `encoder`.
    pub(super) fn copy_texture_to_pending_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
    ) -> PendingComposite {
        let (texture, view) = self.acquire_pending_texture(dimensions);
        let copy_width = dimensions.width.min(source.width().saturating_sub(source_origin.x));
        let copy_height = dimensions.height.min(source.height().saturating_sub(source_origin.y));

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: source_origin,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: copy_width,
                height: copy_height,
                depth_or_array_layers: 1,
            },
        );

        PendingComposite {
            texture,
            view,
            alpha,
            origin,
            texel_scale: self.raster_scale,
            corner_radius: 0.0,
            src_rect: None,
            clip_rect: None,
        }
    }

    /// Copy the most recent filtered result to a dedicated texture for
    /// deferred compositing.
    pub(super) fn copy_last_filtered_to_pending(
        &self,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
        source: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
    ) -> Result<PendingComposite, String> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Pending Filter Composite Copy Encoder"),
        });
        let composite = self.copy_texture_to_pending_composite(
            &mut encoder,
            source,
            source_origin,
            dimensions,
            alpha,
            origin,
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        Ok(composite)
    }
}

/// Texel count for a scene-pixel extent at raster scale `s`, never zero.
pub(super) fn scaled_texels(extent: u32, s: f32) -> u32 {
    ((extent as f32 * s).round() as u32).max(1)
}

/// Texel coordinate for a scene-pixel origin at raster scale `s` (can be zero).
pub(super) fn scaled_origin(coord: f32, s: f32) -> u32 {
    (coord.max(0.0) * s).round() as u32
}

/// Multiply every `Pixel`-unit `f32` lane in a packed author uniform by `scale`.
///
/// `size` is the byte width the spec declared, so a `Vec2` offset covers two
/// lanes. Non-`F32`-sized lanes cannot appear here: the only pixel parameters in
/// the catalog are floats and float pairs.
pub(super) fn scale_pixel_params(params: &[EffectParamSpec], scale: f32, uniforms: &mut [u8]) {
    for spec in params.iter().filter(|spec| spec.is_pixel()) {
        let start = spec.offset as usize;
        let end = (spec.offset as usize + spec.size as usize).min(uniforms.len());
        for lane in uniforms[start..end].chunks_exact_mut(4) {
            let arr: [u8; 4] = lane.try_into().expect("chunks_exact(4) yields 4 bytes");
            let v = f32::from_le_bytes(arr) * scale;
            lane.copy_from_slice(&v.to_le_bytes());
        }
    }
}
