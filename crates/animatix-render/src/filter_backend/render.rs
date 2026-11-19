use super::readback::*;
use super::*;

struct PlannedEffectInstance {
    effect_type: Box<str>,
    uniforms: Vec<u8>,
    pass_count: u32,
}

struct PlannedEffectPass {
    pipeline: wgpu::ComputePipeline,
    uniform_buffer: wgpu::Buffer,
    author_uniform_size: u64,
    author_uniform_offset: u32,
    pass_index: u32,
    pass_count: u32,
}

fn encode_seed_copy(
    encoder: &mut wgpu::CommandEncoder,
    seed_texture: &wgpu::Texture,
    pp_a: &wgpu::Texture,
    pp_original: &wgpu::Texture,
    width: u32,
    height: u32,
) {
    for target in [pp_a, pp_original] {
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
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
    }
}

impl GpuFilterBackend {
    pub(super) fn render_and_filter_scene_to_view(
        &mut self,
        scene: &animatix::scene::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<wgpu::TextureView, String> {
        self.render_and_filter_scene(scene, dimensions, region, chain, None)
            .map(|(v, _)| v)
    }

    pub(super) fn render_and_filter_scene(
        &mut self,
        scene: &animatix::scene::Scene,
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
                let (width, height) =
                    crate::filter_region::quantize_region_size(r.size, dimensions);
                let (width, height) = (scaled_texels(width, s), scaled_texels(height, s));
                wrapped = animatix::scene::Scene::new();
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
                wrapped = animatix::scene::Scene::new();
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
            .render_scene_with_background(
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
        let (view, composite) =
            self.run_chain_passes(seed, dispatch_dims, chain, scratch.is_some(), harvest_req)?;
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

    /// Pack active instances, grow persistent uniform storage as needed, and
    /// prepare one execution entry per declared pass.
    fn plan_effect_passes(
        &mut self,
        chain: &EffectChain,
    ) -> Result<Vec<PlannedEffectPass>, String> {
        let mut instance_plan = Vec::new();
        let mut uniform_slots_by_type: HashMap<Box<str>, usize> = HashMap::new();
        let mut total_pass_count = 0usize;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let Some(effect) = effect(&instance.id) else {
                tracing::warn!("chain effect has no registered descriptor; skipping");
                continue;
            };
            self.ensure_effect_pipeline(effect.as_effect());
            let Some(pipeline) = self.pipelines.get(effect.type_name()) else {
                continue;
            };
            let pass_count = pipeline.passes.len() as u32;
            total_pass_count += pass_count as usize;
            if pass_count != 0 {
                *uniform_slots_by_type.entry(effect.type_name().into()).or_default() += 1;
            }

            let mut uniforms = vec![0u8; effect.author_uniform_size() as usize];
            effect.pack(&instance.params, &mut uniforms);
            // Scene-pixel parameters shrink with the raster so the effect
            // keeps the same apparent size when the frame is rendered smaller.
            if self.raster_scale != 1.0 {
                scale_pixel_params(effect.params(), self.raster_scale, &mut uniforms);
            }
            instance_plan.push(PlannedEffectInstance {
                effect_type: effect.type_name().into(),
                uniforms,
                pass_count,
            });
        }

        // Reject oversized plans before writing author/context uniforms or
        // submitting any work.
        validate_pass_count(total_pass_count)?;

        for (effect_type, slots) in &uniform_slots_by_type {
            if let Some(pipeline) = self.pipelines.get_mut(effect_type.as_ref())
                && pipeline.ensure_author_uniform_capacity(&self.device, *slots)?
            {
                self.effect_bind_group_cache.clear();
            }
        }

        let mut next_uniform_slot_by_type: HashMap<Box<str>, usize> =
            HashMap::with_capacity(uniform_slots_by_type.len());
        let mut pass_plan = Vec::with_capacity(total_pass_count);
        for instance in instance_plan {
            if instance.pass_count == 0 {
                continue;
            }
            let slot = next_uniform_slot_by_type.entry(instance.effect_type.clone()).or_default();
            let pipeline = self
                .pipelines
                .get(instance.effect_type.as_ref())
                .expect("effect pipeline was ensured while planning the chain");
            let author_uniform_offset = pipeline.author_uniform_offset(*slot)?;
            *slot += 1;

            // Every instance gets a separate aligned slot in the persistent
            // per-type buffer, so these queue writes cannot overwrite each
            // other's parameters before the passes submit.
            self.queue.write_buffer(
                &pipeline.uniform_buffer,
                u64::from(author_uniform_offset),
                &instance.uniforms,
            );

            let pass_count = pipeline.passes.len() as u32;
            for (index, pass_pipeline) in pipeline.passes.iter().enumerate() {
                pass_plan.push(PlannedEffectPass {
                    pipeline: pass_pipeline.clone(),
                    uniform_buffer: pipeline.uniform_buffer.clone(),
                    author_uniform_size: pipeline.author_uniform_size,
                    author_uniform_offset,
                    pass_index: index as u32,
                    pass_count,
                });
            }
        }
        Ok(pass_plan)
    }

    fn write_effect_contexts(
        &self,
        pass_plan: &[PlannedEffectPass],
        width: u32,
        height: u32,
        time_ms: f32,
    ) {
        if pass_plan.is_empty() {
            return;
        }
        let mut context_bytes = vec![0u8; (CONTEXT_STRIDE as usize) * pass_plan.len()];
        for (slot, pass) in pass_plan.iter().enumerate() {
            let context = EffectContextUniform {
                tex_size: [width, height],
                _pad0: [0, 0],
                inv_size: [1.0 / width as f32, 1.0 / height as f32],
                _pad1: [0.0, 0.0],
                pass_index: pass.pass_index,
                pass_count: pass.pass_count,
                time_ms,
                _pad2: 0.0,
            };
            let base = slot * CONTEXT_STRIDE as usize;
            context_bytes[base..base + EFFECT_CONTEXT_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&context));
        }
        self.queue.write_buffer(&self.context_buffer, 0, &context_bytes);
    }

    fn encode_effect_passes(
        &mut self,
        pass_plan: &[PlannedEffectPass],
        pp_a_view: &wgpu::TextureView,
        pp_b_view: &wgpu::TextureView,
        pp_original_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        strategy: PassSubmitStrategy,
        mut combined_encoder: Option<&mut wgpu::CommandEncoder>,
    ) -> FilteredSource {
        let mut current = FilteredSource::TexA;
        for (slot, pass) in pass_plan.iter().enumerate() {
            let reading_a = matches!(current, FilteredSource::TexA);
            let (src_view, dst_view) = if reading_a {
                (pp_a_view, pp_b_view)
            } else {
                (pp_b_view, pp_a_view)
            };
            let cache_key = EffectBindGroupCacheKey {
                src_view: src_view.clone(),
                dst_view: dst_view.clone(),
                original_view: pp_original_view.clone(),
                uniform_buffer: pass.uniform_buffer.clone(),
                author_uniform_size: pass.author_uniform_size,
            };
            if !self.effect_bind_group_cache.contains_key(&cache_key) {
                if self.effect_bind_group_cache.len() >= MAX_EFFECT_BIND_GROUP_CACHE_ENTRIES {
                    self.effect_bind_group_cache.clear();
                }
                let bind_group = self.effect_bind_group_for_slot(
                    src_view,
                    dst_view,
                    pp_original_view,
                    &pass.uniform_buffer,
                    pass.author_uniform_size,
                );
                self.effect_bind_group_cache.insert(cache_key.clone(), bind_group);
            }
            let bind_group = self
                .effect_bind_group_cache
                .get(&cache_key)
                .expect("effect bind group was cached before dispatch");
            let context_offset = slot as u32 * CONTEXT_STRIDE;
            match strategy {
                PassSubmitStrategy::Separate => {
                    let mut encoder =
                        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Animatix Effect Pass Encoder"),
                        });
                    Self::encode_effect_pass(
                        &mut encoder,
                        bind_group,
                        &pass.pipeline,
                        width,
                        height,
                        pass.author_uniform_offset,
                        context_offset,
                    );
                    self.queue.submit(std::iter::once(encoder.finish()));
                },
                PassSubmitStrategy::Combined => {
                    let encoder = combined_encoder
                        .as_deref_mut()
                        .expect("combined strategy owns one encoder for the whole chain");
                    Self::encode_effect_pass(
                        encoder,
                        bind_group,
                        &pass.pipeline,
                        width,
                        height,
                        pass.author_uniform_offset,
                        context_offset,
                    );
                },
            }
            current = match current {
                FilteredSource::TexA => FilteredSource::TexB,
                _ => FilteredSource::TexA,
            };
        }
        current
    }

    fn submit_strategy(&self) -> PassSubmitStrategy {
        // Stage snapshots synchronously read the seed before later passes can
        // overwrite it, so diagnostic dumps retain the eager submit path.
        if std::env::var_os("ANIMATIX_DUMP_STAGES").is_some() {
            return PassSubmitStrategy::Separate;
        }
        #[cfg(test)]
        {
            self.test_submit_strategy
        }
        #[cfg(not(test))]
        {
            PassSubmitStrategy::Combined
        }
    }

    /// Copy the rendered seed into the ping-pong and run every chain pass.
    ///
    /// Seed copies, independent compute passes, and the optional harvest copy
    /// share one encoder and submission. Per-instance author slots and per-pass
    /// context slots preserve parameters across all dispatches. Pixel A/B tests
    /// compare this path with separate submissions on software and hardware
    /// Vulkan; see `docs/effects.md` §4.6. Stage dumps retain separate submits
    /// so each intermediate image can be read before it is overwritten.
    pub(super) fn run_chain_passes(
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
        let pass_plan = self.plan_effect_passes(chain)?;
        let strategy = self.submit_strategy();
        self.write_effect_contexts(&pass_plan, width, height, chain.time_ms);

        let mut combined_encoder = if strategy == PassSubmitStrategy::Combined {
            Some(self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Combined Effect Chain Encoder"),
            }))
        } else {
            None
        };

        // Seed copy: start ping-pong A and the pre-chain original from the
        // rendered sub-scene. The original is the *pre-chain* frame, so a
        // bloom-style pass can add onto what the scene looked like rather than
        // onto whatever the previous pass left behind.
        match strategy {
            PassSubmitStrategy::Separate => {
                let mut seed_encoder =
                    self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Animatix Effect Seed Encoder"),
                    });
                encode_seed_copy(
                    &mut seed_encoder,
                    &seed_texture,
                    &pp_a,
                    &pp_original,
                    width,
                    height,
                );
                self.queue.submit(std::iter::once(seed_encoder.finish()));
                self.dump_stage("tex_a_seed", &pp_a, dispatch_dims);
            },
            PassSubmitStrategy::Combined => {
                encode_seed_copy(
                    combined_encoder
                        .as_mut()
                        .expect("combined strategy owns one encoder for the whole chain"),
                    &seed_texture,
                    &pp_a,
                    &pp_original,
                    width,
                    height,
                );
            },
        }

        let current = self.encode_effect_passes(
            &pass_plan,
            &pp_a_view,
            &pp_b_view,
            &pp_original_view,
            width,
            height,
            strategy,
            combined_encoder.as_mut(),
        );
        let (result_texture, result_view) = match current {
            FilteredSource::TexA => (pp_a, pp_a_view),
            FilteredSource::TexB => (pp_b, pp_b_view),
            FilteredSource::Render => unreachable!("effects always write a ping-pong texture"),
        };

        // Separate mode keeps harvest in its own submit; combined mode
        // appends it after the last pass in the chain encoder.
        let pending_composite = if let Some(req) = harvest_req {
            let composite = match strategy {
                PassSubmitStrategy::Separate => {
                    let mut encoder =
                        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Animatix Effect Harvest Encoder"),
                        });
                    let composite = self.copy_texture_to_pending_composite(
                        &mut encoder,
                        &result_texture,
                        req.source_origin,
                        req.dimensions,
                        req.alpha,
                        req.origin,
                    );
                    self.queue.submit(std::iter::once(encoder.finish()));
                    composite
                },
                PassSubmitStrategy::Combined => self.copy_texture_to_pending_composite(
                    combined_encoder
                        .as_mut()
                        .expect("combined strategy owns one encoder for the whole chain"),
                    &result_texture,
                    req.source_origin,
                    req.dimensions,
                    req.alpha,
                    req.origin,
                ),
            };
            Some(composite)
        } else {
            None
        };

        if let Some(encoder) = combined_encoder {
            self.queue.submit(std::iter::once(encoder.finish()));
        }

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
