//! `Grain` — film-grain noise with an optional timeline clock.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use crate::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Grain` parameters.
///
/// `amount` is the ± deviation around the mid-grey noise term (0.25 at the
/// canonical film-grain strength, clamped so the noise never inverts a channel
/// by more than its headroom); `seed` shifts the noise field; `monochrome`
/// applies one noise sample to all channels. `animated` selects between a
/// timeline-driven pattern and a fixed pattern. `size` is the noise spacing in
/// raster pixels.
pub const GRAIN_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("seed", EffectParamKind::F32, EffectParamValue::F32(0.0), 4, 4),
    EffectParamSpec::new("monochrome", EffectParamKind::Bool, EffectParamValue::Bool(false), 8, 4),
    EffectParamSpec::new("animated", EffectParamKind::Bool, EffectParamValue::Bool(true), 12, 4),
    EffectParamSpec::new("size", EffectParamKind::F32, EffectParamValue::F32(1.0), 16, 4),
];

pub(crate) const GRAIN_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    include_str!("grain_common.wgsl"),
    r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: GrainParams;
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
    textureStore(dst, coord, apply_grain(texel, coord, params, ctx));
}
"#
);

const GRAIN_FRAGMENT_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    include_str!("fullscreen_vertex.wgsl"),
    include_str!("grain_common.wgsl"),
    include_str!("portable/grain.wgsl"),
);

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

    fn fragment_shader(&self) -> Option<&'static str> {
        Some(GRAIN_FRAGMENT_WGSL)
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
        let animated =
            u32::from(!matches!(params.values.get(3), Some(EffectParamValue::Bool(false))));
        out[12..16].copy_from_slice(&animated.to_le_bytes());
        out[16..20].copy_from_slice(&params.f32_at(4).max(1.0).to_le_bytes());
    }

    fn is_identity(&self, params: &EffectParams) -> bool {
        params.f32_at(0) <= 0.0
    }

    /// Reads only the pixel itself.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
