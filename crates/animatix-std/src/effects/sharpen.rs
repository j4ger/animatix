//! `Sharpen` — unsharp mask: adds the local high-frequency residual back.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Sharpen` parameters.
///
/// `radius` is the box-blur radius in texels for the local-average term;
/// `amount` is how much of the residual (pixel − local average) is added back.
pub const SHARPEN_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("radius", EffectParamKind::F32, EffectParamValue::F32(0.0), 4, 4).pixel(),
];

/// Kernel radii beyond this are clamped: a (2·32+1)² box kernel is already far
/// past anything sharpening means, and unbounded loops stall the GPU.
const MAX_KERNEL_RADIUS: i32 = 32;

const SHARPEN_WGSL: &str = r#"
struct SharpenParams {
    amount: f32,
    radius: f32,
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
@group(0) @binding(2) var<uniform> params: SharpenParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let size = ctx.tex_size;

    if (coord.x < 0 || coord.y < 0 || coord.x >= i32(size.x) || coord.y >= i32(size.y)) {
        return;
    }

    let texel = textureLoad(src, coord, 0);
    let radius = i32(clamp(params.radius, 0.0, 32.0));
    let amount = params.amount;

    if (radius == 0 || amount == 0.0) {
        textureStore(dst, vec2<u32>(coord), texel);
        return;
    }

    // Box average over the (2r+1)² neighbourhood, clamped at the edges.
    var total = vec3<f32>(0.0, 0.0, 0.0);
    var samples = 0.0;
    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        for (var dx = -radius; dx <= radius; dx = dx + 1) {
            let p = clamp(coord + vec2<i32>(dx, dy), vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
            total = total + textureLoad(src, p, 0).rgb;
            samples = samples + 1.0;
        }
    }
    let average = total / samples;

    // Unsharp mask: add the high-frequency residual back.
    let sharpened = texel.rgb + (texel.rgb - average) * amount;
    textureStore(
        dst,
        vec2<u32>(coord),
        vec4<f32>(clamp(sharpened, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a),
    );
}
"#;

const SHARPEN_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("sharpen", SHARPEN_WGSL, "main")];

/// The `Sharpen` effect.
pub struct Sharpen;

/// Singleton instance of [`Sharpen`].
pub const SHARPEN: Sharpen = Sharpen;

impl Effect for Sharpen {
    fn type_name(&self) -> &str {
        "Sharpen"
    }

    fn params(&self) -> &[EffectParamSpec] {
        SHARPEN_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        SHARPEN_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
    }

    /// Reads a ±radius box around the pixel.
    fn support(&self, params: &EffectParams) -> f32 {
        params.f32_at(1).clamp(0.0, MAX_KERNEL_RADIUS as f32)
    }
}
