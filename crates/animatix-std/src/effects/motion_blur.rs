//! `MotionBlur` — directional smear along an authored vector, sampled with the
//! linear sampler for sub-pixel offsets.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `MotionBlur` parameters.
///
/// `length` is the smear extent in scene pixels; `angle` is measured in
/// degrees, 0 = to the right, counter-clockwise (y-up scene convention).
pub const MOTION_BLUR_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("length", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4).pixel(),
    EffectParamSpec::new("angle", EffectParamKind::F32, EffectParamValue::F32(0.0), 4, 4),
];

/// Sample-count cap: the tap count is `length` (1 tap per pixel of smear), so
/// this bounds a single pass's work. Beyond ~64 px a chain should use several
/// shorter `MotionBlur` stages or the bloom-style two-pass design.
const MAX_TAPS: i32 = 64;

const MOTION_BLUR_WGSL: &str = r#"
struct MotionBlurParams {
    length: f32,
    angle: f32,
    _pad0: f32,
    _pad1: f32,
}

struct EffectContext {
    tex_size: vec2<u32>,
    _pad0: vec2<u32>,
    inv_size: vec2<f32>,
    _pad1: vec2<f32>,
    pass_index: u32,
    pass_count: u32,
    time_ms: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: MotionBlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let taps = i32(clamp(params.length, 0.0, 64.0));
    if (taps <= 1) {
        textureStore(dst, coord, texel);
        return;
    }

    // Scene space is y-up; texture UV space is y-down, hence the sign flip.
    let radians = params.angle * 0.017453292519943295;
    let step_uv = vec2<f32>(cos(radians), -sin(radians)) / vec2<f32>(size);

    // Sub-pixel offsets need the linear sampler; textureSample is
    // fragment-only, so sample with an explicit level.
    let center_uv = (vec2<f32>(coord) + vec2<f32>(0.5)) / vec2<f32>(size);
    var total = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    let half_taps = f32(taps) * 0.5;
    // i32 loop bound: taps is already clamped, so the loop is bounded.
    for (var i = 0; i < taps; i = i + 1) {
        let t = (f32(i) + 0.5 - half_taps);
        total = total + textureSampleLevel(src, samp, center_uv + step_uv * t, 0.0);
    }
    let blurred = total / f32(taps);

    // Alpha keeps its maximum so the smear never erodes the silhouette.
    let out = vec4<f32>(blurred.rgb, max(texel.a, blurred.a));
    textureStore(dst, coord, out);
}
"#;

const MOTION_BLUR_PASSES: &[EffectPassSpec] =
    &[EffectPassSpec::new("motion-blur", MOTION_BLUR_WGSL, "main")];

/// The `MotionBlur` effect.
pub struct MotionBlur;

/// Singleton instance of [`MotionBlur`].
pub const MOTION_BLUR: MotionBlur = MotionBlur;

impl Effect for MotionBlur {
    fn type_name(&self) -> &str {
        "MotionBlur"
    }

    fn params(&self) -> &[EffectParamSpec] {
        MOTION_BLUR_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        MOTION_BLUR_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
    }

    /// Reads up to `length` pixels along the smear direction.
    fn support(&self, params: &EffectParams) -> f32 {
        params.f32_at(0).clamp(0.0, MAX_TAPS as f32)
    }
}
