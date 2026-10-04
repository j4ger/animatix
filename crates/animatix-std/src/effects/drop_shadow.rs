//! `DropShadow` — offsets a copy of the silhouette behind the content.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.
//!
//! `softness` spreads the silhouette over a disc of taps. `softness: 0` is the
//! original hard shadow, unchanged down to the bit: the radius-zero case keeps
//! its own single-sample branch rather than relying on an average of identical
//! taps happening to round back to the same value.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `DropShadow` parameters.
///
/// `offset` shifts the shadow copy in scene pixels (positive x right,
/// positive y down). `softness` spreads that copy over a disc of the given
/// radius, which is what makes a shadow read as cast rather than pasted.
/// `color` carries the shadow colour *and* its strength in the alpha channel.
/// A zero offset is the identity, so an unauthored stage contributes nothing.
pub const DROP_SHADOW_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("offset", EffectParamKind::Vec2, EffectParamValue::Vec2([0.0, 0.0]), 0, 8),
    // Spread of the silhouette in scene px. Lands in the word the layout used
    // to pad, so `color` keeps its own 16-byte line.
    EffectParamSpec::new("softness", EffectParamKind::F32, EffectParamValue::F32(0.0), 8, 4),
    // vec4 lands on its own 16-byte line per the host packing rule.
    EffectParamSpec::new(
        "color",
        EffectParamKind::Vec4,
        EffectParamValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        16,
        16,
    ),
];

const DROP_SHADOW_WGSL: &str = r#"
struct DropShadowParams {
    offset: vec2<f32>,
    softness: f32,
    _pad0: f32,
    color: vec4<f32>,
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
@group(0) @binding(2) var<uniform> params: DropShadowParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    // The silhouette is the source alpha read at the shadow's offset, spread
    // over a disc of `softness` px. Sixteen golden-angle taps average to a
    // penumbra.
    let shadow_coord = vec2<i32>(coord) - vec2<i32>(params.offset);
    let radius = max(params.softness, 0.0);
    var shadow_cover = 0.0;
    if (radius <= 0.0) {
        // The hard case takes the single sample it always took. Averaging
        // sixteen identical taps would be the same number to within an
        // accumulation rounding, and "within" is not "identical" — existing
        // scenes must not shift a shade.
        if (shadow_coord.x >= 0 && shadow_coord.y >= 0
            && shadow_coord.x < i32(size.x) && shadow_coord.y < i32(size.y)) {
            shadow_cover = textureLoad(src, shadow_coord, 0).a;
        }
    } else {
        let taps: u32 = 16u;
        var shadow_sum = 0.0;
        for (var i: u32 = 0u; i < taps; i = i + 1u) {
            let f = (f32(i) + 0.5) / f32(taps);
            let angle = f32(i) * 2.399963229728653;
            let reach = sqrt(f) * radius;
            let tap = shadow_coord + vec2<i32>(vec2<f32>(cos(angle), sin(angle)) * reach);
            if (tap.x >= 0 && tap.y >= 0 && tap.x < i32(size.x) && tap.y < i32(size.y)) {
                shadow_sum = shadow_sum + textureLoad(src, tap, 0).a;
            }
        }
        shadow_cover = shadow_sum / f32(taps);
    }
    let shadow_alpha = shadow_cover * params.color.a;

    // The render target is premultiplied, so compose "original over shadow"
    // in premultiplied space and keep the result premultiplied.
    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let shadow_rgb = params.color.rgb * shadow_alpha;
    let out_alpha = texel.a + shadow_alpha * (1.0 - texel.a);
    let out_rgb = texel.rgb + shadow_rgb * (1.0 - texel.a);
    textureStore(dst, coord, vec4<f32>(out_rgb, out_alpha));
}
"#;

const DROP_SHADOW_PASSES: &[EffectPassSpec] =
    &[EffectPassSpec::new("drop-shadow", DROP_SHADOW_WGSL, "main")];

/// The `DropShadow` effect.
pub struct DropShadow;

/// Singleton instance of [`DropShadow`].
pub const DROP_SHADOW: DropShadow = DropShadow;

impl Effect for DropShadow {
    fn type_name(&self) -> &str {
        "DropShadow"
    }

    fn params(&self) -> &[EffectParamSpec] {
        DROP_SHADOW_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        DROP_SHADOW_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        if let Some(EffectParamValue::Vec2(offset)) = params.values.first() {
            out[0..4].copy_from_slice(&offset[0].to_le_bytes());
            out[4..8].copy_from_slice(&offset[1].to_le_bytes());
        }
        if let Some(EffectParamValue::F32(softness)) = params.values.get(1) {
            out[8..12].copy_from_slice(&softness.to_le_bytes());
        }
        if let Some(EffectParamValue::Vec4(color)) = params.values.get(2) {
            for (channel, value) in color.iter().enumerate() {
                let at = 16 + channel * 4;
                out[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    }

    /// The shadow samples the silhouette at a fixed offset, spread by
    /// `softness`, so the region has to grow by both.
    fn support(&self, params: &EffectParams) -> f32 {
        let offset = match params.values.first() {
            Some(EffectParamValue::Vec2(offset)) => {
                (offset[0] * offset[0] + offset[1] * offset[1]).sqrt()
            },
            _ => 0.0,
        };
        let softness = match params.values.get(1) {
            Some(EffectParamValue::F32(softness)) => softness.max(0.0),
            _ => 0.0,
        };
        offset + softness
    }
}
