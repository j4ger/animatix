//! `MotionBlur` — directional smear along an authored vector, with an
//! alpha-correct linear footprint for sub-pixel offsets.
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

pub(crate) const MOTION_BLUR_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    r#"
struct MotionBlurParams {
    length: f32,
    angle: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: MotionBlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

fn premultiplied_texel(coord: vec2<i32>) -> vec4<f32> {
    let color = textureLoad(src, coord, 0);
    return vec4<f32>(color.rgb * color.a, color.a);
}

// Reproduce the linear sampler footprint with clamped texel loads, but
// premultiply each source texel before interpolation. Sampling straight RGBA
// first would allow transparent RGB to contaminate the smear.
fn bilinear_premultiplied(position: vec2<f32>) -> vec4<f32> {
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    let size = vec2<i32>(ctx.tex_size);
    let max_coord = size - vec2<i32>(1);
    let p00 = clamp(base, vec2<i32>(0), max_coord);

    if (all(fraction == vec2<f32>(0.0))) {
        return premultiplied_texel(p00);
    }

    if (fraction.y == 0.0) {
        let p10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
        return mix(premultiplied_texel(p00), premultiplied_texel(p10), fraction.x);
    }

    if (fraction.x == 0.0) {
        let p01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
        return mix(premultiplied_texel(p00), premultiplied_texel(p01), fraction.y);
    }

    let p10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
    let p01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
    let p11 = clamp(base + vec2<i32>(1, 1), vec2<i32>(0), max_coord);
    let top = mix(premultiplied_texel(p00), premultiplied_texel(p10), fraction.x);
    let bottom = mix(premultiplied_texel(p01), premultiplied_texel(p11), fraction.x);
    return mix(top, bottom, fraction.y);
}

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
    let step_direction = vec2<f32>(cos(radians), -sin(radians));

    // Reconstruct the linear-sampler footprint at each sub-pixel offset. The
    // helper premultiplies each source texel before interpolation.
    let center = vec2<f32>(coord);
    var total_premultiplied = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    let half_taps = f32(taps) * 0.5;
    // i32 loop bound: taps is already clamped, so the loop is bounded.
    for (var i = 0; i < taps; i = i + 1) {
        let t = (f32(i) + 0.5 - half_taps);
        total_premultiplied = total_premultiplied
            + bilinear_premultiplied(center + step_direction * t);
    }
    let blurred = total_premultiplied / f32(taps);

    // Alpha keeps its maximum so the smear never erodes the silhouette.
    var straight_rgb = vec3<f32>(0.0);
    if (blurred.a > 0.0) {
        straight_rgb = blurred.rgb / blurred.a;
    }
    let out = vec4<f32>(straight_rgb, max(texel.a, blurred.a));
    textureStore(dst, coord, out);
}
"#
);

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
