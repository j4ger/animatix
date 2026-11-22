use super::harness::{DIMS, EffectHarness, EffectImage, blur_chain, effect_chain};
use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{
    EffectChain, EffectId, EffectInstance, EffectParamValue, EffectParams, EffectRegion,
};

fn scene_pixel(image: &EffectImage, x: u32, y: u32) -> [u8; 4] {
    super::pixel(&image.rgba, image.size[0], x, y)
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

fn filled_scene(color: peniko::Color, rect: kurbo::Rect) -> animatix::scene::Scene {
    let mut scene = animatix::scene::Scene::new();
    let path = kurbo::Shape::to_path(&rect, 1e-3);
    scene.fill(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, color, None, &path);
    scene
}

fn white_half_scene(height: f64) -> animatix::scene::Scene {
    filled_scene(peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 32.0, height))
}

#[test]
fn filter_region_composites_in_target_pixels_at_half_raster() {
    use animatix::scene::{CompositePlacement, Scene};

    let Some(mut harness) = EffectHarness::new() else {
        return;
    };
    let region = EffectRegion {
        origin: [16.0, 16.0],
        size: SceneDimensions {
            width: 32,
            height: 32,
        },
    };
    for placement in [CompositePlacement::Inline, CompositePlacement::AfterScene] {
        let mut scene = Scene::new();
        scene.push_filter(white_half_scene(64.0), Some(region), blur_chain(8.0), 1.0, placement);
        let image = harness.render(&scene, DIMS, &EffectChain::default(), 0.5);
        assert_eq!(image.size, [32, 32]);
        assert_eq!(scene_pixel(&image, 7, 16), [0; 4], "outside the region");
        assert!(scene_pixel(&image, 10, 16)[3] > 200, "inside the crop");
        assert!((5..128).contains(&scene_pixel(&image, 16, 16)[3]), "soft edge");
    }
}

#[test]
fn blur_radius_keeps_its_scene_width_at_half_raster() {
    let Some(mut harness) = EffectHarness::new() else {
        return;
    };
    let scene = white_half_scene(64.0);
    let ramp_width = |image: EffectImage| {
        let width = image.size[0];
        let row = image.size[1] / 2;
        (0..width)
            .filter(|x| (41..215).contains(&scene_pixel(&image, *x, row)[3]))
            .count()
            * (64 / width) as usize
    };
    let full = ramp_width(harness.render(&scene, DIMS, &blur_chain(8.0), 1.0));
    let half = ramp_width(harness.render(&scene, DIMS, &blur_chain(8.0), 0.5));
    let unscaled = ramp_width(harness.render(&scene, DIMS, &blur_chain(16.0), 0.5));
    assert!(full > 2 && full.abs_diff(half) <= 3, "full {full}, half {half}");
    assert!(unscaled > full + 3, "an unscaled radius would widen the blur: {unscaled}");
}

#[test]
fn lens_distortion_keeps_its_scene_pixel_displacement_at_half_raster() {
    let Some(mut harness) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 16.0, 64.0));
    let chain = effect_chain("LensDistortion", vec![EffectParamValue::F32(24.0)]);
    let edge = |image: EffectImage| {
        let width = image.size[0];
        (0..width)
            .find(|x| scene_pixel(&image, *x, image.size[1] / 2)[3] < 128)
            .expect("distorted band boundary")
            * (64 / width)
    };
    let full = edge(harness.render(&scene, DIMS, &chain, 1.0));
    let half = edge(harness.render(&scene, DIMS, &chain, 0.5));
    assert!((5..20).contains(&full) && full.abs_diff(half) <= 3, "full {full}, half {half}");
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

#[test]
fn chromatic_aberration_fringes_a_hard_edge() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // White half-plane for x < 32 on opaque black keeps the fringe visible.
    let mut scene = filled_scene(peniko::Color::BLACK, kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
    scene.append(&white_half_scene(64.0), None);
    let chain = chromatic_aberration_chain(8.0);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    // Pixel right of the boundary, on the horizontal through the
    // centre: blue samples back into the white half-plane, red
    // samples further out into the empty half.
    let (x, y) = (36usize, 32usize);
    let r = raw[(y * w + x) * 4];
    let b = raw[(y * w + x) * 4 + 2];
    assert!(b > r + 60, "expected a blue-dominant fringe at ({x},{y}), got r={r} b={b}");
}

#[test]
fn blur_softens_a_hard_boundary() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // Sharp boundary at x=32: left half white, right half empty.
    let image = backend.render(&white_half_scene(64.0), DIMS, &blur_chain(8.0), 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    // Independent interior, exterior and guard bands pin propagation, not
    // merely the existence of one partially covered (possibly AA) pixel.
    // These broad alpha ranges intentionally do not freeze a kernel golden.
    let contract = |pixels: &[u8]| -> Result<(), &'static str> {
        let alpha = |x: usize, y: usize| pixels[(y * w + x) * 4 + 3];
        for y in 28..36 {
            for x in 8..16 {
                if alpha(x, y) < 250 {
                    return Err("opaque interior");
                }
            }
            for x in 48..56 {
                if alpha(x, y) > 3 {
                    return Err("far guard");
                }
            }
            let inside: u32 = (28..32).map(|x| u32::from(alpha(x, y))).sum();
            let outside: u32 = (32..36).map(|x| u32::from(alpha(x, y))).sum();
            if !(4 * 128..4 * 250).contains(&inside) {
                return Err("inside softening");
            }
            if !(4 * 5..4 * 128).contains(&outside) {
                return Err("outside spread");
            }
            // Complementary samples around x=32 detect translation and
            // asymmetric propagation; allow filtering/UNORM rounding.
            for distance in 0..8 {
                let sum = u16::from(alpha(31 - distance, y)) + u16::from(alpha(32 + distance, y));
                if sum.abs_diff(255) > 12 {
                    return Err("boundary symmetry");
                }
            }
        }
        Ok(())
    };
    contract(raw).expect("blur spatial contract");

    // Faults derive from the real readback and require the specific oracle
    // to fail. No extra render/adapter is needed for these negative controls.
    let mut clipped = raw.to_vec();
    for y in 0..DIMS.height as usize {
        for x in 32..w {
            clipped[(y * w + x) * 4 + 3] = 0;
        }
    }
    assert_eq!(contract(&clipped), Err("outside spread"));
    let mut hard = raw.to_vec();
    for y in 0..DIMS.height as usize {
        for x in 0..w {
            hard[(y * w + x) * 4 + 3] = if x < 32 { 255 } else { 0 };
        }
    }
    assert_eq!(contract(&hard), Err("inside softening"));
    let mut polluted = raw.to_vec();
    polluted[(30 * w + 50) * 4 + 3] = 255;
    assert_eq!(contract(&polluted), Err("far guard"));
    let mut asymmetric = raw.to_vec();
    asymmetric[(28 * w + 24) * 4 + 3] = 0;
    assert_eq!(contract(&asymmetric), Err("boundary symmetry"));
}

#[test]
fn repeated_effect_instances_keep_distinct_author_uniforms() {
    let Some(mut backend) = EffectHarness::new() else {
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

    let alpha_delta = |left: &EffectImage, right: &EffectImage| -> u64 {
        left.rgba
            .as_slice()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(right.rgba.as_slice().as_chunks::<4>().0)
            .map(|(left, right)| u64::from(left[3].abs_diff(right[3])))
            .sum()
    };

    // The second render reuses the same texture views, buffer, and bind groups
    // while changing slot 0's radius; stale or type-wide author uniforms would
    // make the outputs identical.
    let different = backend.render(&scene, DIMS, &different_radii, 1.0);
    let equal = backend.render(&scene, DIMS, &equal_radii, 1.0);
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
    let grown = backend.render(&scene, DIMS, &three_distinct, 1.0);
    let grown_equal = backend.render(&scene, DIMS, &three_equal, 1.0);
    let growth_delta = alpha_delta(&grown, &grown_equal);
    assert!(
        growth_delta > 64,
        "Blur(2) → Blur(8) → Blur(4) must differ after uniform-buffer growth from three Blur(4) stages, got total alpha delta {growth_delta}"
    );
}

#[test]
fn bloom_adds_the_pre_chain_original_to_the_blur() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    let mut scene = filled_scene(
        peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let rect = filled_scene(
        peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(20.0, 20.0, 44.0, 44.0),
    );
    scene.append(&rect, None);

    let blurred = backend.render(&scene, DIMS, &blur_chain(6.0), 1.0);

    let mut chain = blur_chain(6.0);
    chain.instances.push(EffectInstance {
        id: EffectId::new("Bloom"),
        enabled: true,
        params: EffectParams {
            values: vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)],
        },
    });
    let bloomed = backend.render(&scene, DIMS, &chain, 1.0);

    let w = bloomed.size[0] as usize;
    let (x, y) = (20usize, 32usize);
    let before = blurred.rgba.as_slice()[(y * w + x) * 4];
    let after = bloomed.rgba.as_slice()[(y * w + x) * 4];
    assert!(
        after > before + 20,
        "Bloom must add the pre-chain original at the blurred rect edge, \
         got blur={before} blur+bloom={after}"
    );
}

#[test]
fn bloom_threshold_brightens_only_bright_regions() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // Opaque black canvas with a bright (230) and a dark (60) opaque rect,
    // separated far enough that a small blur cannot mix them.
    let mut scene = filled_scene(
        peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let bright = filled_scene(
        peniko::Color::from_rgba8(230, 230, 230, 255),
        kurbo::Rect::new(6.0, 20.0, 26.0, 44.0),
    );
    let dark = filled_scene(
        peniko::Color::from_rgba8(60, 60, 60, 255),
        kurbo::Rect::new(38.0, 20.0, 58.0, 44.0),
    );
    scene.append(&bright, None);
    scene.append(&dark, None);

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
    let red = |image: &EffectImage, x: usize, y: usize| {
        let w = image.size[0] as usize;
        image.rgba.as_slice()[(y * w + x) * 4]
    };

    let plain = backend.render(&scene, DIMS, &EffectChain::default(), 1.0);
    let (plain_bright, plain_dark) = (red(&plain, 16, 32), red(&plain, 48, 32));

    let gated = backend.render(&scene, DIMS, &blur_bloom_chain(0.6), 1.0);
    let (gated_bright, gated_dark) = (red(&gated, 16, 32), red(&gated, 48, 32));
    assert!(
        gated_bright > plain_bright + 15,
        "threshold 0.6 must still brighten the bright region, got plain={plain_bright} gated={gated_bright}"
    );
    assert!(
        gated_dark.abs_diff(plain_dark) <= 3,
        "threshold 0.6 must leave the dark region alone, got plain={plain_dark} gated={gated_dark}"
    );

    let open = backend.render(&scene, DIMS, &blur_bloom_chain(0.0), 1.0);
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

#[test]
fn bloom_keeps_translucent_saturated_rgb_straight() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let chain =
        effect_chain("Bloom", vec![EffectParamValue::F32(0.25), EffectParamValue::F32(1.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
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
    let reduced_keep_image = backend.render(&scene, DIMS, &reduced_keep_chain, 1.0);
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

#[test]
fn bloom_keeps_partial_glow_rgb_straight() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 128),
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
    let image = backend.render(&scene, DIMS, &chain, 1.0);
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

#[test]
fn bloom_zero_alpha_stores_transparent_black() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(20, 80, 255, 0),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let chain = effect_chain("Bloom", vec![EffectParamValue::F32(1.0), EffectParamValue::F32(1.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    assert_eq!(
        scene_pixel(&image, 28, 32),
        [0, 0, 0, 0],
        "zero-alpha input must not leak color through Bloom"
    );
}

#[test]
fn bloom_clamps_added_alpha_to_opaque() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(0, 255, 0, 128),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let chain = effect_chain("Bloom", vec![EffectParamValue::F32(4.0), EffectParamValue::F32(1.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let pixel = scene_pixel(&image, 32, 32);
    assert_eq!(pixel, [0, 255, 0, 255], "intensity 4 should saturate to opaque green");
}

#[test]
fn bloom_threshold_uses_straight_rgb_for_translucent_pixels() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let mut scene = animatix::scene::Scene::new();
    let white = filled_scene(
        peniko::Color::from_rgba8(255, 255, 255, 64),
        kurbo::Rect::new(6.0, 20.0, 26.0, 44.0),
    );
    let red = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 64),
        kurbo::Rect::new(38.0, 20.0, 58.0, 44.0),
    );
    scene.append(&white, None);
    scene.append(&red, None);
    let chain = effect_chain(
        "Bloom",
        vec![
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(0.6),
        ],
    );
    let image = backend.render(&scene, DIMS, &chain, 1.0);
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

#[test]
fn bloom_uses_running_result_for_glow_and_original_for_keep() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(64, 0, 0, 128),
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
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let pixel = scene_pixel(&image, 32, 32);
    assert!(
        pixel[0].abs_diff(96) <= 3,
        "Bloom must add the doubled running red to the original base in premultiplied space, \
         got {pixel:?}"
    );
    assert_eq!(pixel[3], 255, "two half-alpha contributions should saturate to opaque");
}

#[test]
fn color_matrix_actually_desaturates() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 255),
        kurbo::Rect::new(16.0, 16.0, 48.0, 48.0),
    );
    let chain = color_grade_chain(1.0, 1.0, 0.0, 0.0, 0.0);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    let (r, g, b) =
        (raw[(32 * w + 32) * 4], raw[(32 * w + 32) * 4 + 1], raw[(32 * w + 32) * 4 + 2]);
    // Desaturated red → channels roughly equal (gray).
    assert!(
        r > 30 && (r as i32 - g as i32).abs() < 40 && (r as i32 - b as i32).abs() < 40,
        "saturate=0 should desaturate red to gray (Rec.709 luma ≈54, not empty), got r={r} g={g} b={b}"
    );
}

#[test]
fn vignette_darkens_corners_not_center() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
    let chain = effect_chain(
        "Vignette",
        vec![
            EffectParamValue::F32(1.0),  // amount
            EffectParamValue::F32(0.15), // radius (start close to center)
            EffectParamValue::F32(0.9),  // softness
            EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        ],
    );
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();

    let center = raw[(32 * w + 32) * 4];
    let corner = raw[(2 * w + 2) * 4];
    assert!(center > 240, "center must keep its luminance, got {center}");
    assert!(corner < 120, "corner must fall into the vignette color, got {corner}");
}

#[test]
fn levels_black_point_maps_below_floor_to_black() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let dims = SceneDimensions {
        width: 32,
        height: 32,
    };

    // 25% gray fills the canvas.
    let scene = filled_scene(
        peniko::Color::from_rgba8(64, 64, 64, 255),
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
    let image = backend.render(&scene, dims, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
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
    let image = backend.render(
        &filled_scene(
            peniko::Color::from_rgba8(128, 128, 128, 255),
            kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        ),
        dims,
        &chain,
        1.0,
    );
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    let r = raw[(16 * w + 16) * 4];
    assert!(
        (150..=210).contains(&r),
        "gamma 2 on 0.5 gray must land near 0.707·255 ≈ 180, got {r}"
    );
}

#[test]
fn sharpen_overshoots_at_hard_edges() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // Left half 50% gray, right half black: one hard vertical edge at x=32.
    let scene = filled_scene(
        peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let chain =
        effect_chain("Sharpen", vec![EffectParamValue::F32(2.0), EffectParamValue::F32(2.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    let interior = raw[(32 * w + 8) * 4];
    let edge = raw[(32 * w + 30) * 4];
    assert_eq!(interior, 128, "interior gray must survive sharpening");
    assert!(
        edge > interior + 30,
        "gray side of the edge must overshoot the interior gray, got edge={edge} interior={interior}"
    );
}

#[test]
fn grain_perturbs_flat_field_deterministically() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    let scene = filled_scene(
        peniko::Color::from_rgba8(128, 128, 128, 255),
        kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let params = vec![
        EffectParamValue::F32(0.6), // amount
        EffectParamValue::F32(0.0), // seed
        EffectParamValue::Bool(true),
    ];

    let mut chain = effect_chain("Grain", params);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let raw = image.rgba.as_slice();
    let touched = raw.as_chunks::<4>().0.iter().filter(|px| px[0] != 128).count();
    assert!(
        touched > 64,
        "grain should perturb most pixels of a flat field, only {touched} changed"
    );

    let image_again = backend.render(&scene, DIMS, &chain, 1.0);
    assert_eq!(raw, image_again.rgba.as_slice(), "grain must be deterministic");

    chain.time_ms = 250.0;
    let later = backend.render(&scene, DIMS, &chain, 1.0);
    assert_ne!(raw, later.rgba.as_slice(), "omitting animated must retain temporal grain");

    chain.instances[0].params.values.push(EffectParamValue::Bool(false));
    let static_later = backend.render(&scene, DIMS, &chain, 1.0);
    assert_eq!(raw, static_later.rgba.as_slice(), "static grain uses the time-zero pattern");
}

#[test]
fn motion_blur_smears_horizontally() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // Opaque white rect over the left third.
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 21.0, 64.0));
    let chain =
        effect_chain("MotionBlur", vec![EffectParamValue::F32(12.0), EffectParamValue::F32(0.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
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

#[test]
fn duotone_maps_luma_onto_the_ramp() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let mut scene = filled_scene(
        peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let white = filled_scene(
        peniko::Color::from_rgba8(255, 255, 255, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.append(&white, None);

    let chain = effect_chain(
        "Duotone",
        vec![
            EffectParamValue::F32(1.0),                   // amount
            EffectParamValue::Vec4([0.0, 0.0, 1.0, 1.0]), // shadow: blue
            EffectParamValue::Vec4([1.0, 0.5, 0.0, 1.0]), // highlight: orange
        ],
    );
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();

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

#[test]
fn posterize_quantises_midtones() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let mut scene = filled_scene(
        peniko::Color::from_rgba8(102, 102, 102, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let lighter = filled_scene(
        peniko::Color::from_rgba8(153, 153, 153, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.append(&lighter, None);

    let chain = effect_chain("Posterize", vec![EffectParamValue::F32(2.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();

    let low = raw[(32 * w + 8) * 4];
    let high = raw[(32 * w + 56) * 4];
    assert!(low < 40, "0.4 must quantise down to black, got {low}");
    assert!(high > 200, "0.6 must quantise up to white, got {high}");
}

#[test]
fn edge_lights_the_step_boundary() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let mut scene = filled_scene(
        peniko::Color::from_rgba8(0, 0, 0, 255),
        kurbo::Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    let white = filled_scene(
        peniko::Color::from_rgba8(255, 255, 255, 255),
        kurbo::Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    scene.append(&white, None);

    let chain = effect_chain("Edge", vec![EffectParamValue::F32(1.0), EffectParamValue::F32(0.0)]);
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();

    let boundary = raw[(32 * w + 32) * 4];
    let flat = raw[(32 * w + 50) * 4];
    assert!(boundary > 150, "the step edge must light up, got {boundary}");
    assert!(flat < 40, "a flat region must stay dark, got {flat}");
}

#[test]
fn lens_distortion_displaces_samples_toward_the_centre() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };

    // White band over the leftmost 6 columns.
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(0.0, 0.0, 6.0, 64.0));

    let control = effect_chain("LensDistortion", vec![EffectParamValue::F32(0.0)]);
    let control_image = backend.render(&scene, DIMS, &control, 1.0);
    let cw = control_image.size[0] as usize;
    let control_raw = control_image.rgba.as_slice();
    assert!(control_raw[(32 * cw + 2) * 4] > 200, "control must show the white band at x=2");

    let warped = effect_chain("LensDistortion", vec![EffectParamValue::F32(24.0)]);
    let image = backend.render(&scene, DIMS, &warped, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();
    assert!(
        raw[(32 * w + 2) * 4] < 60,
        "the warp must pull the band out of the near-edge pixel, got {}",
        raw[(32 * w + 2) * 4]
    );
}

#[test]
fn drop_shadow_offsets_the_silhouette() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
    let chain = effect_chain(
        "DropShadow",
        vec![
            EffectParamValue::Vec2([-8.0, 0.0]), // offset (shadow to the left)
            EffectParamValue::F32(0.0),          // softness (hard silhouette)
            EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]), // opaque black shadow
        ],
    );
    let image = backend.render(&scene, DIMS, &chain, 1.0);
    let w = image.size[0] as usize;
    let raw = image.rgba.as_slice();

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

#[test]
fn drop_shadow_softness_spreads_the_silhouette() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(peniko::Color::WHITE, kurbo::Rect::new(24.0, 16.0, 56.0, 48.0));
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
        backend.render(&scene, DIMS, &chain, 1.0)
    };
    // The hard shadow's left edge sits at x=16 (rect left 24, offset -8);
    // x=12 is 4px outside it, so only a penumbra can reach it.
    let px = (32 * DIMS.width as usize + 12) * 4 + 3;

    let hard = shadow_at(0.0);
    assert_eq!(hard.rgba.as_slice()[px], 0, "softness 0 must stay hard: nothing reaches x=12");
    let soft = shadow_at(6.0);
    let alpha = soft.rgba.as_slice()[px];
    assert!(
        (20..200).contains(&alpha),
        "softness 6 must cast a partial-coverage penumbra at x=12, got alpha {alpha}"
    );
}

#[test]
fn drop_shadow_keeps_translucent_colored_shadow_rgb_straight() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let rect = kurbo::Rect::new(20.0, 16.0, 36.0, 48.0);
    let translucent = filled_scene(peniko::Color::from_rgba8(255, 0, 0, 128), rect);
    let chain = blue_drop_shadow_chain([8.0, 0.0], 0.0);
    let image = backend.render(&translucent, DIMS, &chain, 1.0);
    let shadow_only = scene_pixel(&image, 40, 32);
    assert_eq!(
        shadow_only[3], 128,
        "half-alpha silhouette should cast half-alpha shadow: {shadow_only:?}"
    );
    assert!(
        shadow_only[2] >= 248 && shadow_only[0] <= 8 && shadow_only[1] <= 8,
        "shadow-only RGB must remain straight blue instead of being scaled by alpha: {shadow_only:?}"
    );

    let opaque = filled_scene(peniko::Color::WHITE, rect);
    let opaque_image = backend.render(&opaque, DIMS, &chain, 1.0);
    let opaque_shadow = scene_pixel(&opaque_image, 40, 32);
    assert_eq!(opaque_shadow, [0, 0, 255, 255], "opaque shadow output must remain unchanged");

    // Exercise the shadow-color alpha independently of source transparency.
    let half_opacity_chain = blue_drop_shadow_chain_with_alpha([8.0, 0.0], 0.0, 0.5);
    let half_opacity_image = backend.render(&opaque, DIMS, &half_opacity_chain, 1.0);
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

#[test]
fn drop_shadow_composites_translucent_content_over_shadow() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let image = backend.render(&scene, DIMS, &blue_drop_shadow_chain([8.0, 0.0], 0.0), 1.0);
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

#[test]
fn drop_shadow_zero_alpha_input_does_not_leak_color() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(20, 80, 255, 0),
        kurbo::Rect::new(20.0, 16.0, 36.0, 48.0),
    );
    let image = backend.render(&scene, DIMS, &blue_drop_shadow_chain([8.0, 0.0], 0.0), 1.0);
    for (x, y) in [(28, 32), (40, 32)] {
        let pixel = scene_pixel(&image, x, y);
        assert_eq!(
            pixel,
            [0, 0, 0, 0],
            "zero-alpha input must stay transparent black at ({x},{y}), got {pixel:?}"
        );
    }
}

#[test]
fn drop_shadow_soft_penumbra_keeps_colored_rgb_straight() {
    let Some(mut backend) = EffectHarness::new() else {
        return;
    };
    let scene = filled_scene(
        peniko::Color::from_rgba8(255, 0, 0, 128),
        kurbo::Rect::new(24.0, 16.0, 56.0, 48.0),
    );
    let image = backend.render(&scene, DIMS, &blue_drop_shadow_chain([-8.0, 0.0], 6.0), 1.0);
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
