use animatix::timeline::effects::{
    EffectId, EffectInstance, EffectParamValue, EffectParams, EffectRegion, PendingLayer,
};
use animatix_core::effect::EffectParamKind;

use super::*;

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
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new("Blur"),
            enabled: true,
            params: EffectParams {
                values: vec![EffectParamValue::F32(radius)],
            },
        }],
        time_ms: 0.0,
    }
}

fn color_grade_chain(
    brightness: f32,
    contrast: f32,
    saturate: f32,
    hue_rotate: f32,
    sepia: f32,
) -> EffectChain {
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new("ColorGrade"),
            enabled: true,
            params: EffectParams {
                values: vec![
                    EffectParamValue::F32(brightness),
                    EffectParamValue::F32(contrast),
                    EffectParamValue::F32(saturate),
                    EffectParamValue::F32(hue_rotate),
                    EffectParamValue::F32(sepia),
                ],
            },
        }],
        time_ms: 0.0,
    }
}

fn chromatic_aberration_chain(offset: f32) -> EffectChain {
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new("ChromaticAberration"),
            enabled: true,
            params: EffectParams {
                values: vec![EffectParamValue::F32(offset)],
            },
        }],
        time_ms: 0.0,
    }
}

async fn create_headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: true,
        })
        .await
        .ok()?;
    let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Animatix Filter Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: needed_limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .ok()?;
    Some((device, queue))
}

/// Smoke test: an empty chain returns a valid image of the right size.
#[test]
fn gpu_filter_backend_empty_chain_produces_image() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let scene = vello::Scene::new();
        let result =
            backend.render_scene_to_image_gpu_filtered(&scene, dims, None, &EffectChain::default());
        assert!(result.is_ok(), "empty chain path should succeed");
        let image = result.unwrap();
        assert_eq!(image.natural_size[0], 64.0);
        assert_eq!(image.natural_size[1], 64.0);
    }
}

/// Smoke test: the blur chain produces a valid SceneImage.
#[test]
fn gpu_filter_backend_blur_chain_produces_image() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let scene = vello::Scene::new();
        let result =
            backend.render_scene_to_image_gpu_filtered(&scene, dims, None, &blur_chain(5.0));
        assert!(result.is_ok(), "GPU blur chain path should succeed");
        let image = result.unwrap();
        assert_eq!(image.natural_size[0], 64.0);
        assert_eq!(image.natural_size[1], 64.0);
    }
}

/// Smoke test: the colour-grade chain produces a valid SceneImage.
#[test]
fn gpu_filter_backend_color_grade_produces_image() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let scene = vello::Scene::new();
        let chain = color_grade_chain(1.5, 1.2, 0.5, 45.0, 0.3);
        let result = backend.render_scene_to_image_gpu_filtered(&scene, dims, None, &chain);
        assert!(result.is_ok(), "GPU color-grade path should succeed");
        let image = result.unwrap();
        assert_eq!(image.natural_size[0], 64.0);
        assert_eq!(image.natural_size[1], 64.0);
    }
}

/// Content-level check that chromatic aberration separates channels across
/// a hard edge: with radial offset, pixels just outside a white region pick
/// up blue from the far sample while red (sampled further out) stays dark.
#[test]
fn chromatic_aberration_fringes_a_hard_edge() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // White half-plane for x < 32 on a transparent background.
        let mut scene = vello::Scene::new();
        use kurbo::Shape;
        let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::WHITE,
            None,
            &rect,
        );

        let chain = chromatic_aberration_chain(8.0);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("chromatic aberration path should succeed");
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
}

/// Region-scoped chains crop the seed copy, dispatch at the region size,
/// and return a region-sized image: the world sub-rect maps to
/// region-local coordinates.
#[test]
fn region_scoped_chain_crops_and_filters() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // White half-plane for x < 32 on a transparent background.
        let mut scene = vello::Scene::new();
        use kurbo::Shape;
        let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::WHITE,
            None,
            &rect,
        );

        let region = EffectRegion {
            origin: [16.0, 16.0],
            size: SceneDimensions {
                width: 32,
                height: 32,
            },
        };
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, Some(region), &blur_chain(4.0))
            .expect("region-scoped chain should succeed");
        assert_eq!(image.natural_size[0], 32.0);
        assert_eq!(image.natural_size[1], 32.0);

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
}

/// Content-level check that blur actually softens a hard boundary.
///
/// Probe 009 root cause: back-to-back compute passes sharing ping-pong
/// textures in one encoder did not synchronise; the passes are now split by
/// submit. This test reads pixels back and verifies a sharp black/white edge
/// becomes a gradient (intermediate alpha) inside the blur radius.
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

/// The point of `set_raster_scale`: a blur radius is a distance in scene
/// pixels, so drawing at half the raster has to halve it too. Measured as the
/// width of the edge ramp in *scene* pixels, the two must agree — and the
/// naive alternative (shrink the texture, keep the radius) visibly doubles it.
/// What the composite hands the drain, at a reduced raster: a texture that is
/// already in target pixels, an origin in the same space, and the factor that
/// tells the drain not to scale it twice. Getting `origin` wrong here puts a
/// frosted panel off to the bottom-right of where it was authored, which no
/// frame-cost measurement would notice.
#[test]
fn a_reduced_raster_composite_is_emitted_in_target_pixels() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut scene = vello::Scene::new();
    use kurbo::Shape;
    scene.fill(
        vello::peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        vello::peniko::Color::WHITE,
        None,
        &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0).to_path(1e-3),
    );
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("backend at scene resolution");
    backend.set_raster_scale(0.5);
    let region = EffectRegion {
        origin: [16.0, 16.0],
        size: SceneDimensions {
            width: 32,
            height: 32,
        },
    };
    backend
        .render_scene_to_pending_composite(&scene, dims, Some(region), &blur_chain(8.0), 1.0)
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

#[test]
fn a_reduced_raster_keeps_the_blur_the_same_size_in_scene_pixels() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let full = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut scene = vello::Scene::new();
    use kurbo::Shape;
    let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
    scene.fill(
        vello::peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        vello::peniko::Color::WHITE,
        None,
        &rect,
    );

    // Reference: the whole frame at scene resolution, radius 8.
    let mut reference = GpuFilterBackend::new(device.clone(), queue.clone(), full)
        .expect("backend at scene resolution");
    let ref_img = reference
        .render_scene_to_image_gpu_filtered(&scene, full, None, &blur_chain(8.0))
        .expect("reference blur");
    let ref_w = ref_img.natural_size[0] as usize;
    let ref_ramp = soft_width(ref_img.data.data.data(), ref_w, 32, 64);
    assert!(ref_ramp > 2, "the reference blur should show a ramp, got {ref_ramp}");

    // Same scene, same authored radius, half the raster.
    let mut half = GpuFilterBackend::new(device.clone(), queue.clone(), full).expect("backend");
    half.set_raster_scale(0.5);
    let half_img = half
        .render_scene_to_image_gpu_filtered(&scene, full, None, &blur_chain(8.0))
        .expect("blurred at half raster");
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
    let naive = unscaled
        .render_scene_to_image_gpu_filtered(&small_scene, small, None, &blur_chain(8.0))
        .expect("naive blur at reduced size");
    let naive_ramp = soft_width(naive.data.data.data(), 32, 16, 32) * 2;
    assert!(
        naive_ramp > ref_ramp + 3,
        "keeping a scene-pixel radius at half the raster must read as a wider \
             blur ({ref_ramp} vs {naive_ramp}) — that is the bug the unit is for"
    );
}

#[test]
fn gpu_filter_blur_softens_a_hard_boundary() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        // Sharp boundary at x=32: left half white, right half empty.
        let mut scene = vello::Scene::new();
        use kurbo::Shape;
        let rect = kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::WHITE,
            None,
            &rect,
        );

        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &blur_chain(8.0))
            .expect("GPU blur path should succeed");
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
}

/// Check that the colour-grade pass mutates pixels: a red rectangle with
/// `saturate: 0` must desaturate to gray.
#[test]
fn color_matrix_actually_desaturates() {
    let maybe_device = pollster::block_on(create_headless_device());
    if let Some((device, queue)) = maybe_device {
        let dims = SceneDimensions {
            width: 64,
            height: 64,
        };
        let mut backend =
            GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

        let mut scene = vello::Scene::new();
        use kurbo::Shape;
        let rect = kurbo::Rect::new(16.0, 16.0, 48.0, 48.0).to_path(1e-3);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            vello::peniko::Color::from_rgba8(255, 0, 0, 255),
            None,
            &rect,
        );

        let chain = color_grade_chain(1.0, 1.0, 0.0, 0.0, 0.0);
        let image = backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("color-grade path should succeed");
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

/// Vignette must darken the corner while leaving the center untouched.
#[test]
fn vignette_darkens_corners_not_center() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("vignette path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 32,
        height: 32,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("levels path should succeed");
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
    let image = backend
        .render_scene_to_image_gpu_filtered(
            &filled_scene(
                vello::peniko::Color::from_rgba8(128, 128, 128, 255),
                kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
            ),
            dims,
            None,
            &chain,
        )
        .expect("levels gamma path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

    // Left half 50% gray, right half black: one hard vertical edge at x=32.
    let mut scene = vello::Scene::new();
    use kurbo::Shape;
    scene.fill(
        vello::peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        vello::peniko::Color::from_rgba8(128, 128, 128, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 32.0, 64.0).to_path(1e-3),
    );
    let chain =
        effect_chain("Sharpen", vec![EffectParamValue::F32(2.0), EffectParamValue::F32(2.0)]);
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("sharpen path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let make_backend = || {
        GpuFilterBackend::new(device.clone(), queue.clone(), dims)
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

    let image = make_backend()
        .render_scene_to_image_gpu_filtered(
            &scene,
            dims,
            None,
            &effect_chain("Grain", params.clone()),
        )
        .expect("grain path should succeed");
    let raw = image.data.data.data();
    let touched = raw.chunks_exact(4).filter(|px| px[0] != 128).count();
    assert!(
        touched > 64,
        "grain should perturb most pixels of a flat field, only {touched} changed"
    );

    let image_again = make_backend()
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &effect_chain("Grain", params))
        .expect("second grain render should succeed");
    assert_eq!(raw, image_again.data.data.data(), "grain must be deterministic");
}

/// MotionBlur with angle 0 must smear horizontally: intermediate alpha
/// appears up to `length` pixels right of the shape's edge.
#[test]
fn motion_blur_smears_horizontally() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

    // Opaque white rect over the left third.
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 21.0, 64.0));
    let chain =
        effect_chain("MotionBlur", vec![EffectParamValue::F32(12.0), EffectParamValue::F32(0.0)]);
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("motion-blur path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("duotone path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("posterize path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("edge path should succeed");
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();

    let boundary = raw[(32 * w + 32) * 4];
    let flat = raw[(32 * w + 50) * 4];
    assert!(boundary > 150, "the step edge must light up, got {boundary}");
    assert!(flat < 40, "a flat region must stay dark, got {flat}");
}

/// LensDistortion must displace samples: a band that survives at the
/// control setting must move out of a near-edge pixel at high `amount`,
/// while the centre (radius 0) is untouched either way.
#[test]
fn lens_distortion_displaces_samples_toward_the_centre() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

    // White band over the leftmost 16 columns.
    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 16.0, 64.0));

    let control = effect_chain("LensDistortion", vec![EffectParamValue::F32(0.0)]);
    let control_image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &control)
        .expect("control path should succeed");
    let cw = control_image.natural_size[0] as usize;
    let control_raw = control_image.data.data.data();
    assert!(control_raw[(32 * cw + 8) * 4] > 200, "control must show the white band at x=8");

    let warped = effect_chain("LensDistortion", vec![EffectParamValue::F32(130.0)]);
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &warped)
        .expect("lens-distortion path should succeed");
    let w = image.natural_size[0] as usize;
    let raw = image.data.data.data();
    assert!(
        raw[(32 * w + 8) * 4] < 60,
        "the warp must pull the band out of the near-edge pixel, got {}",
        raw[(32 * w + 8) * 4]
    );
}

/// DropShadow must paint the silhouette offset behind the content, and
/// leave the content itself untouched.
#[test]
fn drop_shadow_offsets_the_silhouette() {
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

    let scene = filled_scene(vello::peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
    let chain = effect_chain(
        "DropShadow",
        vec![
            EffectParamValue::Vec2([-8.0, 0.0]), // offset (shadow to the left)
            EffectParamValue::F32(0.0),          // softness (hard silhouette)
            EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]), // opaque black shadow
        ],
    );
    let image = backend
        .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
        .expect("drop-shadow path should succeed");
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
    let Some((device, queue)) = pollster::block_on(create_headless_device()) else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };
    let mut backend =
        GpuFilterBackend::new(device, queue, dims).expect("GpuFilterBackend should initialise");

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
        backend
            .render_scene_to_image_gpu_filtered(&scene, dims, None, &chain)
            .expect("drop-shadow path should succeed")
    };
    // The hard shadow's left edge sits at x=16 (rect left 24, offset -8);
    // x=12 is 4px outside it, so only a penumbra can reach it.
    let px = (32 * dims.width as usize + 12) * 4 + 3;

    let hard = shadow_at(0.0);
    assert_eq!(hard.data.data.data()[px], 0, "softness 0 must stay hard: nothing reaches x=12");
    let soft = shadow_at(6.0);
    let alpha = soft.data.data.data()[px];
    assert!(
        (20..200).contains(&alpha),
        "softness 6 must cast a partial-coverage penumbra at x=12, got alpha {alpha}"
    );
}
