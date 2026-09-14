//! `Duotone` — maps luminance onto a two-colour ramp.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Duotone` parameters.
///
/// `shadow` is the colour at luminance 0 and `highlight` the colour at
/// luminance 1; `amount` blends the ramp back over the original, so an
/// unauthored stage (all identities) contributes nothing.
pub const DUOTONE_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    // vec4 lands on its own 16-byte line per the host packing rule.
    EffectParamSpec::new(
        "shadow",
        EffectParamKind::Vec4,
        EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        16,
        16,
    ),
    EffectParamSpec::new(
        "highlight",
        EffectParamKind::Vec4,
        EffectParamValue::Vec4([1.0, 1.0, 1.0, 1.0]),
        32,
        16,
    ),
];

const DUOTONE_WGSL: &str = r#"
struct DuotoneParams {
    amount: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    shadow: vec4<f32>,
    highlight: vec4<f32>,
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
@group(0) @binding(2) var<uniform> params: DuotoneParams;
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
    // Rec. 709 luma, matching the perceptual weights the other effects use.
    let luma = clamp(dot(texel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let mapped = mix(params.shadow.rgb, params.highlight.rgb, luma);
    let weight = clamp(params.amount, 0.0, 1.0);
    textureStore(dst, coord, vec4<f32>(mix(texel.rgb, mapped, weight), texel.a));
}
"#;

const DUOTONE_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("duotone", DUOTONE_WGSL, "main")];

/// The `Duotone` effect.
pub struct Duotone;

/// Singleton instance of [`Duotone`].
pub const DUOTONE: Duotone = Duotone;

impl Effect for Duotone {
    fn type_name(&self) -> &str {
        "Duotone"
    }

    fn params(&self) -> &[EffectParamSpec] {
        DUOTONE_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        DUOTONE_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        for (index, base) in [1usize, 2].iter().zip([16usize, 32]) {
            if let Some(EffectParamValue::Vec4(color)) = params.values.get(*index) {
                for (channel, value) in color.iter().enumerate() {
                    let at = base + channel * 4;
                    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
