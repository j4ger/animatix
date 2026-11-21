//! `Vignette` — darkens (or tints) the frame toward its edges.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use crate::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

const DEFAULT_CENTER: [f32; 2] = [0.5, 0.5];

/// `Vignette` parameters.
///
/// `center` is a pair of fractions of the effect texture's width and height,
/// measured from the top left. `radius` and `softness` are fractions of the
/// shorter side: `radius` starts the falloff and `softness` extends it outward.
pub const VIGNETTE_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("radius", EffectParamKind::F32, EffectParamValue::F32(0.9), 4, 4),
    EffectParamSpec::new("softness", EffectParamKind::F32, EffectParamValue::F32(0.6), 8, 4),
    // vec4 lands on its own 16-byte line per the host packing rule.
    EffectParamSpec::new(
        "color",
        EffectParamKind::Vec4,
        EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        16,
        16,
    ),
    EffectParamSpec::new(
        "center",
        EffectParamKind::Vec2,
        EffectParamValue::Vec2(DEFAULT_CENTER),
        32,
        8,
    ),
];

pub(crate) const VIGNETTE_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    r#"
struct VignetteParams {
    amount: f32,
    radius: f32,
    softness: f32,
    _pad0: f32,
    color: vec4<f32>,
    center: vec2<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: VignetteParams;
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

    let center = vec2<f32>(size) * params.center;
    let p = (vec2<f32>(coord) - center) / min(f32(size.x), f32(size.y));
    let dist = length(p);

    let inner = max(params.radius, 0.0);
    let outer = max(inner + max(params.softness, 0.001), inner + 0.001);
    let t = smoothstep(inner, outer, dist);
    let weight = clamp(params.amount, 0.0, 1.0) * t;

    let rgb = mix(texel.rgb, params.color.rgb, weight);
    textureStore(dst, coord, vec4<f32>(rgb, texel.a));
}
"#
);

const VIGNETTE_PASSES: &[EffectPassSpec] =
    &[EffectPassSpec::new("vignette", VIGNETTE_WGSL, "main")];

/// The `Vignette` effect.
pub struct Vignette;

/// Singleton instance of [`Vignette`].
pub const VIGNETTE: Vignette = Vignette;

impl Effect for Vignette {
    fn type_name(&self) -> &str {
        "Vignette"
    }

    fn params(&self) -> &[EffectParamSpec] {
        VIGNETTE_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        VIGNETTE_PASSES
    }

    fn fragment_shader(&self) -> Option<&'static str> {
        Some(super::portable::VIGNETTE_FRAGMENT_WGSL)
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
        out[8..12].copy_from_slice(&params.f32_at(2).to_le_bytes());
        if let Some(EffectParamValue::Vec4(color)) = params.values.get(3) {
            let base = 16;
            for (i, channel) in color.iter().enumerate() {
                let at = base + i * 4;
                out[at..at + 4].copy_from_slice(&channel.to_le_bytes());
            }
        }
        let center = match params.values.get(4) {
            Some(EffectParamValue::Vec2(center)) => *center,
            _ => DEFAULT_CENTER, // Older packed callers may omit the new parameter.
        };
        out[32..36].copy_from_slice(&center[0].to_le_bytes());
        out[36..40].copy_from_slice(&center[1].to_le_bytes());
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
