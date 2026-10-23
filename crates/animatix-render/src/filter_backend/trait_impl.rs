use super::readback::*;
use super::*;

impl FilterBackend for GpuFilterBackend {
    fn set_raster_scale(&mut self, scale: f32) {
        self.raster_scale = scale.clamp(0.05, 1.0);
    }

    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String> {
        self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;

        if let Some((texture, _)) = &self.last_region_pp {
            // Region-scoped result: contents cover `last_effect_dims` from
            // (0, 0) inside the scratch texture.
            let t = timing_probe();
            let image = self.readback_to_scene_image_at(
                texture,
                wgpu::Origin3d::ZERO,
                self.last_effect_dims,
            );
            if t.is_some() {
                eprintln!(
                    "[filter-timing] readback dims={}x{} took={:.2}ms",
                    self.last_effect_dims.width,
                    self.last_effect_dims.height,
                    probe_ms(t),
                );
            }
            return image;
        }

        let texture = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        let (origin, dims) = match self.last_region {
            Some(region) => (
                wgpu::Origin3d {
                    x: scaled_origin(region.origin[0], self.raster_scale),
                    y: scaled_origin(region.origin[1], self.raster_scale),
                    z: 0,
                },
                self.last_effect_dims,
            ),
            None => (wgpu::Origin3d::ZERO, self.last_effect_dims),
        };
        self.readback_to_scene_image_at(texture, origin, dims)
    }

    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        // The composite is emitted in target pixels — origin included, because
        // the texture it carries is `raster_scale` of a scene pixel wide. The
        // drain divides its frame scale by `texel_scale` to land the whole thing
        // without shrinking it a second time.
        let origin = region.map_or([0.0, 0.0], |region| {
            [
                region.origin[0] * self.raster_scale,
                region.origin[1] * self.raster_scale,
            ]
        });
        let t_copy = timing_probe();

        if chain.is_empty() {
            self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;
            let harvest = self.last_effect_dims;
            let (source, source_origin) = if let Some((texture, _)) = &self.last_region_pp {
                (texture, wgpu::Origin3d::ZERO)
            } else {
                let texture = match self.last_filtered_source {
                    FilteredSource::Render => &self.render_texture,
                    FilteredSource::TexA => &self.tex_a,
                    FilteredSource::TexB => &self.tex_b,
                };
                let source_origin = match self.last_region {
                    Some(region) => wgpu::Origin3d {
                        x: scaled_origin(region.origin[0], self.raster_scale),
                        y: scaled_origin(region.origin[1], self.raster_scale),
                        z: 0,
                    },
                    None => wgpu::Origin3d::ZERO,
                };
                (texture, source_origin)
            };
            let composite =
                self.copy_last_filtered_to_pending(harvest, alpha, origin, source, source_origin)?;
            self.pending_composites.push(PendingLayer::Composite(composite));
            return Ok(());
        }

        // When chain has effect passes, fuse compute pass encoding and pending composite copying:
        let harvest = match region {
            Some(r) => SceneDimensions {
                width: scaled_texels(r.size.width, self.raster_scale),
                height: scaled_texels(r.size.height, self.raster_scale),
            },
            None => SceneDimensions {
                width: scaled_texels(dimensions.width, self.raster_scale),
                height: scaled_texels(dimensions.height, self.raster_scale),
            },
        };
        let harvest_req = PendingHarvestReq {
            dimensions: harvest,
            alpha,
            origin,
            source_origin: wgpu::Origin3d::ZERO,
        };
        let (_view, composite) = self.render_and_filter_scene_fused(
            scene,
            dimensions,
            region,
            chain,
            Some(harvest_req),
        )?;
        if let Some(comp) = composite {
            self.pending_composites.push(PendingLayer::Composite(comp));
        }
        if t_copy.is_some() {
            eprintln!(
                "[filter-timing] pending dims={}x{} render+chain+fused_copy={:.2}ms",
                harvest.width,
                harvest.height,
                probe_ms(t_copy),
            );
        }
        Ok(())
    }

    fn take_pending_composites(&mut self) -> Vec<PendingLayer> {
        std::mem::take(&mut self.pending_composites)
    }

    fn recycle_pending_composite(&mut self, composite: PendingComposite) {
        let size = composite.texture.size();
        let key = (size.width, size.height);
        let mut pool = self.pending_texture_pool.borrow_mut();
        let list = pool.entry(key).or_default();
        if list.len() < 8 {
            list.push((composite.texture, composite.view));
        }
    }

    /// Queue the panel's own children as a plain composite: the scene renders
    /// into the region scratch with a transparent clear and an empty chain, then
    /// the copy is queued. It lands *after* whatever is already in the queue, so
    /// a `Glass` scope's children composite above its backdrop.
    fn enqueue_scene_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        origin: [f32; 2],
        alpha: f32,
    ) -> Result<(), String> {
        let view =
            self.render_and_filter_scene_to_view(scene, dimensions, None, &EffectChain::default())?;
        let harvest = self.last_effect_dims;
        let source = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        let composite = self.copy_last_filtered_to_pending(
            harvest,
            alpha,
            origin,
            source,
            wgpu::Origin3d::ZERO,
        )?;
        let _ = view;
        self.pending_composites.push(PendingLayer::Composite(composite));
        Ok(())
    }

    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        self.pending_composites.push(PendingLayer::Backdrop(backdrop.clone()));
        Ok(())
    }

    /// Blur the panel's own slice of the finished frame.
    ///
    /// Runs at drain time because the pixels only exist after the main scene
    /// render: the target's contents are copied into the region scratch (which
    /// `Filter` scopes already own and reuse), run through the scope's chain, and
    /// handed back as a composite clipped to the panel's rect and corners.
    fn run_backdrop(
        &mut self,
        target: &wgpu::Texture,
        backdrop: &PendingBackdrop,
    ) -> Result<PendingComposite, String> {
        let size = target.size();
        let canvas = SceneDimensions {
            width: size.width,
            height: size.height,
        };
        let region = backdrop.region;
        let (w, h) = Self::quantize_region_size(region.size, canvas);
        let dispatch = SceneDimensions {
            width: w,
            height: h,
        };
        let scratch = self.region_scratch_for(w, h);

        // Clear the seed first. The copy below writes only the panel's rect; the
        // quantized padding ring would otherwise hold the previous frame's
        // texels, and a blur reaching into them smears that garbage inward.
        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &scratch.render_view,
                w,
                h,
                &vello::Scene::new(),
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;

        // The copy has to stay inside both textures. The region is already
        // clamped to the canvas by `region_from_rect`, but its quantized dispatch
        // size is rounded *up*, so the padded region can hang off the right or
        // bottom edge.
        let ox = (region.origin[0].max(0.0).round() as u32).min(canvas.width);
        let oy = (region.origin[1].max(0.0).round() as u32).min(canvas.height);
        let copy_w = region.size.width.min(w).min(canvas.width - ox);
        let copy_h = region.size.height.min(h).min(canvas.height - oy);
        if copy_w > 0 && copy_h > 0 {
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Backdrop Copy Encoder"),
            });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: ox, y: oy, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &scratch.render_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: copy_w,
                    height: copy_h,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        let seed = ChainSeed {
            seed_texture: scratch.render_texture.clone(),
            pp_a: scratch.pp_a.clone(),
            pp_a_view: scratch.pp_a_view.clone(),
            pp_b: scratch.pp_b.clone(),
            pp_b_view: scratch.pp_b_view.clone(),
            pp_original: scratch.pp_original.clone(),
            pp_original_view: scratch.pp_original_view.clone(),
        };
        self.last_effect_dims = region.size;
        self.last_region = Some(region);
        self.last_region_pp = None;
        let harvest_req = PendingHarvestReq {
            dimensions: region.size,
            alpha: backdrop.alpha,
            origin: backdrop.clip.origin,
            source_origin: wgpu::Origin3d::ZERO,
        };
        let (_view, composite_opt) =
            self.run_chain_passes_fused(seed, dispatch, &backdrop.chain, true, Some(harvest_req))?;
        let mut composite =
            composite_opt.ok_or_else(|| "backdrop chain produced no composite".to_string())?;

        // The panel rect lives *inside* the padded result: the composite samples
        // just that window and paints it back where it came from, shaped by the
        // panel's own corners.
        let px0 = (backdrop.clip.origin[0] - region.origin[0]).max(0.0);
        let py0 = (backdrop.clip.origin[1] - region.origin[1]).max(0.0);
        let px1 = px0 + backdrop.clip.size.width as f32;
        let py1 = py0 + backdrop.clip.size.height as f32;
        composite.src_rect = Some([px0, py0, px1, py1]);
        composite.clip_rect = Some([
            backdrop.clip.origin[0],
            backdrop.clip.origin[1],
            backdrop.clip.origin[0] + backdrop.clip.size.width as f32,
            backdrop.clip.origin[1] + backdrop.clip.size.height as f32,
        ]);
        composite.corner_radius = backdrop.corner_radius;
        Ok(composite)
    }
}
