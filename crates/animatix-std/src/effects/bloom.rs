//! `Bloom` — add the chain's current state back onto the pre-chain original.
//!
//! This is the first consumer of the ABI v2 second input (§4.2 binding 5): a
//! pass that reads *two* textures. The chain has always handed a pass only the
//! running result, so a glow could replace the frame or blur it, but never
//! brighten the frame with it.
//!
//! Bloom is therefore a *composite*, not a whole effect. Its identity is
//! `intensity: 0` (with `keep` at 1), so a bare `Bloom` contributes nothing and
//! is skipped. A positive `intensity` adds the pre-chain original back onto the
//! running result — an exposure lift when Bloom runs alone — and `Blur` followed
//! by `Bloom` is the glow: the blurred copy is added over the untouched frame.
//! `threshold` gates that addition by the running result's luma: at its identity
//! 0 the gate is off and the historical bloom behaviour is unchanged; above 0
//! only pixels brighter than the threshold receive the added light.
//!
//! ```amx
//! stage: Filter, bounds: (0, 0, 1280, 720) {
//!   blur: Blur, radius: 14
//!   glow: Bloom, intensity: 0.8, threshold: 0.55
//!   // …the scene's own actors, which the chain above lights
//! }
//! ```

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Bloom` parameters.
pub const BLOOM_PARAMS: &[EffectParamSpec] = &[
    // Identity 0 contributes nothing. Above 0 it adds the pre-chain original
    // back onto the running result: an exposure lift on its own, and a glow
    // after a blur.
    EffectParamSpec::new("intensity", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    // How much of the un-blurred frame to keep underneath the glow. At 0 the
    // pass is a plain blurred copy of the scene; at 1 the glow sits on the
    // original, which is the point.
    EffectParamSpec::new("keep", EffectParamKind::F32, EffectParamValue::F32(1.0), 4, 4),
    // Luma below which the running result receives no added light. 0 disables
    // the gate entirely, preserving the historical bloom behaviour; luma is
    // 0..1, so the useful range is 0..1.
    EffectParamSpec::new("threshold", EffectParamKind::F32, EffectParamValue::F32(0.0), 8, 4),
];

pub(crate) const BLOOM_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    r#"
struct BloomParams {
    intensity: f32,
    keep: f32,
    threshold: f32,
    _pad0: f32,
}

// The running chain result — for a bloom, whatever the previous pass left
// (usually a blur of this same region).
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: BloomParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
// ABI v2: the pre-chain original, copied by the host before pass 0.
@group(0) @binding(5) var original: texture_2d<f32>;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let base = textureLoad(original, vec2<i32>(coord), 0);
    let glow = textureLoad(src, vec2<i32>(coord), 0);
    let add = clamp(params.intensity, 0.0, 4.0);
    let keep = clamp(params.keep, 0.0, 1.0);
    let threshold = clamp(params.threshold, 0.0, 1.0);

    // Gate the added light by the running result's luma: below the threshold a
    // dark region keeps only its own light, while a bright one still blooms.
    // At threshold 0 the gate is disabled so the historical behaviour — add
    // the running result everywhere — is preserved exactly.
    let luma = dot(glow.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let gate = select(1.0, smoothstep(threshold, threshold + 0.1, luma), threshold > 0.0);
    let added_alpha = glow.a * add * gate;
    let added_rgb = glow.rgb * add * gate;

    // Additive, in premultiplied space: the render target is premultiplied, so
    // the result has to keep `rgb <= a` or the composite fringes at the scope's
    // semi-transparent edges. A glow also *creates* coverage where the blurred
    // copy has alpha, which is the point of it. Where both texels are opaque
    // this is identical to clamping the colour alone.
    let out_alpha = clamp(base.a + added_alpha, 0.0, 1.0);
    let rgb = clamp(
        base.rgb * keep + added_rgb,
        vec3<f32>(0.0),
        vec3<f32>(out_alpha),
    );
    textureStore(dst, coord, vec4<f32>(rgb, out_alpha));
}
"#
);

const BLOOM_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("bloom", BLOOM_WGSL, "main")];

/// The `Bloom` effect.
pub struct Bloom;

/// Singleton instance of [`Bloom`].
pub const BLOOM: Bloom = Bloom;

impl Effect for Bloom {
    fn type_name(&self) -> &str {
        "Bloom"
    }

    fn display_name(&self) -> &str {
        "Bloom"
    }

    fn params(&self) -> &[EffectParamSpec] {
        BLOOM_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        BLOOM_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
        out[8..12].copy_from_slice(&params.f32_at(2).to_le_bytes());
    }

    /// Reads one texel from each of two textures — no neighbourhood, so a
    /// region-scoped scope needs no padding.
    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}
