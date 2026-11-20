use animatix::timeline::effects::{
    EFFECTS, EffectChain, EffectInstance, EffectParamKind, EffectParamSpec, EffectPassSpec,
    PluginEffectData, register_extension_effect, unregister_extension_effect,
};
use animatix_core::effect::{EffectParamValue, EffectParams};

use super::helpers::ALPHA_CONVERSION_WGSL;
use super::{EffectsExtent, EffectsGpu, FORMAT, FRAGMENT_ENTRY, FULLSCREEN_VERTEX};

#[test]
fn all_builtin_fragment_shaders_validate_and_expose_fullscreen_entries() {
    assert_eq!(EFFECTS.len(), 14);
    for effect in EFFECTS {
        let shader = effect
            .fragment_shader()
            .unwrap_or_else(|| panic!("{} has no fragment shader", effect.type_name()));
        let module = naga::front::wgsl::parse_str(shader)
            .unwrap_or_else(|error| panic!("{} WGSL parse failed: {error}", effect.type_name()));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{} WGSL validation failed: {error}", effect.type_name()));
        assert!(module.entry_points.iter().any(|entry| {
            entry.name == FULLSCREEN_VERTEX && entry.stage == naga::ShaderStage::Vertex
        }));
        assert!(module.entry_points.iter().any(|entry| {
            entry.name == FRAGMENT_ENTRY && entry.stage == naga::ShaderStage::Fragment
        }));
    }

    let alpha_module = naga::front::wgsl::parse_str(ALPHA_CONVERSION_WGSL)
        .expect("alpha conversion shader must parse");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&alpha_module)
    .expect("alpha conversion shader must validate");
}

#[test]
fn all_builtin_fragment_pipelines_execute_on_a_headless_device() {
    let Some((device, queue)) = pollster::block_on(crate::test_support::create_headless_device(
        "Animatix Portable Effect Test Device",
    )) else {
        return;
    };
    let width = 8;
    let height = 8;
    let input = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix Portable Effect Test Input"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                let alpha = if (x + y) % 3 == 0 { 128u8 } else { 255u8 };
                let alpha = u16::from(alpha);
                [
                    ((x * 30 * u32::from(alpha) / 255) as u8),
                    ((y * 30 * u32::from(alpha) / 255) as u8),
                    (((x + y) * 15 * u32::from(alpha) / 255) as u8),
                    alpha as u8,
                ]
            })
        })
        .collect();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &input,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());
    let mut executor = EffectsGpu::new(device.clone(), queue);
    for descriptor in EFFECTS {
        let mut params = EffectParams {
            values: descriptor.params().iter().map(|spec| spec.identity).collect(),
        };
        if let Some(first) = descriptor.params().first() {
            params.values[0] = match first.identity {
                EffectParamValue::F32(value) => {
                    EffectParamValue::F32(if value == 0.0 { 0.5 } else { value + 0.25 })
                },
                EffectParamValue::U32(value) => EffectParamValue::U32(value.saturating_add(1)),
                EffectParamValue::Vec2(value) => {
                    EffectParamValue::Vec2([value[0] + 1.0, value[1] - 1.0])
                },
                EffectParamValue::Vec4(value) => {
                    EffectParamValue::Vec4([value[0] + 0.25, value[1], value[2], value[3]])
                },
                EffectParamValue::Bool(value) => EffectParamValue::Bool(!value),
            };
        }
        let chain = EffectChain {
            instances: vec![EffectInstance {
                id: animatix_core::effect::EffectId::new(descriptor.type_name()),
                enabled: true,
                params,
            }],
            time_ms: 250.0,
        };
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let output = executor
            .apply(
                &input_view,
                &chain,
                EffectsExtent {
                    width,
                    height,
                    origin: [0.0, 0.0],
                },
                1.0,
            )
            .unwrap_or_else(|error| panic!("{} apply failed: {error}", descriptor.type_name()));
        let error = pollster::block_on(validation_scope.pop());
        assert!(
            error.is_none(),
            "{} render pipeline validation failed: {error:?}",
            descriptor.type_name()
        );
        assert_eq!(output.view.texture().width(), width);
        assert_eq!(output.view.texture().height(), height);
        assert!(
            output.view.texture().usage().contains(wgpu::TextureUsages::COPY_SRC),
            "{} output must support the shared frame copy",
            descriptor.type_name()
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_compute_plugin_runs_the_demo_pixelate_shader() {
    const DEMO_SOURCE: &str = include_str!("../../../animatix-plugin-demo/src/lib.rs");
    const EFFECT_NAME: &str = "PortableComputePixelateCheck";
    let marker = "const PIXELATE_WGSL: &str = r#\"";
    let start = DEMO_SOURCE.find(marker).expect("demo plugin keeps its Pixelate WGSL constant")
        + marker.len();
    let remaining = &DEMO_SOURCE[start..];
    let end = remaining.find("\"#;").expect("demo Pixelate WGSL raw string is terminated");
    let pixelate_wgsl = &remaining[..end];

    let Some((device, queue)) = pollster::block_on(crate::test_support::create_headless_device(
        "Animatix Native Plugin Effect Test Device",
    )) else {
        return;
    };
    let data = PluginEffectData {
        type_name: EFFECT_NAME.into(),
        display_name: "Pixelate check".into(),
        params: vec![EffectParamSpec::new(
            "size",
            EffectParamKind::F32,
            EffectParamValue::F32(0.0),
            0,
            4,
        )],
        passes: vec![EffectPassSpec::new("Pixelate", pixelate_wgsl, "main")],
        author_uniform_size: 16,
        support_px: 0.0,
    };
    assert_eq!(register_extension_effect(data).as_deref(), Some(EFFECT_NAME));

    let width = 16;
    let height = 16;
    let input = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix Pixelate Check Input"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| [((x * 13) as u8), ((y * 13) as u8), 90, 255]))
        .collect();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &input,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let chain = EffectChain {
        instances: vec![EffectInstance {
            id: animatix_core::effect::EffectId::new(EFFECT_NAME),
            enabled: true,
            params: EffectParams {
                values: vec![EffectParamValue::F32(4.0)],
            },
        }],
        time_ms: 0.0,
    };
    let input_view = input.create_view(&wgpu::TextureViewDescriptor::default());
    let mut executor = EffectsGpu::new(device.clone(), queue.clone());
    let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = executor.apply(
        &input_view,
        &chain,
        EffectsExtent {
            width,
            height,
            origin: [0.0, 0.0],
        },
        1.0,
    );
    let error = pollster::block_on(validation_scope.pop());
    let output = result.expect("native compute-only effect should execute");
    assert!(error.is_none(), "Pixelate pipeline validation failed: {error:?}");
    assert!(output.view.texture().usage().contains(wgpu::TextureUsages::COPY_SRC));
    let output_pixels = readback_rgba(&device, &queue, output.view.texture(), width, height);
    let first = pixel(&output_pixels, width, 0, 0);
    let same_cell = pixel(&output_pixels, width, 3, 3);
    let source = pixel(&pixels, width, 0, 0);
    assert_eq!(first, same_cell, "Pixelate should share color within one cell");
    assert_ne!(first, source, "Pixelate should sample the cell center");

    unregister_extension_effect(EFFECT_NAME);
}

fn readback_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_row = (width * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Animatix effect test readback"),
        size: u64::from(bytes_per_row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Animatix effect test readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).ok();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("effect output readback poll");
    receiver
        .recv()
        .expect("effect output readback callback")
        .expect("effect output readback map");
    let mapped = slice.get_mapped_range().expect("mapped output");
    let mut compact = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height as usize {
        let start = row * bytes_per_row as usize;
        compact.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    compact
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().expect("RGBA pixel")
}
