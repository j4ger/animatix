//! `Posterize` — quantises each channel to a fixed number of levels.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Posterize` parameters.
///
/// `levels` is the number of steps per channel (`2` = hard threshold);
/// the identity `0` means "no posterisation", so an unauthored stage skips.
pub const POSTERIZE_PARAMS: &[EffectParamSpec] = &[EffectParamSpec::new(
    "levels",
    EffectParamKind::F32,
    EffectParamValue::F32(0.0),
    0,
    4,
)];

const POSTERIZE_WGSL: &str = r#"
struct PosterizeParams {
    levels: f32,
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
@group(0) @binding(2) var<uniform> params: PosterizeParams;
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
    // `levels` steps means `levels - 1` intervals, so the endpoints 0 and 1
    // survive quantisation exactly.
    let steps = max(params.levels - 1.0, 1.0);
    let rgb = round(texel.rgb * steps) / steps;
    textureStore(dst, coord, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a));
}
"#;

const POSTERIZE_PASSES: &[EffectPassSpec] =
    &[EffectPassSpec::new("posterize", POSTERIZE_WGSL, "main")];

/// The `Posterize` effect.
pub struct Posterize;

/// Singleton instance of [`Posterize`].
pub const POSTERIZE: Posterize = Posterize;

impl Effect for Posterize {
    fn type_name(&self) -> &str {
        "Posterize"
    }

    fn params(&self) -> &[EffectParamSpec] {
        POSTERIZE_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        POSTERIZE_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
