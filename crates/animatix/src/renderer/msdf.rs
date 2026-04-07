use super::text::ExtractedGlyph;
use std::collections::HashMap;

pub struct FontAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub uv_map: HashMap<(String, u16), [f32; 4]>,
}

impl FontAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, glyphs: &[ExtractedGlyph]) -> Self {
        let mut unique_glyphs = HashMap::new();
        for g in glyphs {
            let key = (g.font.info().family.clone(), g.glyph_id);
            if !unique_glyphs.contains_key(&key) {
                unique_glyphs.insert(key, g.font.clone());
            }
        }

        let raster_size = 128.0;
        let cell_size = 256;
        let half_cell = cell_size / 2;
        let atlas_width = 4096;
        let atlas_height = 4096;

        let mut uv_map = HashMap::new();
        let mut tex_data = vec![0u8; (atlas_width * atlas_height * 4) as usize];

        let mut font_cache: HashMap<String, fontdue::Font> = HashMap::new();

        let mut current_cell_x = 0;
        let mut current_cell_y = 0;

        for (key, font) in unique_glyphs {
            let font_name = key.0.clone();
            let glyph_id = key.1;

            if !font_cache.contains_key(&font_name) {
                let bytes = font.data().as_slice();
                let f = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).unwrap();
                font_cache.insert(font_name.clone(), f);
            }

            let f = font_cache.get(&font_name).unwrap();
            let (metrics, bitmap) = f.rasterize_indexed(glyph_id, raster_size);

            if !bitmap.is_empty() && metrics.width > 0 && metrics.height > 0 {
                let w = metrics.width as i32;
                let h = metrics.height as i32;
                let xmin = metrics.xmin;
                let ymin = metrics.ymin;

                let px_start = half_cell + xmin;
                let py_start = half_cell - (ymin + h);

                for y in 0..h {
                    for x in 0..w {
                        let dst_x = current_cell_x + px_start + x;
                        let dst_y = current_cell_y + py_start + y;

                        if dst_x >= 0
                            && dst_x < atlas_width as i32
                            && dst_y >= 0
                            && dst_y < atlas_height as i32
                        {
                            let alpha = bitmap[(y * w + x) as usize];
                            let atlas_idx = ((dst_y * atlas_width as i32 + dst_x) * 4) as usize;
                            tex_data[atlas_idx] = alpha;
                            tex_data[atlas_idx + 1] = alpha;
                            tex_data[atlas_idx + 2] = alpha;
                            tex_data[atlas_idx + 3] = alpha;
                        }
                    }
                }
            }

            let u0 = current_cell_x as f32 / atlas_width as f32;
            let v0 = current_cell_y as f32 / atlas_height as f32;
            let u1 = (current_cell_x + cell_size) as f32 / atlas_width as f32;
            let v1 = (current_cell_y + cell_size) as f32 / atlas_height as f32;

            uv_map.insert(key, [u0, v0, u1 - u0, v1 - v0]);

            current_cell_x += cell_size;
            if current_cell_x + cell_size > atlas_width as i32 {
                current_cell_x = 0;
                current_cell_y += cell_size;
            }
        }

        let size = wgpu::Extent3d {
            width: atlas_width,
            height: atlas_height,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Atlas Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tex_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas_width * 4),
                rows_per_image: Some(atlas_height),
            },
            size,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Font Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Font Atlas Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font Atlas Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        Self {
            texture,
            view,
            sampler,
            bind_group_layout,
            bind_group,
            uv_map,
        }
    }

    pub fn get_uv_rect(&self, glyph: &ExtractedGlyph) -> [f32; 4] {
        let key = (glyph.font.info().family.clone(), glyph.glyph_id);
        self.uv_map
            .get(&key)
            .copied()
            .unwrap_or([0.0, 0.0, 1.0, 1.0])
    }
}
