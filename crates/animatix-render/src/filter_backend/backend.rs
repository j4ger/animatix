use super::*;

impl GpuFilterBackend {
    /// TEMPORARY stage dump for debugging the multi-scope video loss.
    pub(super) fn dump_stage(&self, label: &str, texture: &wgpu::Texture, dims: SceneDimensions) {
        if std::env::var_os("ANIMATIX_DUMP_STAGES").is_none() {
            return;
        }
        let img = self
            .readback_to_scene_image_at(texture, wgpu::Origin3d::ZERO, dims)
            .expect("stage dump readback");
        let data = img.data.data.data();
        let n = data.len() / 4;
        let step = (n / 300).max(1);
        let (mut opaque, mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u64, 0u64, 0u64);
        for i in (0..n).step_by(step) {
            let (r, g, b, a) = (
                data[i * 4] as u32,
                data[i * 4 + 1] as u32,
                data[i * 4 + 2] as u32,
                data[i * 4 + 3] as u32,
            );
            if a > 100 {
                opaque += 1;
                sum_r += r as u64;
                sum_g += g as u64;
                sum_b += b as u64;
            }
        }
        let total = (n as u32).div_ceil(step as u32);
        let _ = std::fs::create_dir_all("/tmp/stages");
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/stages/{seq:03}_{label}.png");
        #[cfg(feature = "raster-encode")]
        let _ = image::save_buffer(&path, data, dims.width, dims.height, image::ColorType::Rgba8);
        #[cfg(not(feature = "raster-encode"))]
        let _ = (&path, data);
        eprintln!(
            "[stages] {seq:03} {label}: opaque={opaque}/{total} avg_rgb=({},{},{})",
            if opaque > 0 { sum_r / opaque as u64 } else { 0 },
            if opaque > 0 { sum_g / opaque as u64 } else { 0 },
            if opaque > 0 { sum_b / opaque as u64 } else { 0 },
        );
    }

    /// Create a new backend from a cloned device/queue pair.
    ///
    /// # Errors
    /// Returns an error if the inner [`RendererCore`] fails to initialise.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        dimensions: SceneDimensions,
    ) -> Result<Self, String> {
        let core = RendererCore::new(&device, &queue)
            .map_err(|e| format!("Failed to create filter renderer core: {e}"))?;
        let bytes_per_row = (dimensions.width * 4 + 255) & !255;

        let texture_size = wgpu::Extent3d {
            width: dimensions.width,
            height: dimensions.height,
            depth_or_array_layers: 1,
        };

        // Render target: needs RENDER_ATTACHMENT for Vello + STORAGE_BINDING for compute
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some("Animatix Filter Render Texture"),
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let ping_pong_usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let tex_a = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter PingPong A"),
            view_formats: &[],
        });
        let tex_a_view = tex_a.create_view(&wgpu::TextureViewDescriptor::default());

        let tex_b = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter PingPong B"),
            view_formats: &[],
        });
        let tex_b_view = tex_b.create_view(&wgpu::TextureViewDescriptor::default());

        let tex_original = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter Original"),
            view_formats: &[],
        });
        let tex_original_view = tex_original.create_view(&wgpu::TextureViewDescriptor::default());

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: (bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Filter Output Buffer"),
            mapped_at_creation: false,
        });

        // Fixed layout shared by every effect (docs/effects.md §4.2).
        let bind_group_layout = create_bind_group_layout(&device);
        let pipeline_layout = create_pipeline_layout(&device, &bind_group_layout);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Effect Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let context_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Effect Context Uniforms"),
            size: u64::from(CONTEXT_STRIDE * MAX_CHAIN_PASSES),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            core,
            render_texture,
            render_view,
            tex_a,
            tex_a_view,
            tex_b,
            tex_b_view,
            tex_original,
            tex_original_view,
            output_buffer,
            bytes_per_row,
            _dimensions: dimensions,
            raster_scale: 1.0,
            bind_group_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            sampler,
            context_buffer,
            last_filtered_source: FilteredSource::Render,
            last_effect_dims: dimensions,
            last_region: None,
            pending_composites: Vec::new(),
            region_scratch: HashMap::new(),
            last_region_pp: None,
            pending_texture_pool: RefCell::new(HashMap::new()),
        })
    }

    /// Build (once) the pipelines and uniform buffer for `effect`.
    pub(super) fn ensure_effect_pipeline(&mut self, effect: &dyn Effect) {
        if self.pipelines.contains_key(effect.type_name()) {
            return;
        }
        let pipeline = create_effect_pipeline(&self.device, &self.pipeline_layout, effect);
        self.pipelines.insert(effect.type_name().into(), pipeline);
    }

    /// Bind group for one pass: views plus the context uniform at `slot`s
    /// dynamic offset in the shared context buffer.
    pub(super) fn effect_bind_group_for_slot(
        &self,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        original_view: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        create_effect_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.context_buffer,
            &self.sampler,
            src_view,
            dst_view,
            original_view,
            uniform_buffer,
        )
    }

    /// Encode one effect pass into `encoder` (dispatch at `width`×`height`,
    /// context uniform at the bind group's dynamic offset).
    pub(super) fn encode_effect_pass(
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        pass_pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        slot_offset: u32,
    ) {
        pipeline::encode_effect_pass(
            encoder,
            bind_group,
            pass_pipeline,
            width,
            height,
            slot_offset,
        );
    }
}
