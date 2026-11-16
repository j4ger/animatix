use super::super::{ChainSeed, FilteredSource, GpuFilterBackend};
use super::{DIMS, blur_chain, effect_chain, gpu_backend};
use animatix::timeline::effects::{
    EffectChain, EffectId, EffectInstance, EffectParamValue, EffectParams,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const CHANNELS: usize = 4;

fn rgba_field(left: [u8; 4], right: [u8; 4], boundary: i32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize * CHANNELS);
    for _y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            pixels.extend_from_slice(if x < boundary { &left } else { &right });
        }
    }
    pixels
}

fn diagonal_rgba_field() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT) as usize * CHANNELS);
    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let pixel = if x - y < 0 {
                [255, 0, 0, 128]
            } else {
                [0, 255, 72, 0]
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    pixels
}

fn upload_seed(backend: &GpuFilterBackend, pixels: &[u8]) -> wgpu::Texture {
    assert_eq!(pixels.len(), (WIDTH * HEIGHT) as usize * CHANNELS);
    let texture = backend.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Straight Alpha Filter Test Seed"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    backend.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * CHANNELS as u32),
            rows_per_image: Some(HEIGHT),
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    texture
}

fn spatial_test_backend() -> Option<GpuFilterBackend> {
    if std::env::var_os("ANIMATIX_SPATIAL_TEST_HARDWARE").is_none() {
        return gpu_backend(DIMS);
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }));
    let Ok(adapter) = adapter else {
        crate::testing::skip_if_no_gpu();
        return None;
    };
    let info = adapter.get_info();
    assert_ne!(
        info.device_type,
        wgpu::DeviceType::Cpu,
        "ANIMATIX_SPATIAL_TEST_HARDWARE requires a non-CPU adapter, got {info:?}"
    );
    tracing::info!(
        target: "animatix_render::filter_backend::straight_alpha_tests",
        adapter = ?info,
        "running spatial alpha GPU tests on hardware"
    );
    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let device_request = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Animatix Spatial Alpha Hardware Test Device"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        memory_hints: Default::default(),
        ..Default::default()
    }));
    let Ok((device, queue)) = device_request else {
        crate::testing::skip_if_no_gpu();
        return None;
    };
    Some(
        GpuFilterBackend::new(device, queue, DIMS)
            .expect("GpuFilterBackend should initialize on the requested hardware adapter"),
    )
}

fn run_chain_pixels(backend: &mut GpuFilterBackend, pixels: &[u8], chain: &EffectChain) -> Vec<u8> {
    let seed_texture = upload_seed(backend, pixels);
    let seed = ChainSeed {
        seed_texture,
        pp_a: backend.tex_a.clone(),
        pp_a_view: backend.tex_a_view.clone(),
        pp_b: backend.tex_b.clone(),
        pp_b_view: backend.tex_b_view.clone(),
        pp_original: backend.tex_original.clone(),
        pp_original_view: backend.tex_original_view.clone(),
    };
    backend
        .run_chain_passes(seed, DIMS, chain, false, None)
        .expect("effect chain should dispatch");
    let result = match backend.last_filtered_source {
        FilteredSource::TexA => &backend.tex_a,
        FilteredSource::TexB => &backend.tex_b,
        FilteredSource::Render => panic!("non-empty effect chain must write a ping-pong target"),
    };
    backend
        .debug_readback_texture(result, wgpu::Origin3d::ZERO, DIMS)
        .data
        .data
        .data()
        .to_vec()
}

fn texel(pixels: &[u8], x: u32, y: u32) -> [f32; 4] {
    let offset = ((y * WIDTH + x) as usize) * CHANNELS;
    std::array::from_fn(|channel| f32::from(pixels[offset + channel]) / 255.0)
}

fn premultiplied_texel(pixels: &[u8], x: u32, y: u32) -> [f32; 4] {
    let [r, g, b, a] = texel(pixels, x, y);
    [r * a, g * a, b * a, a]
}

/// Sample at texel-index coordinates with the same clamp-to-edge linear
/// footprint as the effect sampler, premultiplying each source texel before
/// interpolation. This is the CPU reference for straight-alpha neighbourhoods.
fn sample_premultiplied(pixels: &[u8], x: f32, y: f32) -> [f32; 4] {
    let x_floor = x.floor();
    let y_floor = y.floor();
    let fx = x - x_floor;
    let fy = y - y_floor;
    let x0 = x_floor as i32;
    let y0 = y_floor as i32;
    let clamped = |x: i32, y: i32| {
        premultiplied_texel(
            pixels,
            x.clamp(0, WIDTH as i32 - 1) as u32,
            y.clamp(0, HEIGHT as i32 - 1) as u32,
        )
    };
    let top_left = clamped(x0, y0);
    let top_right = clamped(x0 + 1, y0);
    let bottom_left = clamped(x0, y0 + 1);
    let bottom_right = clamped(x0 + 1, y0 + 1);
    std::array::from_fn(|channel| {
        let top = top_left[channel] * (1.0 - fx) + top_right[channel] * fx;
        let bottom = bottom_left[channel] * (1.0 - fx) + bottom_right[channel] * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

fn store_straight_with_alpha(pixel: [f32; 4], output_alpha: f32, output: &mut [u8]) {
    let alpha = output_alpha.clamp(0.0, 1.0);
    let mut straight = [0.0; 4];
    if alpha > 0.0 {
        let filtered_alpha = pixel[3];
        if filtered_alpha > 0.0 {
            straight[0] = (pixel[0] / filtered_alpha).clamp(0.0, 1.0);
            straight[1] = (pixel[1] / filtered_alpha).clamp(0.0, 1.0);
            straight[2] = (pixel[2] / filtered_alpha).clamp(0.0, 1.0);
        }
    }
    straight[3] = alpha;
    for channel in 0..CHANNELS {
        output[channel] = (straight[channel] * 255.0).round() as u8;
    }
}

fn store_straight(pixel: [f32; 4], output: &mut [u8]) {
    store_straight_with_alpha(pixel, pixel[3], output);
}

fn reference_blur(pixels: &[u8], radius: f32) -> Vec<u8> {
    const TAPS: i32 = 17;
    let sigma = radius / 3.0;
    let half_taps = (TAPS - 1) as f32 * 0.5;
    let stride = (2.0 * radius / (TAPS - 1) as f32).max(1.0);
    let mut current = pixels.to_vec();

    for horizontal in [true, false] {
        let mut next = vec![0; current.len()];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let mut sum = [0.0; 4];
                let mut weight_sum = 0.0;
                for tap in 0..TAPS {
                    let offset = (tap as f32 - half_taps) * stride;
                    let sample = if horizontal {
                        sample_premultiplied(&current, x as f32 + offset, y as f32)
                    } else {
                        sample_premultiplied(&current, x as f32, y as f32 + offset)
                    };
                    let weight = (-(offset * offset) / (2.0 * sigma * sigma)).exp();
                    for channel in 0..CHANNELS {
                        sum[channel] += sample[channel] * weight;
                    }
                    weight_sum += weight;
                }
                let total = sum.map(|channel| channel / weight_sum);
                let offset = ((y * WIDTH + x) as usize) * CHANNELS;
                store_straight(total, &mut next[offset..offset + CHANNELS]);
            }
        }
        current = next;
    }
    current
}

fn reference_motion_blur(pixels: &[u8], length: u32, angle_degrees: f32) -> Vec<u8> {
    let taps = length.clamp(0, 64) as i32;
    if taps <= 1 {
        return pixels.to_vec();
    }
    let radians = angle_degrees.to_radians();
    let dir_x = radians.cos();
    let dir_y = -radians.sin();
    let half_taps = taps as f32 * 0.5;
    let mut output = vec![0; pixels.len()];

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut total = [0.0; 4];
            for tap in 0..taps {
                let offset = tap as f32 + 0.5 - half_taps;
                let sample = sample_premultiplied(
                    pixels,
                    x as f32 + dir_x * offset,
                    y as f32 + dir_y * offset,
                );
                for channel in 0..CHANNELS {
                    total[channel] += sample[channel] / taps as f32;
                }
            }
            let center = texel(pixels, x, y);
            let output_alpha = total[3].max(center[3]);
            let offset = ((y * WIDTH + x) as usize) * CHANNELS;
            store_straight_with_alpha(total, output_alpha, &mut output[offset..offset + CHANNELS]);
        }
    }
    output
}

fn reference_bloom(original: &[u8], running: &[u8], intensity: f32, keep: f32) -> Vec<u8> {
    let mut output = vec![0; original.len()];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let base = texel(original, x, y);
            let glow = texel(running, x, y);
            let added_alpha = glow[3] * intensity.clamp(0.0, 4.0);
            let out_alpha = (base[3] + added_alpha).clamp(0.0, 1.0);
            let mut out = [0.0; 4];
            out[3] = out_alpha;
            if out_alpha > 0.0 {
                for channel in 0..3 {
                    out[channel] = ((base[channel] * base[3] * keep.clamp(0.0, 1.0)
                        + glow[channel] * added_alpha)
                        .clamp(0.0, out_alpha))
                        / out_alpha;
                }
            }
            let offset = ((y * WIDTH + x) as usize) * CHANNELS;
            for channel in 0..CHANNELS {
                output[offset + channel] = (out[channel] * 255.0).round() as u8;
            }
        }
    }
    output
}

fn assert_images_near(actual: &[u8], expected: &[u8], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}: image buffer length");
    let mut mismatches = 0usize;
    let mut max_delta = 0;
    let mut first_mismatch = None;
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let delta = actual.abs_diff(*expected);
        if delta > 2 {
            mismatches += 1;
            max_delta = max_delta.max(delta);
            first_mismatch.get_or_insert((index, *actual, *expected));
        }
    }
    assert_eq!(
        mismatches, 0,
        "{label}: {mismatches} bytes exceed tolerance 2, max delta {max_delta}, first mismatch {first_mismatch:?}"
    );
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * WIDTH + x) as usize) * CHANNELS;
    pixels[offset..offset + CHANNELS].try_into().expect("RGBA pixel")
}

fn assert_saturated_red(pixel: [u8; 4], label: &str) {
    assert!(pixel[0] >= 248, "{label}: red should stay saturated, got {pixel:?}");
    assert!(pixel[1] <= 3 && pixel[2] <= 3, "{label}: hidden color leaked, got {pixel:?}");
}

fn bloom_after_blur(radius: f32, intensity: f32) -> EffectChain {
    let mut chain = blur_chain(radius);
    chain.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![
                EffectParamValue::F32(intensity),
                EffectParamValue::F32(1.0),
                EffectParamValue::F32(0.0),
            ],
        },
    });
    chain
}

fn motion_chain(length: f32, angle: f32) -> EffectChain {
    effect_chain("MotionBlur", vec![EffectParamValue::F32(length), EffectParamValue::F32(angle)])
}

#[test]
fn blur_fractional_taps_premultiply_before_interpolation() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let translucent_edge = rgba_field([255, 0, 0, 128], [0, 255, 72, 0], 32);
    let blur_radius = 10.0;
    let blurred = run_chain_pixels(&mut backend, &translucent_edge, &blur_chain(blur_radius));
    let expected_blur = reference_blur(&translucent_edge, blur_radius);
    assert_images_near(&blurred, &expected_blur, "Blur translucent edge analytic reference");
    // Radius 10 gives a 1.25-texel stride. At x=34 the t=-2.5 sample
    // straddles the x=32 edge and must premultiply source texels before
    // interpolation, or transparent green leaks into the red.
    let fractional_edge = pixel(&blurred, 34, 32);
    assert_saturated_red(fractional_edge, "Blur fractional footprint x=34");
    assert!(
        fractional_edge[3] > 0 && fractional_edge[3] < 128,
        "Blur edge should retain fractional coverage, got {fractional_edge:?}"
    );
    let blur_clamped_edge = pixel(&blurred, 0, 32);
    assert_saturated_red(blur_clamped_edge, "Blur clamp-to-edge source");
    assert_eq!(blur_clamped_edge[3], 128, "Blur must clamp at the opaque-side edge");
    assert_eq!(
        pixel(&blurred, 63, 32),
        [0, 0, 0, 0],
        "Blur must clamp hidden transparent RGB to black"
    );
}

#[test]
fn blur_dense_radius_two_keeps_saturated_rgb_at_translucent_edges() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let translucent_edge = rgba_field([255, 0, 0, 128], [0, 255, 72, 0], 32);
    let blurred = run_chain_pixels(&mut backend, &translucent_edge, &blur_chain(2.0));
    let expected = reference_blur(&translucent_edge, 2.0);
    assert_images_near(&blurred, &expected, "Blur radius 2 analytic reference");
    for x in [31, 32] {
        let actual = pixel(&blurred, x, 32);
        assert_saturated_red(actual, &format!("Blur radius 2 edge x={x}"));
        assert!(actual[3] > 0 && actual[3] < 128, "expected partial coverage, got {actual:?}");
    }
}

#[test]
fn motion_blur_axial_taps_preserve_translucent_color_and_max_coverage() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let translucent_edge = rgba_field([255, 0, 0, 128], [0, 255, 72, 0], 32);
    let chain = motion_chain(8.0, 0.0);
    let actual = run_chain_pixels(&mut backend, &translucent_edge, &chain);
    let expected = reference_motion_blur(&translucent_edge, 8, 0.0);
    assert_images_near(&actual, &expected, "MotionBlur axial analytic reference");
    let motion_edge = pixel(&actual, 32, 32);
    assert_saturated_red(motion_edge, "MotionBlur fractional edge");
    assert!(
        motion_edge[3].abs_diff(56) <= 2,
        "three full and one half-coverage taps (3.5/8) should yield alpha 56, got {motion_edge:?}"
    );
    let motion_inside = pixel(&actual, 31, 32);
    assert_saturated_red(motion_inside, "MotionBlur silhouette");
    // The sampled mean alpha here is 72, but the center texel has alpha 128;
    // max-coverage keeps the silhouette while RGB stays straight red.
    assert_eq!(
        motion_inside[3], 128,
        "MotionBlur's max(center alpha, blurred alpha) must preserve the silhouette"
    );
    assert_saturated_red(pixel(&actual, 0, 32), "MotionBlur clamp-to-edge source");
    assert_eq!(
        pixel(&actual, 63, 32),
        [0, 0, 0, 0],
        "MotionBlur must not leak clamped hidden RGB"
    );
}

#[test]
fn motion_blur_diagonal_samples_premultiply_before_bilinear_filtering() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    // A diagonal boundary makes both sample coordinates fractional and catches
    // hidden-colour leakage along a two-axis motion vector.
    let diagonal = diagonal_rgba_field();
    let diagonal_motion = motion_chain(8.0, 45.0);
    let diagonal_actual = run_chain_pixels(&mut backend, &diagonal, &diagonal_motion);
    let diagonal_expected = reference_motion_blur(&diagonal, 8, 45.0);
    assert_images_near(
        &diagonal_actual,
        &diagonal_expected,
        "MotionBlur diagonal hidden-colour analytic reference",
    );
    let diagonal_edge = pixel(&diagonal_actual, 33, 32);
    assert_saturated_red(diagonal_edge, "MotionBlur diagonal fractional edge");
    assert!(
        (0..128).contains(&diagonal_edge[3]),
        "diagonal blur should add partial coverage outside the silhouette, got {diagonal_edge:?}"
    );
}

fn assert_transparent_black(name: &str, actual: &[u8]) {
    assert!(
        actual.iter().all(|channel| *channel == 0),
        "{name} must store transparent black when every neighborhood alpha is zero"
    );
}

#[test]
fn blur_zero_alpha_hidden_color_becomes_transparent_black() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let transparent_hidden = [17, 214, 93, 0].repeat((WIDTH * HEIGHT) as usize);
    let actual = run_chain_pixels(&mut backend, &transparent_hidden, &blur_chain(10.0));
    assert_transparent_black("Blur", &actual);
}

#[test]
fn motion_blur_zero_alpha_hidden_color_becomes_transparent_black() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let transparent_hidden = [17, 214, 93, 0].repeat((WIDTH * HEIGHT) as usize);
    let actual = run_chain_pixels(&mut backend, &transparent_hidden, &motion_chain(8.0, 0.0));
    assert_transparent_black("MotionBlur", &actual);
}

#[test]
fn opaque_blur_and_motion_blur_remain_ordinary_color_averages() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    // Opaque red/blue neighbors are the control: filtering must still mix the
    // colors according to the same taps while retaining full coverage.
    let opaque_edge = rgba_field([255, 0, 0, 255], [0, 0, 255, 255], 32);
    let opaque_blur = run_chain_pixels(&mut backend, &opaque_edge, &blur_chain(10.0));
    let opaque_blur_expected = reference_blur(&opaque_edge, 10.0);
    assert_images_near(&opaque_blur, &opaque_blur_expected, "opaque Blur control");
    let blur_control_pixel = pixel(&opaque_blur, 31, 32);
    assert_eq!(blur_control_pixel[3], 255, "opaque Blur should preserve full alpha");
    assert!(
        blur_control_pixel[0] > blur_control_pixel[2]
            && blur_control_pixel[0] > 0
            && blur_control_pixel[2] > 0,
        "opaque Blur should mix both colors at the boundary, got {blur_control_pixel:?}"
    );

    let opaque_motion = run_chain_pixels(&mut backend, &opaque_edge, &motion_chain(8.0, 0.0));
    let opaque_motion_expected = reference_motion_blur(&opaque_edge, 8, 0.0);
    assert_images_near(&opaque_motion, &opaque_motion_expected, "opaque MotionBlur control");
    let motion_control_pixel = pixel(&opaque_motion, 32, 32);
    assert_eq!(motion_control_pixel[3], 255, "opaque MotionBlur should preserve full alpha");
    assert!(
        motion_control_pixel[0] > 90 && motion_control_pixel[2] > 100,
        "opaque MotionBlur should mix both colors at the boundary, got {motion_control_pixel:?}"
    );
}

#[test]
fn blur_then_bloom_keeps_the_analytic_translucent_glow() {
    let Some(mut backend) = spatial_test_backend() else {
        return;
    };
    let translucent_edge = rgba_field([255, 0, 0, 128], [0, 255, 72, 0], 32);
    let blur_radius = 10.0;
    let bloom_intensity = 0.5;
    let expected_blur = reference_blur(&translucent_edge, blur_radius);
    let blur_bloom = run_chain_pixels(
        &mut backend,
        &translucent_edge,
        &bloom_after_blur(blur_radius, bloom_intensity),
    );
    let blur_bloom_expected =
        reference_bloom(&translucent_edge, &expected_blur, bloom_intensity, 1.0);
    assert_images_near(&blur_bloom, &blur_bloom_expected, "Blur then Bloom analytic reference");
    let glow_edge = pixel(&blur_bloom, 32, 32);
    assert_saturated_red(glow_edge, "Blur then Bloom edge");
    assert!(
        glow_edge[3].abs_diff(27) <= 2,
        "radius-10 Blur followed by intensity-.5 Bloom should produce about alpha 27 at x=32, got {glow_edge:?}"
    );
}
