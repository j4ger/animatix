use animatix::timeline::effects::{
    EffectId, EffectInstance, EffectParamValue, EffectParams, EffectRegion, PendingLayer,
};
use animatix_core::effect::EffectParamKind;

use super::pipeline::{MAX_CHAIN_PASSES, validate_pass_count};
use super::readback::scale_pixel_params;
use super::*;

use crate::test_support::create_headless_device;

const DIMS: SceneDimensions = SceneDimensions {
    width: 64,
    height: 64,
};

fn pack_radius(radius: f32) -> [u8; 4] {
    let spec =
        EffectParamSpec::new("radius", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4)
            .pixel();
    let mut buf = [0u8; 4];
    let v = radius.to_le_bytes();
    buf.copy_from_slice(&v);
    scale_pixel_params(std::slice::from_ref(&spec), 0.5, &mut buf);
    buf
}

#[test]
fn a_declared_pixel_parameter_shrinks_with_the_raster() {
    assert_eq!(pack_radius(20.0), 10.0f32.to_le_bytes());
}

#[test]
fn a_pass_plan_over_the_context_slot_limit_is_rejected() {
    // 65 entries is one past the 64 context slots: `run_chain_passes` must
    // reject this plan before writing a slot or submitting any GPU work.
    let pass_plan = vec![(); MAX_CHAIN_PASSES as usize + 1];
    let error = validate_pass_count(pass_plan.len()).expect_err("65 passes must be rejected");
    assert!(error.contains("65"), "error should report the requested count: {error}");
    assert!(error.contains("64"), "error should report the slot limit: {error}");
}

#[test]
fn a_pass_plan_at_the_context_slot_limit_is_accepted() {
    let pass_plan = vec![(); MAX_CHAIN_PASSES as usize];
    assert!(validate_pass_count(pass_plan.len()).is_ok());
}

#[test]
fn an_undeclared_parameter_is_left_alone() {
    // Same offset and size, no `.pixel()` — a radius expressed as a
    // fraction of the frame must survive the raster change untouched.
    let spec =
        EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4);
    let mut buf = 20.0f32.to_le_bytes();
    scale_pixel_params(&[spec], 0.5, &mut buf);
    assert_eq!(buf, 20.0f32.to_le_bytes());
}

#[test]
fn every_lane_of_a_vec2_offset_is_scaled() {
    let spec = EffectParamSpec::new(
        "offset",
        EffectParamKind::Vec2,
        EffectParamValue::Vec2([0.0, 0.0]),
        0,
        8,
    )
    .pixel();
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&4.0f32.to_le_bytes());
    buf[4..8].copy_from_slice(&(-8.0f32).to_le_bytes());
    scale_pixel_params(&[spec], 0.25, &mut buf);
    assert_eq!(&buf[0..4], &1.0f32.to_le_bytes());
    assert_eq!(&buf[4..8], &(-2.0f32).to_le_bytes());
}

fn blur_chain(radius: f32) -> EffectChain {
    effect_chain("Blur", vec![EffectParamValue::F32(radius)])
}

fn color_grade_chain(
    brightness: f32,
    contrast: f32,
    saturate: f32,
    hue_rotate: f32,
    sepia: f32,
) -> EffectChain {
    effect_chain(
        "ColorGrade",
        vec![
            EffectParamValue::F32(brightness),
            EffectParamValue::F32(contrast),
            EffectParamValue::F32(saturate),
            EffectParamValue::F32(hue_rotate),
            EffectParamValue::F32(sepia),
        ],
    )
}

fn chromatic_aberration_chain(offset: f32) -> EffectChain {
    effect_chain("ChromaticAberration", vec![EffectParamValue::F32(offset)])
}

/// One-stage chain for any built-in effect, addressed by its authored name.
fn effect_chain(name: &str, values: Vec<EffectParamValue>) -> EffectChain {
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new(name),
            enabled: true,
            params: EffectParams { values },
        }],
        time_ms: 0.0,
    }
}

fn filled_scene(color: vello::peniko::Color, rect: kurbo::Rect) -> vello::Scene {
    let mut scene = vello::Scene::new();
    let path = kurbo::Shape::to_path(&rect, 1e-3);
    scene.fill(vello::peniko::Fill::NonZero, kurbo::Affine::IDENTITY, color, None, &path);
    scene
}

/// Opaque white half-plane for `x < 32`, transparent to the right.
fn white_half_scene(height: f64) -> vello::Scene {
    filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 32.0, height))
}

/// Headless backend at `dims`, or `None` where the suite skips GPU tests.
fn gpu_backend(dims: SceneDimensions) -> Option<GpuFilterBackend> {
    let (device, queue) =
        pollster::block_on(create_headless_device("Animatix Filter Test Device"))?;
    Some(GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise"))
}

/// Backend helper for regression tests that must fail instead of silently
/// passing when GPU execution is required by the environment.
fn required_gpu_backend(dims: SceneDimensions) -> Option<GpuFilterBackend> {
    let backend = gpu_backend(dims);
    if backend.is_none() {
        crate::testing::skip_if_no_gpu();
    }
    backend
}

fn adapter_device(
    force_fallback_adapter: bool,
) -> Option<(wgpu::Device, wgpu::Queue, wgpu::DeviceType)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter,
        apply_limit_buckets: false,
    }))
    .ok()?;
    let adapter_info = adapter.get_info();
    eprintln!(
        "[filter-submit-policy] force_fallback_adapter={force_fallback_adapter} adapter={adapter_info:?}"
    );
    let device_type = adapter_info.device_type;
    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Animatix Filter Submit Policy Test Device"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        memory_hints: Default::default(),
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue, device_type))
}

fn required_adapter_device(
    force_fallback_adapter: bool,
) -> Option<(wgpu::Device, wgpu::Queue, wgpu::DeviceType)> {
    let result = adapter_device(force_fallback_adapter);
    if result.is_none() {
        crate::testing::skip_if_no_gpu();
    }
    result
}

fn policy_backend(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    dims: SceneDimensions,
    strategy: PassSubmitStrategy,
) -> GpuFilterBackend {
    let mut backend = GpuFilterBackend::new(device.clone(), queue.clone(), dims)
        .expect("GpuFilterBackend should initialise");
    backend.test_submit_strategy = strategy;
    backend
}

fn assert_policy_images_equal(label: &str, separate: &SceneImage, combined: &SceneImage) {
    assert_eq!(separate.natural_size, combined.natural_size, "{label}: image dimensions differ");
    let separate = separate.data.data.data();
    let combined = combined.data.data.data();
    assert_eq!(separate.len(), combined.len(), "{label}: pixel buffers differ in length");
    let (mismatched_bytes, max_delta) = separate.iter().zip(combined).fold(
        (0usize, 0u8),
        |(mismatched, max_delta), (left, right)| {
            let delta = left.abs_diff(*right);
            (mismatched + usize::from(delta != 0), max_delta.max(delta))
        },
    );
    assert_eq!(
        mismatched_bytes, 0,
        "{label}: separate/combined full images differ in {mismatched_bytes} bytes (max delta {max_delta})"
    );
}

fn assert_images_differ(label: &str, left: &SceneImage, right: &SceneImage) {
    let left = left.data.data.data();
    let right = right.data.data.data();
    assert_ne!(left, right, "{label}: expected the parameter change to affect pixels");
}

fn scene_pixel(image: &SceneImage, x: usize, y: usize) -> [u8; 4] {
    let width = image.natural_size[0] as usize;
    let start = (y * width + x) * 4;
    image.data.data.data()[start..start + 4].try_into().expect("RGBA pixel")
}

fn checker_cross_scene(dims: SceneDimensions) -> vello::Scene {
    let width = f64::from(dims.width);
    let height = f64::from(dims.height);
    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, width, height),
    );
    for row in 0..4 {
        for column in 0..4 {
            if (row + column) % 2 == 0 {
                let x = 2.0 + f64::from(column * 6);
                let y = 2.0 + f64::from(row * 6);
                let tile = filled_scene(
                    vello::peniko::Color::WHITE,
                    kurbo::Rect::new(x, y, x + 6.0, y + 6.0),
                );
                scene.encoding_mut().append(tile.encoding(), &None);
            }
        }
    }
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    let vertical = filled_scene(
        vello::peniko::Color::WHITE,
        kurbo::Rect::new(center_x - 4.0, 4.0, center_x + 4.0, height - 4.0),
    );
    let horizontal = filled_scene(
        vello::peniko::Color::WHITE,
        kurbo::Rect::new(4.0, center_y - 4.0, width - 4.0, center_y + 4.0),
    );
    scene.encoding_mut().append(vertical.encoding(), &None);
    scene.encoding_mut().append(horizontal.encoding(), &None);
    scene
}

fn render_at(
    backend: &mut GpuFilterBackend,
    scene: &vello::Scene,
    dims: SceneDimensions,
    region: Option<EffectRegion>,
    chain: &EffectChain,
) -> SceneImage {
    backend
        .render_scene_to_image_gpu_filtered(scene, dims, region, chain)
        .expect("GPU filter chain should succeed")
}

fn render_pending_region(
    backend: &mut GpuFilterBackend,
    scene: &vello::Scene,
    dims: SceneDimensions,
    region: EffectRegion,
    chain: &EffectChain,
) -> SceneImage {
    backend
        .render_scene_to_pending_composite(scene, dims, Some(region), chain, 1.0)
        .expect("pending filtered composite should succeed");
    let layers = backend.take_pending_composites();
    let Some(PendingLayer::Composite(composite)) = layers.into_iter().next() else {
        panic!("expected the effect chain to queue a composite");
    };
    let target = backend.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Filter Submit Policy Harvest Readback Target"),
        size: wgpu::Extent3d {
            width: region.size.width,
            height: region.size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut clear_encoder =
        backend.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Filter Submit Policy Harvest Target Clear"),
        });
    {
        let _clear_pass = clear_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Filter Submit Policy Harvest Target Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    backend.queue.submit(std::iter::once(clear_encoder.finish()));
    backend.core.blit_texture(
        &backend.device,
        &backend.queue,
        &composite.view,
        &target_view,
        region.size.width,
        region.size.height,
        composite.alpha,
    );
    backend.debug_readback_texture(&target, wgpu::Origin3d::ZERO, region.size)
}

fn compare_submit_strategies_on_device(device: &wgpu::Device, queue: &wgpu::Queue) {
    assert!(
        std::env::var_os("ANIMATIX_DUMP_STAGES").is_none(),
        "unset ANIMATIX_DUMP_STAGES so the test exercises Combined submissions"
    );
    let dims = SceneDimensions {
        width: 128,
        height: 96,
    };
    let scene = checker_cross_scene(dims);
    let mut separate = policy_backend(device, queue, dims, PassSubmitStrategy::Separate);
    let mut combined = policy_backend(device, queue, dims, PassSubmitStrategy::Combined);

    let blur = blur_chain(6.0);
    let separate_blur = render_at(&mut separate, &scene, dims, None, &blur);
    let combined_blur = render_at(&mut combined, &scene, dims, None, &blur);
    assert_policy_images_equal("2D Blur", &separate_blur, &combined_blur);
    let center = scene_pixel(&separate_blur, 64, 48);
    let vertical_edge = scene_pixel(&separate_blur, 58, 60);
    let horizontal_edge = scene_pixel(&separate_blur, 76, 42);
    assert!(center[0] > 200, "2D cross center should remain bright: {center:?}");
    assert!(
        (8..247).contains(&vertical_edge[0]),
        "blur should soften the vertical edge: {vertical_edge:?}"
    );
    assert!(
        (8..247).contains(&horizontal_edge[0]),
        "blur should soften the horizontal edge: {horizontal_edge:?}"
    );

    let mut blur_grade_bloom = blur_chain(4.0);
    blur_grade_bloom.instances.push(EffectInstance {
        id: EffectId::new("ColorGrade"),
        enabled: true,
        params: EffectParams {
            values: vec![
                EffectParamValue::F32(1.1),
                EffectParamValue::F32(1.15),
                EffectParamValue::F32(0.8),
                EffectParamValue::F32(0.0),
                EffectParamValue::F32(0.0),
            ],
        },
    });
    blur_grade_bloom.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![
                EffectParamValue::F32(0.65),
                EffectParamValue::F32(0.75),
                EffectParamValue::F32(0.2),
            ],
        },
    });
    let separate_grade_bloom = render_at(&mut separate, &scene, dims, None, &blur_grade_bloom);
    let combined_grade_bloom = render_at(&mut combined, &scene, dims, None, &blur_grade_bloom);
    assert_policy_images_equal(
        "Blur + ColorGrade + Bloom",
        &separate_grade_bloom,
        &combined_grade_bloom,
    );

    let mut repeated = blur_chain(2.0);
    repeated.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(8.0)],
        },
    });
    let separate_repeated = render_at(&mut separate, &scene, dims, None, &repeated);
    let combined_repeated = render_at(&mut combined, &scene, dims, None, &repeated);
    assert_policy_images_equal("repeated Blur instances", &separate_repeated, &combined_repeated);

    let frame_two = blur_chain(2.0);
    let frame_three = blur_chain(9.0);
    let separate_frame_two = render_at(&mut separate, &scene, dims, None, &frame_two);
    let combined_frame_two = render_at(&mut combined, &scene, dims, None, &frame_two);
    assert_policy_images_equal(
        "changed uniforms frame 2",
        &separate_frame_two,
        &combined_frame_two,
    );
    let separate_frame_three = render_at(&mut separate, &scene, dims, None, &frame_three);
    let combined_frame_three = render_at(&mut combined, &scene, dims, None, &frame_three);
    assert_policy_images_equal(
        "changed uniforms frame 3",
        &separate_frame_three,
        &combined_frame_three,
    );
    assert_images_differ(
        "Blur parameter change across frames",
        &separate_frame_two,
        &separate_frame_three,
    );

    let roi_small = EffectRegion {
        origin: [16.0, 8.0],
        size: SceneDimensions {
            width: 32,
            height: 24,
        },
    };
    let roi_wide = EffectRegion {
        origin: [24.0, 20.0],
        size: SceneDimensions {
            width: 80,
            height: 32,
        },
    };
    for (label, region) in [("small ROI", roi_small), ("wide ROI", roi_wide)] {
        let separate_region =
            render_at(&mut separate, &scene, dims, Some(region), &blur_grade_bloom);
        let combined_region =
            render_at(&mut combined, &scene, dims, Some(region), &blur_grade_bloom);
        assert_policy_images_equal(label, &separate_region, &combined_region);
    }

    let separate_harvest =
        render_pending_region(&mut separate, &scene, dims, roi_wide, &blur_grade_bloom);
    let combined_harvest =
        render_pending_region(&mut combined, &scene, dims, roi_wide, &blur_grade_bloom);
    assert_policy_images_equal(
        "multi-pass pending-composite harvest",
        &separate_harvest,
        &combined_harvest,
    );
}

#[test]
fn separate_and_combined_submits_match_on_fallback_adapter() {
    let Some((device, queue, device_type)) = required_adapter_device(true) else {
        return;
    };
    assert_eq!(
        device_type,
        wgpu::DeviceType::Cpu,
        "force_fallback_adapter should select the software CPU adapter"
    );
    compare_submit_strategies_on_device(&device, &queue);
}

#[test]
#[ignore = "manual physical-GPU validation for separate versus combined submissions"]
fn separate_and_combined_submits_match_on_hardware_adapter() {
    let Some((device, queue, device_type)) = required_adapter_device(false) else {
        return;
    };
    assert_ne!(
        device_type,
        wgpu::DeviceType::Cpu,
        "hardware probe requires a non-CPU adapter; use a physical GPU"
    );
    compare_submit_strategies_on_device(&device, &queue);
}

fn render(
    backend: &mut GpuFilterBackend,
    scene: &vello::Scene,
    dims: SceneDimensions,
    chain: &EffectChain,
) -> SceneImage {
    backend
        .render_scene_to_image_gpu_filtered(scene, dims, None, chain)
        .expect("GPU filter chain should succeed")
}

fn assert_image_size(image: &SceneImage, size: f32) {
    assert_eq!(image.natural_size[0], size);
    assert_eq!(image.natural_size[1], size);
}

/// Smoke test: an empty chain returns a valid image of the right size.
#[test]
fn gpu_filter_backend_empty_chain_produces_image() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let image = backend
        .render_scene_to_image_gpu_filtered(
            &vello::Scene::new(),
            DIMS,
            None,
            &EffectChain::default(),
        )
        .expect("empty chain path should succeed");
    assert_image_size(&image, 64.0);
}

/// Smoke test: the blur chain produces a valid SceneImage.
#[test]
fn gpu_filter_backend_blur_chain_produces_image() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let image = render(&mut backend, &vello::Scene::new(), DIMS, &blur_chain(5.0));
    assert_image_size(&image, 64.0);
}

/// Smoke test: the colour-grade chain produces a valid SceneImage.
#[test]
fn gpu_filter_backend_color_grade_produces_image() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let chain = color_grade_chain(1.5, 1.2, 0.5, 45.0, 0.3);
    let image = render(&mut backend, &vello::Scene::new(), DIMS, &chain);
    assert_image_size(&image, 64.0);
}

/// Content-level check that chromatic aberration separates channels across
/// a hard edge: with radial offset, pixels just outside a white region pick
/// up blue from the far sample while red (sampled further out) stays dark.
#[test]
fn chromatic_aberration_fringes_a_hard_edge() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // White half-plane for x < 32 on a transparent background.
    let scene = white_half_scene(64.0);
    let chain = chromatic_aberration_chain(8.0);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    // Pixel right of the boundary, on the horizontal through the
    // centre: blue samples back into the white half-plane, red
    // samples further out into the empty half.
    let (x, y) = (36usize, 32usize);
    let r = raw[(y * w + x) * 4];
    let b = raw[(y * w + x) * 4 + 2];
    assert!(b > r + 60, "expected a blue-dominant fringe at ({x},{y}), got r={r} b={b}");
}

/// Region-scoped chains crop the seed copy, dispatch at the region size,
/// and return a region-sized image: the world sub-rect maps to
/// region-local coordinates.
#[test]
fn region_scoped_chain_crops_and_filters() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // White half-plane for x < 32 on a transparent background.
    let scene = white_half_scene(64.0);
    let region = EffectRegion {
        origin: [16.0, 16.0],
        size: SceneDimensions {
            width: 32,
            height: 32,
        },
    };
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, DIMS, Some(region), &blur_chain(4.0))
        .expect("region-scoped chain should succeed");
    assert_image_size(&image, 32.0);

    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    // Region-local: world x < 32 maps to local x < 16, so (8, 16) sits
    // well inside the white half-plane and stays opaque.
    let a = raw[(16 * w + 8) * 4 + 3];
    assert!(a > 200, "expected opaque white inside the region, got a={a}");
    // World x = 32 maps to local x = 16; blur must soften the edge.
    let mut soft = false;
    for lx in 10..22usize {
        let a = raw[(16 * w + lx) * 4 + 3];
        if a > 40 && a < 215 {
            soft = true;
            break;
        }
    }
    assert!(soft, "blur should soften the world boundary inside the region");
}

/// Where an edge's half-alpha crossing sits, in scene pixels. A chain that
/// dispatches into a smaller texture has to put the result back where it came
/// from, so this is the origin arithmetic the harvest and the composite share.
fn edge_mid_scene_x(raw: &[u8], w: usize, y: usize, span: usize, scale_back: usize) -> usize {
    (0..span).find(|&x| raw[(y * w + x) * 4 + 3] < 128).unwrap_or(span) * scale_back
}

/// How wide the alpha ramp across an edge is: the count of pixels that are
/// neither lit nor empty on a middle row.
fn soft_width(raw: &[u8], w: usize, y: usize, span: usize) -> usize {
    (0..span)
        .filter(|&x| {
            let a = raw[(y * w + x) * 4 + 3];
            a > 40 && a < 215
        })
        .count()
}

/// What the composite hands the drain, at a reduced raster: a texture that is
/// already in target pixels, an origin in the same space, and the factor that
/// tells the drain not to scale it twice. Getting `origin` wrong here puts a
/// frosted panel off to the bottom-right of where it was authored, which no
/// frame-cost measurement would notice.
#[test]
fn a_reduced_raster_composite_is_emitted_in_target_pixels() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
    backend.set_raster_scale(0.5);
    let region = EffectRegion {
        origin: [16.0, 16.0],
        size: SceneDimensions {
            width: 32,
            height: 32,
        },
    };
    backend
        .render_scene_to_pending_composite(&scene, DIMS, Some(region), &blur_chain(8.0), 1.0)
        .expect("region composite at half raster");
    let layers = backend.take_pending_composites();
    let composite = match layers.first() {
        Some(PendingLayer::Composite(composite)) => composite,
        // `PendingLayer` holds wgpu resources, so it cannot be formatted.
        Some(_) => panic!("expected a composite layer, got a backdrop"),
        None => panic!("the scope queued no composite at all"),
    };
    let size = composite.texture.size();
    assert_eq!(
        (size.width, size.height),
        (16, 16),
        "32 scene px of region at half the raster is 16 texels"
    );
    assert_eq!(
        composite.origin,
        [8.0, 8.0],
        "origin is in target pixels, not the scene-pixel 16 it was authored in"
    );
    assert_eq!(composite.texel_scale, 0.5);
}

/// The point of `set_raster_scale`: a blur radius is a distance in scene
/// pixels, so drawing at half the raster has to halve it too. Measured as the
/// width of the edge ramp in *scene* pixels, the two must agree — and the
/// naive alternative (shrink the texture, keep the radius) visibly doubles it.
#[test]
fn a_reduced_raster_keeps_the_blur_the_same_size_in_scene_pixels() {
    let Some((device, queue)) =
        pollster::block_on(create_headless_device("Animatix Filter Test Device"))
    else {
        return;
    };
    let scene = white_half_scene(64.0);

    // Reference: the whole frame at scene resolution, radius 8.
    let mut reference = GpuFilterBackend::new(device.clone(), queue.clone(), DIMS)
        .expect("backend at scene resolution");
    let ref_img = render(&mut reference, &scene, DIMS, &blur_chain(8.0));
    let ref_w = ref_img.natural_size[0] as usize;
    let ref_ramp = soft_width(ref_img.data.data.data(), ref_w, 32, 64);
    assert!(ref_ramp > 2, "the reference blur should show a ramp, got {ref_ramp}");

    // Same scene, same authored radius, half the raster.
    let mut half = GpuFilterBackend::new(device.clone(), queue.clone(), DIMS).expect("backend");
    half.set_raster_scale(0.5);
    let half_img = render(&mut half, &scene, DIMS, &blur_chain(8.0));
    assert_eq!(
        half_img.natural_size,
        [32.0, 32.0],
        "the chain should have dispatched at the reduced raster"
    );
    let half_raw = half_img.data.data.data();
    let half_ramp = soft_width(half_raw, 32, 16, 32) * 2;
    assert!(
        edge_mid_scene_x(ref_img.data.data.data(), 64, 32, 64, 1).abs_diff(32) <= 2,
        "the reference edge should cross half-alpha at scene x=32"
    );
    let half_mid = edge_mid_scene_x(half_raw, 32, 16, 32, 2);
    assert!(
        half_mid.abs_diff(32) <= 3,
        "a chain dispatched at half the raster must still land at scene x=32, got {half_mid}"
    );
    assert!(
        half_ramp.abs_diff(ref_ramp) <= 3,
        "radius 8 at half the raster should ramp like radius 4 did at full: \
             {ref_ramp} scene px vs {half_ramp}"
    );

    // The failure mode this prevents: the reduced texture with the radius
    // still expressed in scene pixels.
    let small = SceneDimensions {
        width: 32,
        height: 32,
    };
    let mut unscaled = GpuFilterBackend::new(device, queue, small).expect("backend at 32x32");
    let mut small_scene = vello::Scene::new();
    small_scene.append(&scene, Some(kurbo::Affine::scale(0.5)));
    let naive = render(&mut unscaled, &small_scene, small, &blur_chain(8.0));
    let naive_ramp = soft_width(naive.data.data.data(), 32, 16, 32) * 2;
    assert!(
        naive_ramp > ref_ramp + 3,
        "keeping a scene-pixel radius at half the raster must read as a wider \
             blur ({ref_ramp} vs {naive_ramp}) — that is the bug the unit is for"
    );
}

#[test]
fn gpu_filter_blur_softens_a_hard_boundary() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // Sharp boundary at x=32: left half white, right half empty.
    let image = render(&mut backend, &white_half_scene(64.0), DIMS, &blur_chain(8.0));
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    // Middle row: within ±8px of the boundary (x=32), expect an
    // intermediate alpha (soft edge), not a hard 0/255 step.
    let y = 32usize;
    let mut soft = false;
    for x in 20..44usize {
        let a = raw[(y * w + x) * 4 + 3];
        if a > 40 && a < 215 {
            soft = true;
            break;
        }
    }
    assert!(
        soft,
        "blur should soften the boundary at x=32, but all sampled pixels look hard"
    );
}

/// Repeated instances of one effect type must keep separate author uniforms.
/// The former per-type single buffer let the last `Blur.radius` write replace
/// the first instance's parameters, making this chain identical to two radius
/// 8 blurs.
#[test]
fn repeated_effect_instances_keep_distinct_author_uniforms() {
    let Some(mut backend) = required_gpu_backend(DIMS) else {
        return;
    };
    let scene = white_half_scene(64.0);

    let mut different_radii = blur_chain(2.0);
    different_radii.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(8.0)],
        },
    });

    let mut equal_radii = blur_chain(8.0);
    equal_radii.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(8.0)],
        },
    });

    let alpha_delta = |left: &SceneImage, right: &SceneImage| -> u64 {
        left.data
            .data
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(right.data.data.data().as_chunks::<4>().0)
            .map(|(left, right)| u64::from(left[3].abs_diff(right[3])))
            .sum()
    };

    // The second render reuses the same texture views, buffer, and bind groups
    // while changing slot 0's radius; stale or type-wide author uniforms would
    // make the outputs identical.
    let different = render(&mut backend, &scene, DIMS, &different_radii);
    let equal = render(&mut backend, &scene, DIMS, &equal_radii);
    let cached_delta = alpha_delta(&different, &equal);
    assert!(
        cached_delta > 64,
        "Blur(2) → Blur(8) must differ from Blur(8) → Blur(8), got total alpha delta {cached_delta}"
    );

    // The backend now has two slots allocated for Blur. Adding a third
    // instance grows that buffer; the result must still preserve the first two
    // instances' values after replacing the allocation.
    let mut three_distinct = different_radii.clone();
    three_distinct.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(4.0)],
        },
    });
    let mut three_equal = blur_chain(4.0);
    three_equal.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(4.0)],
        },
    });
    three_equal.instances.push(EffectInstance {
        id: EffectId::new("Blur"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(4.0)],
        },
    });
    let grown = render(&mut backend, &scene, DIMS, &three_distinct);
    let grown_equal = render(&mut backend, &scene, DIMS, &three_equal);
    let growth_delta = alpha_delta(&grown, &grown_equal);
    assert!(
        growth_delta > 64,
        "Blur(2) → Blur(8) → Blur(4) must differ after uniform-buffer growth from three Blur(4) stages, got total alpha delta {growth_delta}"
    );
}

/// Bloom is a composite: it adds the pre-chain original back onto the running
/// result. On a black canvas, a mid-gray rect run through `Blur` alone has a
/// dimmed edge; appending `Bloom` restores the original's light on top of it.
#[test]
fn bloom_adds_the_pre_chain_original_to_the_blur() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let rect = filled_scene(
        vello::peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(20.0, 20.0, 44.0, 44.0),
    );
    scene.encoding_mut().append(rect.encoding(), &None);

    let blurred = render(&mut backend, &scene, DIMS, &blur_chain(6.0));

    let mut chain = blur_chain(6.0);
    chain.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)],
        },
    });
    let bloomed = render(&mut backend, &scene, DIMS, &chain);

    let w = bloomed.natural_size[0] as usize;
    let (x, y) = (20usize, 32usize);
    let before = blurred.data.data.data()[(y * w + x) * 4];
    let after = bloomed.data.data.data()[(y * w + x) * 4];
    assert!(
        after > before + 20,
        "Bloom must add the pre-chain original at the blurred rect edge, \
         got blur={before} blur+bloom={after}"
    );
}

/// `Bloom.threshold` gates the added light by the running result's luma. Two
/// flat regions — a bright one above the threshold and a dark one below it —
/// run through the same `Blur → Bloom` chain: at threshold 0.6 only the bright
/// region brightens. At threshold 0 both regions brighten, which is the
/// historical exposure lift the gate must not disturb.
#[test]
fn bloom_threshold_brightens_only_bright_regions() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // Opaque black canvas with a bright (230) and a dark (60) opaque rect,
    // separated far enough that a small blur cannot mix them.
    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let bright = filled_scene(
        vello::peniko::Color::from_rgba8(230, 230, 230, 255),
        kurbo::Rect::new(6.0, 20.0, 26.0, 44.0),
    );
    let dark = filled_scene(
        vello::peniko::Color::from_rgba8(60, 60, 60, 255),
        kurbo::Rect::new(38.0, 20.0, 58.0, 44.0),
    );
    scene.encoding_mut().append(bright.encoding(), &None);
    scene.encoding_mut().append(dark.encoding(), &None);

    let blur_bloom_chain = |threshold: f32| {
        let mut chain = blur_chain(3.0);
        chain.instances.push(EffectInstance {
            id: EffectId::new("Bloom"),
            enabled: true,
            params: EffectParams {
                values: vec![
                    EffectParamValue::F32(1.0), // intensity
                    EffectParamValue::F32(1.0), // keep
                    EffectParamValue::F32(threshold),
                ],
            },
        });
        chain
    };
    let red = |image: &SceneImage, x: usize, y: usize| {
        let w = image.natural_size[0] as usize;
        image.data.data.data()[(y * w + x) * 4]
    };

    let plain = render(&mut backend, &scene, DIMS, &EffectChain::default());
    let (plain_bright, plain_dark) = (red(&plain, 16, 32), red(&plain, 48, 32));

    let gated = render(&mut backend, &scene, DIMS, &blur_bloom_chain(0.6));
    let (gated_bright, gated_dark) = (red(&gated, 16, 32), red(&gated, 48, 32));
    assert!(
        gated_bright > plain_bright + 15,
        "threshold 0.6 must still brighten the bright region, got plain={plain_bright} gated={gated_bright}"
    );
    assert!(
        gated_dark.abs_diff(plain_dark) <= 3,
        "threshold 0.6 must leave the dark region alone, got plain={plain_dark} gated={gated_dark}"
    );

    let open = render(&mut backend, &scene, DIMS, &blur_bloom_chain(0.0));
    let (open_bright, open_dark) = (red(&open, 16, 32), red(&open, 48, 32));
    assert!(
        open_bright > plain_bright + 15,
        "threshold 0 must still brighten the bright region, got plain={plain_bright} bloom={open_bright}"
    );
    assert!(
        open_dark > plain_dark + 20,
        "threshold 0 must preserve the historical exposure lift on dark regions, \
         got plain={plain_dark} bloom={open_dark}"
    );
}

/// Bloom's texture contract is straight alpha. A half-alpha saturated source
/// contributes its premultiplied light and must be unpremultiplied on store.
#[test]
fn bloom_keeps_translucent_saturated_rgb_straight() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let chain =
        effect_chain("Bloom", vec![EffectParamValue::F32(0.25), EffectParamValue::F32(1.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let pixel = scene_pixel(&image, 32, 32);
    assert!(
        pixel[0] >= 248 && pixel[1] <= 4 && pixel[2] <= 4,
        "Bloom must preserve saturated straight red on translucent content, got {pixel:?}"
    );
    assert!(
        pixel[3].abs_diff(160) <= 2,
        "intensity 0.25 should raise half alpha to about 0.625, got {pixel:?}"
    );

    let reduced_keep_chain =
        effect_chain("Bloom", vec![EffectParamValue::F32(0.25), EffectParamValue::F32(0.5)]);
    let reduced_keep_image = render(&mut backend, &scene, DIMS, &reduced_keep_chain);
    let reduced_keep_pixel = scene_pixel(&reduced_keep_image, 32, 32);
    assert!(
        reduced_keep_pixel[0].abs_diff(153) <= 3,
        "keep 0.5 must scale only the original's straight red contribution, got {reduced_keep_pixel:?}"
    );
    assert!(
        reduced_keep_pixel[3].abs_diff(160) <= 3,
        "keep must not change the existing alpha accumulation, got {reduced_keep_pixel:?}"
    );
}

/// A DropShadow penumbra is a partial-alpha blue glow on transparent original
/// pixels. Bloom must retain the glow's straight blue colour while adding its
/// coverage to the frame.
#[test]
fn bloom_keeps_partial_glow_rgb_straight() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(24.0, 16.0, 56.0, 48.0),
    );
    let mut chain = blue_drop_shadow_chain([-8.0, 0.0], 6.0);
    chain.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)],
        },
    });
    let image = render(&mut backend, &scene, DIMS, &chain);
    // The original shape starts at x=24, and the hard shadow starts at x=16;
    // x=12 is reached only by the soft penumbra.
    let penumbra = scene_pixel(&image, 12, 32);
    assert!(
        (8..150).contains(&penumbra[3]),
        "the penumbra must remain partially covered after Bloom, got {penumbra:?}"
    );
    assert!(
        penumbra[2] >= 248 && penumbra[0] <= 8 && penumbra[1] <= 8,
        "Bloom must preserve straight blue in the partial glow, got {penumbra:?}"
    );
}

/// Zero-alpha texels remain transparent black after Bloom, without producing
/// NaNs or leaking a glow.
#[test]
fn bloom_zero_alpha_stores_transparent_black() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(20, 80, 255, 0),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let chain = effect_chain("Bloom", vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    assert_eq!(
        scene_pixel(&image, 28, 32),
        [0, 0, 0, 0],
        "zero-alpha input must not leak color through Bloom"
    );
}

/// Additive alpha saturates at one, and the corresponding premultiplied color
/// is clamped to that coverage before it is stored straight.
#[test]
fn bloom_clamps_added_alpha_to_opaque() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 255, 0, 128),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let chain = effect_chain("Bloom", vec![EffectParamValue::F32(4.0), EffectParamValue::F32(1.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let pixel = scene_pixel(&image, 32, 32);
    assert_eq!(pixel, [0, 255, 0, 255], "intensity 4 should saturate to opaque green");
}

/// Threshold measures straight RGB luma even for partial-alpha pixels. White
/// is above the gate and red is below it; each output keeps its saturated color.
#[test]
fn bloom_threshold_uses_straight_rgb_for_translucent_pixels() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let mut scene = vello::Scene::new();
    let white = filled_scene(
        vello::peniko::Color::from_rgba8(255, 255, 255, 64),
        kurbo::Rect::new(6.0, 20.0, 26.0, 44.0),
    );
    let red = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 64),
        kurbo::Rect::new(38.0, 20.0, 58.0, 44.0),
    );
    scene.encoding_mut().append(white.encoding(), &None);
    scene.encoding_mut().append(red.encoding(), &None);
    let chain = effect_chain(
        "Bloom",
        vec![
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(0.6),
        ],
    );
    let image = render(&mut backend, &scene, DIMS, &chain);
    let bright = scene_pixel(&image, 16, 32);
    let dim = scene_pixel(&image, 48, 32);
    assert!(
        bright[3].abs_diff(128) <= 2,
        "straight white luma is above threshold and should raise quarter alpha to about half alpha, \
         got {bright:?}"
    );
    assert!(
        dim[3].abs_diff(64) <= 1,
        "straight red luma is below threshold and should keep source alpha, got {dim:?}"
    );
    assert!(
        bright[..3].iter().all(|channel| *channel >= 248),
        "bright translucent white must remain straight white, got {bright:?}"
    );
    assert!(
        dim[0] >= 248 && dim[1] <= 4 && dim[2] <= 4,
        "gated translucent red must remain straight red, got {dim:?}"
    );
}

/// The pre-chain original supplies the kept base while the running result
/// supplies the glow: doubling red in ColorGrade therefore adds the brighter
/// copy over the unmodified original.
#[test]
fn bloom_uses_running_result_for_glow_and_original_for_keep() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(64, 0, 0, 128),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let mut chain = color_grade_chain(2.0, 1.0, 1.0, 0.0, 0.0);
    chain.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)],
        },
    });
    let image = render(&mut backend, &scene, DIMS, &chain);
    let pixel = scene_pixel(&image, 32, 32);
    assert!(
        pixel[0].abs_diff(96) <= 3,
        "Bloom must add the doubled running red to the original base in premultiplied space, \
         got {pixel:?}"
    );
    assert_eq!(pixel[3], 255, "two half-alpha contributions should saturate to opaque");
}

/// Check that the colour-grade pass mutates pixels: a red rectangle with
/// `saturate: 0` must desaturate to gray.
#[test]
fn color_matrix_actually_desaturates() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 255),
        kurbo::Rect::new(16.0, 16.0, 48.0, 48.0),
    );
    let chain = color_grade_chain(1.0, 1.0, 0.0, 0.0, 0.0);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    let (r, g, b) =
        (raw[(32 * w + 32) * 4], raw[(32 * w + 32) * 4 + 1], raw[(32 * w + 32) * 4 + 2]);
    // Desaturated red → channels roughly equal (gray).
    assert!(
        r > 30 && (r as i32 - g as i32).abs() < 40 && (r as i32 - b as i32).abs() < 40,
        "saturate=0 should desaturate red to gray (Rec.709 luma ≈54, not empty), got r={r} g={g} b={b}"
    );
}

/// Vignette must darken the corner while leaving the center untouched.
#[test]
fn vignette_darkens_corners_not_center() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
    let chain = effect_chain(
        "Vignette",
        vec![
            EffectParamValue::F32(1.0),  // amount
            EffectParamValue::F32(0.15), // radius (start close to center)
            EffectParamValue::F32(0.9),  // softness
            EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        ],
    );
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let center = raw[(32 * w + 32) * 4];
    let corner = raw[(2 * w + 2) * 4];
    assert!(center > 240, "center must keep its luminance, got {center}");
    assert!(corner < 120, "corner must fall into the vignette color, got {corner}");
}

/// Levels: lifting the black point above the input gray must floor it to
/// the (raised) black output — the classic contrast-crush check.
#[test]
fn levels_black_point_maps_below_floor_to_black() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let dims = SceneDimensions {
        width: 32,
        height: 32,
    };

    // 25% gray fills the canvas.
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(64, 64, 64, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
    );
    let chain = effect_chain(
        "Levels",
        vec![
            EffectParamValue::F32(0.3), // in_black above the input gray
            EffectParamValue::F32(1.0), // in_white
            EffectParamValue::F32(1.0), // gamma
            EffectParamValue::F32(0.0), // out_black
            EffectParamValue::F32(1.0), // out_white
        ],
    );
    let image = render(&mut backend, &scene, dims, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    let r = raw[(16 * w + 16) * 4];
    assert!(r < 20, "0.25 gray under in_black=0.3 must crush to black, got {r}");

    // gamma = 2 brightens mid-gray: 0.5^(1/2) ≈ 0.707.
    let chain = effect_chain(
        "Levels",
        vec![
            EffectParamValue::F32(0.0),
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(2.0),
            EffectParamValue::F32(0.0),
            EffectParamValue::F32(1.0),
        ],
    );
    let image = render(
        &mut backend,
        &filled_scene(
            vello::peniko::Color::from_rgba8(128, 128, 128, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        ),
        dims,
        &chain,
    );
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    let r = raw[(16 * w + 16) * 4];
    assert!(
        (150..=210).contains(&r),
        "gamma 2 on 0.5 gray must land near 0.707·255 ≈ 180, got {r}"
    );
}

/// Sharpen: along a gray→black edge, the gray side must overshoot its
/// interior value (the high-frequency residual is added back).
#[test]
fn sharpen_overshoots_at_hard_edges() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // Left half 50% gray, right half black: one hard vertical edge at x=32.
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let chain =
        effect_chain("Sharpen", vec![EffectParamValue::F32(2.0), EffectParamValue::F32(2.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    let interior = raw[(32 * w + 8) * 4];
    let edge = raw[(32 * w + 30) * 4];
    assert_eq!(interior, 128, "interior gray must survive sharpening");
    assert!(
        edge > interior + 30,
        "gray side of the edge must overshoot the interior gray, got edge={edge} interior={interior}"
    );
}

/// Grain must actually perturb a flat field, and be deterministic for a
/// given (params, time) pair (§6 determinism contract).
#[test]
fn grain_perturbs_flat_field_deterministically() {
    let Some((device, queue)) =
        pollster::block_on(create_headless_device("Animatix Filter Test Device"))
    else {
        return;
    };
    let make_backend = || {
        GpuFilterBackend::new(device.clone(), queue.clone(), DIMS)
            .expect("GpuFilterBackend should initialise")
    };

    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let params = vec![
        EffectParamValue::F32(0.6), // amount
        EffectParamValue::F32(0.0), // seed
        EffectParamValue::Bool(true),
    ];

    let image = render(&mut make_backend(), &scene, DIMS, &effect_chain("Grain", params.clone()));
    let raw = image.data.data.data();
    let touched = raw.as_chunks::<4>().0.iter().filter(|px| px[0] != 128).count();
    assert!(
        touched > 64,
        "grain should perturb most pixels of a flat field, only {touched} changed"
    );

    let image_again = render(&mut make_backend(), &scene, DIMS, &effect_chain("Grain", params));
    assert_eq!(raw, image_again.data.data.data(), "grain must be deterministic");
}

/// MotionBlur with angle 0 must smear horizontally: intermediate alpha
/// appears up to `length` pixels right of the shape's edge.
#[test]
fn motion_blur_smears_horizontally() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // Opaque white rect over the left third.
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 21.0, 64.0));
    let chain =
        effect_chain("MotionBlur", vec![EffectParamValue::F32(12.0), EffectParamValue::F32(0.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    let y = 32usize;
    let mut smeared = false;
    for x in 22..33usize {
        let a = raw[(y * w + x) * 4 + 3];
        if a > 20 && a < 235 {
            smeared = true;
            break;
        }
    }
    assert!(smeared, "motion blur must smear the trailing edge to intermediate alpha");

    // Far outside the smear extent the frame must stay empty.
    let a = raw[(y * w + 45) * 4 + 3];
    assert!(a < 20, "smear must not reach beyond length pixels, got a={a}");
}

/// Duotone must map black onto the shadow colour and white onto the
/// highlight colour at full strength.
#[test]
fn duotone_maps_luma_onto_the_ramp() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let white = filled_scene(
        vello::peniko::Color::from_rgba8(255, 255, 255, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.encoding_mut().append(white.encoding(), &None);

    let chain = effect_chain(
        "Duotone",
        vec![
            EffectParamValue::F32(1.0),                   // amount
            EffectParamValue::Vec4([0.0, 0.0, 1.0, 1.0]), // shadow: blue
            EffectParamValue::Vec4([1.0, 0.5, 0.0, 1.0]), // highlight: orange
        ],
    );
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let dark = &raw[(32 * w + 8) * 4..(32 * w + 8) * 4 + 3];
    assert!(
        dark[2] > 200 && dark[0] < 60,
        "black input must map to the shadow colour, got {dark:?}"
    );
    let light = &raw[(32 * w + 56) * 4..(32 * w + 56) * 4 + 3];
    assert!(
        light[0] > 200 && light[2] < 80,
        "white input must map to the highlight colour, got {light:?}"
    );
}

/// Posterize with two levels must push 0.4 to black and 0.6 to white.
#[test]
fn posterize_quantises_midtones() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(102, 102, 102, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let lighter = filled_scene(
        vello::peniko::Color::from_rgba8(153, 153, 153, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.encoding_mut().append(lighter.encoding(), &None);

    let chain = effect_chain("Posterize", vec![EffectParamValue::F32(2.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let low = raw[(32 * w + 8) * 4];
    let high = raw[(32 * w + 56) * 4];
    assert!(low < 40, "0.4 must quantise down to black, got {low}");
    assert!(high > 200, "0.6 must quantise up to white, got {high}");
}

/// Edge must light the step boundary and darken flat interiors.
#[test]
fn edge_lights_the_step_boundary() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let mut scene = filled_scene(
        vello::peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let white = filled_scene(
        vello::peniko::Color::from_rgba8(255, 255, 255, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.encoding_mut().append(white.encoding(), &None);

    let chain = effect_chain("Edge", vec![EffectParamValue::F32(1.0), EffectParamValue::F32(0.0)]);
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let boundary = raw[(32 * w + 32) * 4];
    let flat = raw[(32 * w + 50) * 4];
    assert!(boundary > 150, "the step edge must light up, got {boundary}");
    assert!(flat < 40, "a flat region must stay dark, got {flat}");
}

/// LensDistortion must displace samples: a band that survives at the
/// control setting must move out of a near-edge pixel at `amount: 24` scene
/// pixels, while the centre (radius 0) is untouched either way.
#[test]
fn lens_distortion_displaces_samples_toward_the_centre() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };

    // White band over the leftmost 6 columns.
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 6.0, 64.0));

    let control = effect_chain("LensDistortion", vec![EffectParamValue::F32(0.0)]);
    let control_image = render(&mut backend, &scene, DIMS, &control);
    let cw = control_image.natural_size[0] as usize;
    let control_raw = control_image.data.data.data();
    assert!(control_raw[(32 * cw + 2) * 4] > 200, "control must show the white band at x=2");

    let warped = effect_chain("LensDistortion", vec![EffectParamValue::F32(24.0)]);
    let image = render(&mut backend, &scene, DIMS, &warped);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    assert!(
        raw[(32 * w + 2) * 4] < 60,
        "the warp must pull the band out of the near-edge pixel, got {}",
        raw[(32 * w + 2) * 4]
    );
}

/// `LensDistortion.amount` is a scene-pixel distance, so the raster scale must
/// not change how far an edge moves: full raster and half raster have to land
/// on the same scene coordinate. Without the declared pixel unit the half-raster
/// chain would apply the same number in half-size texels and displace twice as
/// far in scene pixels.
#[test]
fn lens_distortion_keeps_the_same_scene_pixel_displacement_at_half_raster() {
    let Some((device, queue)) =
        pollster::block_on(create_headless_device("Animatix Filter Test Device"))
    else {
        return;
    };
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 16.0, 64.0));
    let chain = effect_chain("LensDistortion", vec![EffectParamValue::F32(24.0)]);

    let mut full = GpuFilterBackend::new(device.clone(), queue.clone(), DIMS)
        .expect("backend at scene resolution");
    let full_image = render(&mut full, &scene, DIMS, &chain);
    let full_w = full_image.natural_size[0] as usize;
    let full_edge = edge_mid_scene_x(full_image.data.data.data(), full_w, 32, 64, 1);
    assert!(
        (5..20).contains(&full_edge),
        "the distorted band edge must stay visible on the left half, got x={full_edge}"
    );

    let mut half = GpuFilterBackend::new(device, queue, DIMS).expect("backend at half raster");
    half.set_raster_scale(0.5);
    let half_image = render(&mut half, &scene, DIMS, &chain);
    assert_eq!(
        half_image.natural_size,
        [32.0, 32.0],
        "the chain should have dispatched at the reduced raster"
    );
    let half_edge = edge_mid_scene_x(half_image.data.data.data(), 32, 16, 32, 2);
    assert!(
        full_edge.abs_diff(half_edge) <= 3,
        "amount 24 px must displace the same scene pixels at half raster: \
         full edge x={full_edge}, half edge x={half_edge}"
    );
}

/// DropShadow must paint the silhouette offset behind the content, and
/// leave the content itself untouched.
#[test]
fn drop_shadow_offsets_the_silhouette() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
    let chain = effect_chain(
        "DropShadow",
        vec![
            EffectParamValue::Vec2([-8.0, 0.0]), // offset (shadow to the left)
            EffectParamValue::F32(0.0),          // softness (hard silhouette)
            EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]), // opaque black shadow
        ],
    );
    let image = render(&mut backend, &scene, DIMS, &chain);
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let shadow = &raw[(32 * w + 20) * 4..(32 * w + 20) * 4 + 4];
    assert!(
        shadow[0] < 40 && shadow[3] > 200,
        "the offset silhouette must be dark and opaque, got {shadow:?}"
    );
    let content = raw[(32 * w + 40) * 4];
    assert!(content > 200, "the content must stay white, got {content}");
    let clear = raw[(32 * w + 4) * 4 + 3];
    assert!(clear < 20, "outside the shadow extent the frame stays empty, got {clear}");
}

/// `softness` must spread the silhouette into a penumbra that the hard
/// case does not have, in the same scene at the same offset.
#[test]
fn drop_shadow_softness_spreads_the_silhouette() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
    let color = EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]);
    let mut shadow_at = |softness: f32| {
        let chain = effect_chain(
            "DropShadow",
            vec![
                EffectParamValue::Vec2([-8.0, 0.0]),
                EffectParamValue::F32(softness),
                color,
            ],
        );
        render(&mut backend, &scene, DIMS, &chain)
    };
    // The hard shadow's left edge sits at x=16 (rect left 24, offset -8);
    // x=12 is 4px outside it, so only a penumbra can reach it.
    let px = (32 * DIMS.width as usize + 12) * 4 + 3;

    let hard = shadow_at(0.0);
    assert_eq!(hard.data.data.data()[px], 0, "softness 0 must stay hard: nothing reaches x=12");
    let soft = shadow_at(6.0);
    let alpha = soft.data.data.data()[px];
    assert!(
        (20..200).contains(&alpha),
        "softness 6 must cast a partial-coverage penumbra at x=12, got alpha {alpha}"
    );
}

fn blue_drop_shadow_chain(offset: [f32; 2], softness: f32) -> EffectChain {
    blue_drop_shadow_chain_with_alpha(offset, softness, 1.0)
}

fn blue_drop_shadow_chain_with_alpha(
    offset: [f32; 2],
    softness: f32,
    color_alpha: f32,
) -> EffectChain {
    effect_chain(
        "DropShadow",
        vec![
            EffectParamValue::Vec2(offset),
            EffectParamValue::F32(softness),
            EffectParamValue::Vec4([0.0, 0.0, 1.0, color_alpha]),
        ],
    )
}

/// DropShadow samples a half-alpha silhouette for its blue shadow. The
/// shadow-only texel must carry straight blue RGB with half alpha, while an
/// opaque silhouette keeps its historical opaque-blue result.
#[test]
fn drop_shadow_keeps_translucent_colored_shadow_rgb_straight() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let rect = kurbo::Rect::new(20.0, 16.0, 36.0, 48.0);
    let translucent = filled_scene(vello::peniko::Color::from_rgba8(255, 0, 0, 128), rect);
    let chain = blue_drop_shadow_chain([8.0, 0.0], 0.0);
    let image = render(&mut backend, &translucent, DIMS, &chain);
    let shadow_only = scene_pixel(&image, 40, 32);
    assert_eq!(
        shadow_only[3], 128,
        "half-alpha silhouette should cast half-alpha shadow: {shadow_only:?}"
    );
    assert!(
        shadow_only[2] >= 248 && shadow_only[0] <= 8 && shadow_only[1] <= 8,
        "shadow-only RGB must remain straight blue instead of being scaled by alpha: {shadow_only:?}"
    );

    let opaque = filled_scene(vello::peniko::Color::WHITE, rect);
    let opaque_image = render(&mut backend, &opaque, DIMS, &chain);
    let opaque_shadow = scene_pixel(&opaque_image, 40, 32);
    assert_eq!(opaque_shadow, [0, 0, 255, 255], "opaque shadow output must remain unchanged");

    // Exercise the shadow-color alpha independently of source transparency.
    let half_opacity_chain = blue_drop_shadow_chain_with_alpha([8.0, 0.0], 0.0, 0.5);
    let half_opacity_image = render(&mut backend, &opaque, DIMS, &half_opacity_chain);
    let half_opacity_shadow = scene_pixel(&half_opacity_image, 40, 32);
    assert_eq!(
        half_opacity_shadow[3], 128,
        "opaque silhouette with 50% shadow alpha should produce alpha 128: {half_opacity_shadow:?}"
    );
    assert!(
        half_opacity_shadow[2] >= 248 && half_opacity_shadow[0] <= 8 && half_opacity_shadow[1] <= 8,
        "50% blue shadow color must keep straight blue RGB: {half_opacity_shadow:?}"
    );
}

/// A half-alpha red source over a half-alpha blue shadow has alpha 0.75 and
/// straight RGB (2/3 red, 1/3 blue) after premultiplied over composition.
#[test]
fn drop_shadow_composites_translucent_content_over_shadow() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let image = render(&mut backend, &scene, DIMS, &blue_drop_shadow_chain([8.0, 0.0], 0.0));
    let overlap = scene_pixel(&image, 32, 32);
    assert!(
        overlap[3].abs_diff(192) <= 2,
        "overlap alpha should be approximately 0.75, got {overlap:?}"
    );
    assert!(
        overlap[0].abs_diff(170) <= 4 && overlap[1] <= 4 && overlap[2].abs_diff(85) <= 4,
        "overlap RGB should be straight-alpha red 170 / blue 85, got {overlap:?}"
    );
}

/// Zero-alpha inputs must produce transparent black, without NaNs or shadow
/// color leaking into pixels that have no silhouette coverage.
#[test]
fn drop_shadow_zero_alpha_input_does_not_leak_color() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(20, 80, 255, 0),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let image = render(&mut backend, &scene, DIMS, &blue_drop_shadow_chain([8.0, 0.0], 0.0));
    for (x, y) in [(28, 32), (40, 32)] {
        let pixel = scene_pixel(&image, x, y);
        assert_eq!(
            pixel,
            [0, 0, 0, 0],
            "zero-alpha input must stay transparent black at ({x},{y}), got {pixel:?}"
        );
    }
}

/// Softness creates a partially covered colored shadow. Its alpha is reduced
/// by coverage, while its straight blue RGB remains fully saturated.
#[test]
fn drop_shadow_soft_penumbra_keeps_colored_rgb_straight() {
    let Some(mut backend) = gpu_backend(DIMS) else {
        return;
    };
    let scene = filled_scene(
        vello::peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(24.0, 16.0, 56.0, 48.0),
    );
    let image = render(&mut backend, &scene, DIMS, &blue_drop_shadow_chain([-8.0, 0.0], 6.0));
    // As in the opaque softness check above, x=12 is four pixels beyond the
    // hard edge at x=16, so only the penumbra reaches this shadow-only pixel.
    let penumbra = scene_pixel(&image, 12, 32);
    assert!(
        (8..150).contains(&penumbra[3]),
        "softness should leave partial alpha at the penumbra: {penumbra:?}"
    );
    assert!(
        penumbra[2] >= 248 && penumbra[0] <= 8 && penumbra[1] <= 8,
        "soft penumbra RGB must stay straight blue despite partial coverage: {penumbra:?}"
    );
}

#[path = "straight_alpha_tests.rs"]
mod straight_alpha_tests;
