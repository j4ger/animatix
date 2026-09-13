//! `Levels` — black/white point remap with gamma.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Levels` parameters.
///
/// Per channel: `out = out_black + clamp((c - in_black) / (in_white -
/// in_black), 0, 1)^(1/gamma) * (out_white - out_black)`. `in_white ==
/// in_black` (a degenerate range) maps everything to `out_black`.
pub const LEVELS_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("in_black", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("in_white", EffectParamKind::F32, EffectParamValue::F32(1.0), 4, 4),
    EffectParamSpec::new("gamma", EffectParamKind::F32, EffectParamValue::F32(1.0), 8, 4),
    EffectParamSpec::new("out_black", EffectParamKind::F32, EffectParamValue::F32(0.0), 12, 4),
    EffectParamSpec::new("out_white", EffectParamKind::F32, EffectParamValue::F32(1.0), 16, 4),
];

const LEVELS_WGSL: &str = r#"
struct LevelsParams {
    in_black: f32,
    in_white: f32,
    gamma: f32,
    out_black: f32,
    out_white: f32,
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
@group(0) @binding(2) var<uniform> params: LevelsParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

fn remap(c: f32) -> f32 {
    let span = params.in_white - params.in_black;
    if (abs(span) < 0.0001) {
        return params.out_black;
    }
    let x = clamp((c - params.in_black) / span, 0.0, 1.0);
    let gamma = max(params.gamma, 0.001);
    return params.out_black + pow(x, 1.0 / gamma) * (params.out_white - params.out_black);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let out = vec4<f32>(
        clamp(remap(texel.r), 0.0, 1.0),
        clamp(remap(texel.g), 0.0, 1.0),
        clamp(remap(texel.b), 0.0, 1.0),
        texel.a,
    );
    textureStore(dst, coord, out);
}
"#;

const LEVELS_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("levels", LEVELS_WGSL, "main")];

/// The `Levels` effect.
pub struct Levels;

/// Singleton instance of [`Levels`].
pub const LEVELS: Levels = Levels;

impl Effect for Levels {
    fn type_name(&self) -> &str {
        "Levels"
    }

    fn params(&self) -> &[EffectParamSpec] {
        LEVELS_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        LEVELS_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        for (i, slot) in [0, 4, 8, 12, 16].iter().enumerate() {
            let at = *slot as usize;
            out[at..at + 4].copy_from_slice(&params.f32_at(i).to_le_bytes());
        }
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
