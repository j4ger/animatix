use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams,
};

use super::*;

/// Every parameter the built-in effects author, with the effect's name on
/// it. Adding an effect file without adding its table here is a test
/// failure on purpose: this is the only place the whole set is enumerable,
/// and it is what the raster-scaling guard below reads.
fn all_params() -> Vec<(&'static str, &'static [EffectParamSpec])> {
    vec![
        ("Bloom", bloom::BLOOM_PARAMS),
        ("Blur", blur::BLUR_PARAMS),
        ("ChromaticAberration", chromatic_aberration::CHROMATIC_ABERRATION_PARAMS),
        ("ColorGrade", color_grade::COLOR_GRADE_PARAMS),
        ("DropShadow", drop_shadow::DROP_SHADOW_PARAMS),
        ("Duotone", duotone::DUOTONE_PARAMS),
        ("Edge", edge::EDGE_PARAMS),
        ("Grain", grain::GRAIN_PARAMS),
        ("LensDistortion", lens_distortion::LENS_DISTORTION_PARAMS),
        ("Levels", levels::LEVELS_PARAMS),
        ("MotionBlur", motion_blur::MOTION_BLUR_PARAMS),
        ("Posterize", posterize::POSTERIZE_PARAMS),
        ("Sharpen", sharpen::SHARPEN_PARAMS),
        ("Vignette", vignette::VIGNETTE_PARAMS),
    ]
}

/// Bloom's identity is "do nothing": intensity 0 contributes no light back
/// onto the pre-chain original, so a bare `Bloom` stage is skipped.
#[test]
fn bloom_intensity_identity_is_zero() {
    let intensity = bloom::BLOOM_PARAMS
        .iter()
        .find(|p| p.name == "intensity")
        .expect("Bloom.intensity is gone");
    assert!(
        matches!(
            intensity.identity,
            animatix_core::effect::EffectParamValue::F32(value) if value == 0.0
        ),
        "Bloom.intensity identity must be 0 (got {:?})",
        intensity.identity
    );
}

/// `threshold` is a gate, not an amount: its identity is 0 (the gate is off),
/// it lands at byte 8 of the 16-byte author uniform, and it is dimensionless.
#[test]
fn bloom_threshold_identity_is_zero_at_offset_eight() {
    let threshold = bloom::BLOOM_PARAMS
        .iter()
        .find(|p| p.name == "threshold")
        .expect("Bloom.threshold is gone");
    assert_eq!(threshold.kind, EffectParamKind::F32);
    assert_eq!((threshold.offset, threshold.size), (8, 4));
    assert!(!threshold.is_pixel(), "threshold is a luma fraction, not scene pixels");
    assert!(
        matches!(threshold.identity, EffectParamValue::F32(value) if value == 0.0),
        "Bloom.threshold identity must be 0 (got {:?})",
        threshold.identity
    );
    assert_eq!(
        bloom::BLOOM.author_uniform_size(),
        16,
        "adding threshold must not grow Bloom's author uniform past 16 bytes"
    );
}

/// `pack` has to put every authored value where the WGSL struct reads it. The
/// threshold's home is byte 8, after intensity and keep, inside the same
/// one-vec4 uniform.
#[test]
fn bloom_pack_writes_threshold_at_offset_eight() {
    let params = EffectParams {
        values: vec![
            EffectParamValue::F32(0.8),
            EffectParamValue::F32(1.0),
            EffectParamValue::F32(0.55),
        ],
    };
    let mut out = vec![0xffu8; bloom::BLOOM.author_uniform_size() as usize];
    bloom::BLOOM.pack(&params, &mut out);

    assert_eq!(out.len(), 16);
    assert_eq!(&out[0..4], &0.8f32.to_le_bytes(), "intensity lives at offset 0");
    assert_eq!(&out[4..8], &1.0f32.to_le_bytes(), "keep lives at offset 4");
    assert_eq!(&out[8..12], &0.55f32.to_le_bytes(), "threshold must be written at offset 8");
    assert_eq!(&out[12..16], &[0, 0, 0, 0], "pack must clear the struct padding");
}

/// The threshold has to be enforced in the shader, not merely declared: the
/// glow is gated by the running result's luma through `smoothstep`, and the
/// `select` guard keeps threshold 0 pixel-identical to the old behaviour.
#[test]
fn bloom_shader_gates_the_glow_by_threshold_and_luma() {
    let wgsl = bloom::BLOOM_WGSL;
    assert!(wgsl.contains("threshold: f32"), "BloomParams must declare threshold");
    assert!(wgsl.contains("params.threshold"), "the shader must read params.threshold");
    assert!(
        wgsl.contains("let luma = dot(glow.rgb,"),
        "the gate must measure the glow's luma"
    );
    assert!(
        wgsl.contains("smoothstep(threshold, threshold + 0.1, luma)"),
        "the gate must soften in over the 0.1 luma band above the threshold"
    );
    assert!(
        wgsl.contains("threshold > 0.0"),
        "threshold 0 must bypass the gate and preserve the old bloom"
    );
}

/// A parameter measured in scene pixels has to shrink when the chain runs
/// below scene resolution, or the effect visually grows as the frame
/// shrinks. The renderer scales whatever is marked, so the mark has to be
/// deliberate — this pins the set, and the WGSL of each entry is why it is
/// on the list (`params.radius` / `params.offset` stepping source pixels).
#[test]
fn pixel_unit_parameters_are_exactly_the_known_seven() {
    let mut marked: Vec<String> = all_params()
        .iter()
        .flat_map(|(effect, params)| {
            params
                .iter()
                .filter(|p| p.is_pixel())
                .map(move |p| format!("{effect}.{}", p.name))
        })
        .collect();
    marked.sort();
    assert_eq!(
        marked,
        vec![
            "Blur.radius",
            "ChromaticAberration.offset",
            "DropShadow.offset",
            "DropShadow.softness",
            "LensDistortion.amount",
            "MotionBlur.length",
            "Sharpen.radius",
        ]
    );
}

/// `Vignette.radius` and `Grain.seed` are unitless, so marking them by their
/// name would be wrong — they must stay untouched when the raster shrinks.
#[test]
fn normalized_lookalikes_are_not_marked_as_pixels() {
    for (effect, name) in [("Vignette", "radius"), ("Grain", "seed")] {
        let params = all_params()
            .into_iter()
            .find(|(e, _)| *e == effect)
            .unwrap_or_else(|| panic!("{effect} is not in the table list"))
            .1;
        let spec = params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{effect}.{name} is gone"));
        assert!(!spec.is_pixel(), "{effect}.{name} is a fraction, not scene pixels");
    }
}

/// `EffectContext` is host-owned: one declaration in
/// `effect_context.wgsl`, prepended to every built-in effect shader. A
/// per-file copy would drift from `EffectContextUniform` silently — the
/// shader compiles either way, but the host would write the wrong bytes.
#[test]
fn every_effect_embeds_the_shared_effect_context_once() {
    let shared = include_str!("effect_context.wgsl");
    assert_eq!(
        shared.matches("struct EffectContext").count(),
        1,
        "effect_context.wgsl must define EffectContext exactly once"
    );
    let expected_struct = "\
struct EffectContext {
    tex_size: vec2<u32>,
    _pad0: vec2<u32>,
    inv_size: vec2<f32>,
    _pad1: vec2<f32>,
    pass_index: u32,
    pass_count: u32,
    time_ms: f32,
    _pad2: f32,
}";
    assert!(
        shared.contains(expected_struct),
        "effect_context.wgsl must keep the host EffectContextUniform field layout"
    );

    let shaders = [
        ("Bloom", bloom::BLOOM_WGSL),
        ("Blur", blur::BLUR_WGSL),
        ("ChromaticAberration", chromatic_aberration::CHROMATIC_ABERRATION_WGSL),
        ("ColorGrade", color_grade::COLOR_GRADE_WGSL),
        ("DropShadow", drop_shadow::DROP_SHADOW_WGSL),
        ("Duotone", duotone::DUOTONE_WGSL),
        ("Edge", edge::EDGE_WGSL),
        ("Grain", grain::GRAIN_WGSL),
        ("LensDistortion", lens_distortion::LENS_DISTORTION_WGSL),
        ("Levels", levels::LEVELS_WGSL),
        ("MotionBlur", motion_blur::MOTION_BLUR_WGSL),
        ("Posterize", posterize::POSTERIZE_WGSL),
        ("Sharpen", sharpen::SHARPEN_WGSL),
        ("Vignette", vignette::VIGNETTE_WGSL),
    ];

    assert_eq!(shaders.len(), 14, "the test must cover every built-in effect");
    for (name, wgsl) in shaders {
        assert!(
            wgsl.starts_with(shared),
            "{name} must prepend effect_context.wgsl instead of declaring its own copy"
        );
        assert_eq!(
            wgsl.matches("struct EffectContext").count(),
            1,
            "{name} must contain exactly one EffectContext declaration"
        );
    }
}
