//! `Grain` — animated film-grain noise, `time_ms`-driven so it animates for
//! free.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Grain` parameters.
///
/// `amount` is the ± deviation around the mid-grey noise term (0.25 at the
/// canonical film-grain strength, clamped so the noise never inverts a channel
/// by more than its headroom); `seed` shifts the noise field; `monochrome`
/// applies one noise sample to all channels instead of independent per-channel
/// grain.
pub const GRAIN_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("seed", EffectParamKind::F32, EffectParamValue::F32(0.0), 4, 4),
    EffectParamSpec::new("monochrome", EffectParamKind::Bool, EffectParamValue::Bool(false), 8, 4),
];

const GRAIN_WGSL: &str = r#"
struct GrainParams {
    amount: f32,
    seed: f32,
    monochrome: u32,
    _pad0: f32,
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
@group(0) @binding(2) var<uniform> params: GrainParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

// PCG-ish integer hash: well-distributed, deterministic for a given
// (pixel, seed, time) triple, no state.
fn hash32(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    let shifted = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (shifted >> 22u) ^ shifted;
    return h;
}

fn unit_noise(x: u32) -> f32 {
    return f32(hash32(x) & 0x00ffffffu) / f32(0x01000000u);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let amount = clamp(params.amount, 0.0, 1.0);
    if (amount <= 0.0) {
        textureStore(dst, coord, texel);
        return;
    }

    // The frame index comes from time_ms so grain animates without a new
    // parameter; 60 fps frames stay distinct for ~1.1 years of timeline.
    let frame = u32(ctx.time_ms * 0.06);
    let pixel = coord.x + coord.y * size.x;
    let salt = u32(params.seed * 4096.0);

    if (params.monochrome != 0u) {
        let n = unit_noise(pixel ^ (frame << 12u) ^ salt) - 0.5;
        let rgb = texel.rgb + vec3<f32>(n * amount);
        textureStore(dst, coord, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a));
        return;
    }

    let r = unit_noise(pixel ^ (frame << 12u) ^ salt);
    let g = unit_noise(pixel ^ (frame << 13u) ^ (salt + 7919u));
    let b = unit_noise(pixel ^ (frame << 14u) ^ (salt + 15485u));
    let rgb = texel.rgb + (vec3<f32>(r, g, b) - vec3<f32>(0.5)) * amount;
    textureStore(dst, coord, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a));
}
"#;

const GRAIN_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("grain", GRAIN_WGSL, "main")];

/// The `Grain` effect.
pub struct Grain;

/// Singleton instance of [`Grain`].
pub const GRAIN: Grain = Grain;

impl Effect for Grain {
    fn type_name(&self) -> &str {
        "Grain"
    }

    fn params(&self) -> &[EffectParamSpec] {
        GRAIN_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        GRAIN_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
        // Bools marshal as u32 (§4.4).
        let flag = match params.values.get(2) {
            Some(EffectParamValue::Bool(true)) => 1u32,
            _ => 0,
        };
        out[8..12].copy_from_slice(&flag.to_le_bytes());
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
