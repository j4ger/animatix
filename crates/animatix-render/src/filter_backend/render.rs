use super::readback::*;
use super::*;

impl GpuFilterBackend {
    pub(super) fn render_and_filter_scene_to_view(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<wgpu::TextureView, String> {
        self.render_and_filter_scene_fused(scene, dimensions, region, chain, None)
            .map(|(v, _)| v)
    }

    pub(super) fn render_and_filter_scene_fused(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        harvest_req: Option<PendingHarvestReq>,
    ) -> Result<(wgpu::TextureView, Option<PendingComposite>), String> {
        // The chain follows the raster it is being drawn into. A radius is a
        // distance in scene pixels, so the dispatch extent, the encode of the
        // scope's own sub-scene and every `Pixel` parameter (see
        // `scale_pixel_params`) shrink by the same factor — which is what makes
        // the result *look* the same at a lower raster instead of blurrier.
        let s = self.raster_scale;
        let mut wrapped;
        let (dispatch_dims, scene_ref, scratch) = match region {
            Some(r) => {
                let (width, height) = Self::quantize_region_size(r.size, dimensions);
                let (width, height) = (scaled_texels(width, s), scaled_texels(height, s));
                wrapped = vello::Scene::new();
                let shift = kurbo::Affine::translate(kurbo::Vec2::new(
                    -f64::from(r.origin[0]),
                    -f64::from(r.origin[1]),
                ));
                wrapped.append(
                    scene,
                    Some(if s == 1.0 {
                        shift
                    } else {
                        kurbo::Affine::scale(f64::from(s)) * shift
                    }),
                );
                (
                    SceneDimensions { width, height },
                    &wrapped,
                    Some(self.region_scratch_for(width, height)),
                )
            },
            None if s != 1.0 => {
                let width = scaled_texels(dimensions.width, s);
                let height = scaled_texels(dimensions.height, s);
                wrapped = vello::Scene::new();
                wrapped.append(scene, Some(kurbo::Affine::scale(f64::from(s))));
                (
                    SceneDimensions { width, height },
                    &wrapped,
                    Some(self.region_scratch_for(width, height)),
                )
            },
            None => (dimensions, scene, None),
        };
        let scratch = scratch.map(std::sync::Arc::new);
        let (
            render_view,
            seed_texture,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        ) = match &scratch {
            Some(s) => (
                s.render_view.clone(),
                s.render_texture.clone(),
                s.pp_a.clone(),
                s.pp_a_view.clone(),
                s.pp_b.clone(),
                s.pp_b_view.clone(),
                s.pp_original.clone(),
                s.pp_original_view.clone(),
            ),
            None => (
                self.render_view.clone(),
                self.render_texture.clone(),
                self.tex_a.clone(),
                self.tex_a_view.clone(),
                self.tex_b.clone(),
                self.tex_b_view.clone(),
                self.tex_original.clone(),
                self.tex_original_view.clone(),
            ),
        };

        // TEMPORARY perf probe (env-gated): per-stage cost of one scope.
        let t_all = timing_probe();

        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &render_view,
                dispatch_dims.width,
                dispatch_dims.height,
                scene_ref,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;
        let t_render_ms = probe_ms(t_all);

        self.dump_stage("render_view", &seed_texture, dispatch_dims);

        // Harvest extent: the true region size (dispatch may be quantized
        // larger; the padding is never harvested).
        // Texels, not scene pixels: everything that copies out of the textures
        // just rendered reads this, and at a reduced raster they are smaller.
        self.last_effect_dims = match region {
            Some(r) => SceneDimensions {
                width: scaled_texels(r.size.width, s),
                height: scaled_texels(r.size.height, s),
            },
            None => SceneDimensions {
                width: scaled_texels(dimensions.width, s),
                height: scaled_texels(dimensions.height, s),
            },
        };
        self.last_region = region;
        self.last_region_pp = None;

        if chain.is_empty() {
            self.last_filtered_source = FilteredSource::Render;
            if region.is_some() {
                // The unfiltered result lives in the region render target.
                self.last_region_pp = Some((seed_texture.clone(), render_view.clone()));
            }
            return Ok((render_view, None));
        }

        let seed = ChainSeed {
            seed_texture: seed_texture.clone(),
            pp_a: pp_a.clone(),
            pp_a_view: pp_a_view.clone(),
            pp_b: pp_b.clone(),
            pp_b_view: pp_b_view.clone(),
            pp_original: pp_original.clone(),
            pp_original_view: pp_original_view.clone(),
        };
        // "Region scoped" means "the result is in the scratch we were handed",
        // which a reduced raster also is — the full-frame textures are not what
        // the chain just wrote.
        let (view, composite) = self.run_chain_passes_fused(
            seed,
            dispatch_dims,
            chain,
            scratch.is_some(),
            harvest_req,
        )?;
        if t_all.is_some() {
            eprintln!(
                "[filter-timing] dims={}x{} stages={} render={:.2}ms seed+passes={:.2}ms",
                dispatch_dims.width,
                dispatch_dims.height,
                chain.instances.len(),
                t_render_ms,
                probe_ms(t_all) - t_render_ms,
            );
        }
        Ok((view, composite))
    }

    /// Copy the rendered seed into the ping-pong and run every chain pass —
    /// all in ONE command encoder and ONE submit. Same-queue submissions are
    /// ordered, and each pass reads its own context through a dynamic offset,
    /// so the batch is correct and the chain costs one submit instead of one
    /// per stage.
    pub(super) fn run_chain_passes_fused(
        &mut self,
        seed: ChainSeed,
        dispatch_dims: SceneDimensions,
        chain: &EffectChain,
        region_scoped: bool,
        harvest_req: Option<PendingHarvestReq>,
    ) -> Result<(wgpu::TextureView, Option<PendingComposite>), String> {
        let ChainSeed {
            seed_texture,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        } = seed;
        let width = dispatch_dims.width.max(1);
        let height = dispatch_dims.height.max(1);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Effect Chain Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &pp_a,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.dump_stage("tex_a_seed", &pp_a, dispatch_dims);
        // The original is the *pre-chain* frame: copied here, before pass 0
        // runs, so a bloom-style pass can add onto what the scene looked like
        // rather than onto whatever the previous pass left behind.
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &pp_original,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // Plan the passes, write every context once (one slot per pass), then
        // encode seed + passes into a single encoder.
        // Owns cloned wgpu handles so plan entries never borrow `self` across
        // the mutable pipeline-ensure calls.
        let mut pass_plan: Vec<(wgpu::ComputePipeline, wgpu::Buffer, u32, u32)> = Vec::new();
        let mut current = FilteredSource::TexA;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let Some(effect) = effect(&instance.id) else {
                tracing::warn!("chain effect has no registered descriptor; skipping");
                continue;
            };
            self.ensure_effect_pipeline(effect.as_effect());
            let Some(pipeline) = self.pipelines.get(effect.type_name()) else {
                continue;
            };

            let mut uniforms = vec![0u8; effect.author_uniform_size() as usize];
            effect.pack(&instance.params, &mut uniforms);
            // A scene-pixel parameter has to shrink with the raster or the
            // effect visually grows as the frame gets smaller: half the pixels
            // with the same radius is a twice-as-wide blur. Done generically off
            // the declared unit rather than per effect, so a new pixel parameter
            // needs only `.pixel()` on its spec to be covered.
            if self.raster_scale != 1.0 {
                scale_pixel_params(effect.params(), self.raster_scale, &mut uniforms);
            }
            // Author-parameter writes land before the encoder's commands on
            // submit; each effect has its own uniform buffer, so writes cannot
            // collide across effects.
            self.queue.write_buffer(&pipeline.uniform_buffer, 0, &uniforms);

            let pass_count = pipeline.passes.len() as u32;
            for (index, pass_pipeline) in pipeline.passes.iter().enumerate() {
                pass_plan.push((
                    pass_pipeline.clone(),
                    pipeline.uniform_buffer.clone(),
                    index as u32,
                    pass_count,
                ));
            }
        }

        let mut context_bytes = vec![0u8; (CONTEXT_STRIDE * MAX_CHAIN_PASSES) as usize];
        for (slot, (pass_pipeline, uniform_buffer, pass_index, pass_count)) in
            pass_plan.iter().enumerate()
        {
            let context = EffectContextUniform {
                tex_size: [width, height],
                _pad0: [0, 0],
                inv_size: [1.0 / width as f32, 1.0 / height as f32],
                _pad1: [0.0, 0.0],
                pass_index: *pass_index,
                pass_count: *pass_count,
                time_ms: chain.time_ms,
                _pad2: 0.0,
            };
            let base = (slot as u32 * CONTEXT_STRIDE) as usize;
            context_bytes[base..base + EFFECT_CONTEXT_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&context));
            let bind_group = self.effect_bind_group_for_slot(
                if matches!(current, FilteredSource::TexA) {
                    &pp_a_view
                } else {
                    &pp_b_view
                },
                if matches!(current, FilteredSource::TexA) {
                    &pp_b_view
                } else {
                    &pp_a_view
                },
                &pp_original_view,
                uniform_buffer,
            );
            Self::encode_effect_pass(
                &mut encoder,
                &bind_group,
                pass_pipeline,
                width,
                height,
                slot as u32 * CONTEXT_STRIDE,
            );
            current = match current {
                FilteredSource::TexA => FilteredSource::TexB,
                _ => FilteredSource::TexA,
            };
        }
        let (result_texture, result_view) = match current {
            FilteredSource::TexA => (pp_a, pp_a_view),
            FilteredSource::TexB => (pp_b, pp_b_view),
            FilteredSource::Render => unreachable!("effects always write a ping-pong texture"),
        };
        let pending_composite = harvest_req.map(|req| {
            self.copy_texture_to_pending_composite(
                &mut encoder,
                &result_texture,
                req.source_origin,
                req.dimensions,
                req.alpha,
                req.origin,
            )
        });
        self.queue.write_buffer(&self.context_buffer, 0, &context_bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        self.dump_stage("tex_b_fx", &result_texture, dispatch_dims);

        self.last_filtered_source = current;
        if region_scoped {
            // Region results live in the region scratch ping-pong, covering
            // `last_effect_dims` from (0, 0).
            self.last_region_pp = Some((result_texture, result_view.clone()));
        }
        Ok((result_view, pending_composite))
    }
}
