//! `ChromaticAberration` — radial RGB channel separation through the linear
//! sampler.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `ChromaticAberration` parameters.
pub const CHROMATIC_ABERRATION_PARAMS: &[EffectParamSpec] = &[EffectParamSpec::new(
    "offset",
    EffectParamKind::F32,
    EffectParamValue::F32(0.0),
    0,
    4,
)];

const CHROMATIC_ABERRATION_WGSL: &str = r#"
struct ChromaOffsetParams {
    offset: f32,
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
@group(0) @binding(2) var<uniform> params: ChromaOffsetParams;
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

    if (params.offset < 0.25) {
        textureStore(dst, coord, texel);
        return;
    }

    let size_f = vec2<f32>(size);
    let center = size_f * 0.5;
    let pos = vec2<f32>(coord);
    let to_pixel = pos - center;
    let dir = to_pixel / max(length(to_pixel), 1.0);
    let duv = dir * params.offset / size_f;
    let uv = (pos + vec2<f32>(0.5)) / size_f;

    // Sub-pixel channel offsets need the linear sampler, so sample with an
    // explicit level (textureSample is fragment-only).
    let r = textureSampleLevel(src, samp, uv + duv, 0.0).r;
    let g = textureSampleLevel(src, samp, uv, 0.0).g;
    let b = textureSampleLevel(src, samp, uv - duv, 0.0).b;

    textureStore(dst, coord, vec4<f32>(r, g, b, texel.a));
}
"#;

const CHROMATIC_ABERRATION_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new(
    "chromatic-aberration",
    CHROMATIC_ABERRATION_WGSL,
    "main",
)];

/// The `ChromaticAberration` effect.
pub struct ChromaticAberration;

/// Singleton instance of [`ChromaticAberration`].
pub const CHROMATIC_ABERRATION: ChromaticAberration = ChromaticAberration;

impl Effect for ChromaticAberration {
    fn type_name(&self) -> &str {
        "ChromaticAberration"
    }

    fn display_name(&self) -> &'static str {
        "Chromatic Aberration"
    }

    fn params(&self) -> &[EffectParamSpec] {
        CHROMATIC_ABERRATION_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        CHROMATIC_ABERRATION_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
    }

    fn support(&self, params: &EffectParams) -> f32 {
        params.f32_at(0).max(0.0)
    }
}
