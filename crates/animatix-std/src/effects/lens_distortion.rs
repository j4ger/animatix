//! `LensDistortion` — barrel/pincushion UV warp.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `LensDistortion` parameters.
///
/// `amount` is the corner displacement in scene pixels: positive pulls the
/// sampled coordinate toward the centre (pincushion), negative pushes it
/// outward (barrel). Identity `0` leaves the frame untouched.
pub const LENS_DISTORTION_PARAMS: &[EffectParamSpec] = &[EffectParamSpec::new(
    "amount",
    EffectParamKind::F32,
    EffectParamValue::F32(0.0),
    0,
    4,
)];

const LENS_DISTORTION_WGSL: &str = r#"
struct LensDistortionParams {
    amount: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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
@group(0) @binding(2) var<uniform> params: LensDistortionParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let uv = (vec2<f32>(coord) + vec2<f32>(0.5)) * ctx.inv_size;
    let offset = uv - vec2<f32>(0.5);
    // 1.0 at the corners, 0.0 at the centre.
    let radius2 = dot(offset, offset) * 2.0;
    // Scaling by the diagonal keeps the corner displacement within `amount`
    // pixels on both axes, which is what `support` reports.
    let diagonal = max(length(vec2<f32>(size)), 1.0);
    let scale = params.amount * radius2 / diagonal;
    let sample_uv = clamp(vec2<f32>(0.5) + offset * (1.0 - scale), vec2<f32>(0.0), vec2<f32>(1.0));
    let texel = textureSampleLevel(src, samp, sample_uv, 0.0);
    textureStore(dst, coord, texel);
}
"#;

const LENS_DISTORTION_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new(
    "lens-distortion",
    LENS_DISTORTION_WGSL,
    "main",
)];

/// The `LensDistortion` effect.
pub struct LensDistortion;

/// Singleton instance of [`LensDistortion`].
pub const LENS_DISTORTION: LensDistortion = LensDistortion;

impl Effect for LensDistortion {
    fn type_name(&self) -> &str {
        "LensDistortion"
    }

    fn params(&self) -> &[EffectParamSpec] {
        LENS_DISTORTION_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        LENS_DISTORTION_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
    }

    /// Displaces samples; `amount` is the corner displacement in pixels, and
    /// the diagonal scaling keeps the true maximum at `0.707 × amount`.
    fn support(&self, params: &EffectParams) -> f32 {
        params.f32_at(0).abs()
    }
}
