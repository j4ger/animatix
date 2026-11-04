//! `Blur` — separable Gaussian blur (horizontal pass, then vertical).
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`. The parser crate derives
//! the author-visible contract from the catalog registration.
//!
//! The old implementation walked every texel out to `±radius` in each pass
//! (O(radius) taps: `glass.amx` animates to radius 44, i.e. 89 taps × 2
//! passes per pixel). It is replaced by a downsample approximation with a
//! *fixed* tap count: a compile-time constant number of bilinear taps is
//! spread across the same `±radius` support by a stride that grows with the
//! radius. Each `textureSampleLevel` tap is a hardware 2×2 box (an on-the-fly
//! 2× downsample), so a small kernel still reaches the requested scene-pixel
//! width while the per-pixel cost stays constant. The stride is floored at one
//! texel, so small and medium radii remain the dense Gaussian the direct blur
//! produced — which is what keeps the reduced-raster ramp test honest.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Blur` parameters.
pub const BLUR_PARAMS: &[EffectParamSpec] =
    &[
        EffectParamSpec::new("radius", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4)
            .pixel(),
    ];

pub(crate) const BLUR_WGSL: &str = concat!(
    include_str!("effect_context.wgsl"),
    r#"
struct BlurParams {
    radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: BlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

// Downsample approximation with a fixed tap count that does not grow with the
// radius. The direct blur walked every texel out to the full radius (O(radius)
// loads). Here TAPS is a compile-time constant: the loop below is bounded by
// TAPS, never by params.radius, and the stride spreads those taps across the
// same support. Each textureSampleLevel tap is a hardware 2x2 box (an on-the-
// fly downsample) through the linear sampler, so a fixed small kernel still
// reaches the requested scene-pixel width.
const TAPS: i32 = 17;

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

// One bilinear tap at `center + offset` (both in texel units). At integer
// offsets this degenerates to the exact texel load the direct blur used, so
// small radii stay bit-identical; at fractional offsets the 2x2 box softens
// the sparse gather that large radii need. `textureSample` is fragment-only,
// so sample with an explicit level (as the other built-in effects do).
fn tap(center: vec2<f32>, offset: vec2<f32>) -> vec4<f32> {
    let uv = (center + offset + vec2<f32>(0.5)) * ctx.inv_size;
    return textureSampleLevel(src, samp, uv, 0.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let size = vec2<i32>(ctx.tex_size);

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    // Sub-pixel radii are a bit-exact copy, not an average of identical taps.
    if (params.radius < 0.5) {
        textureStore(dst, coord, textureLoad(src, coord, 0));
        return;
    }

    let center = vec2<f32>(f32(coord.x), f32(coord.y));
    let sigma = params.radius / 3.0;
    let half_taps = f32(TAPS - 1) * 0.5;
    // Floor the stride at one texel: for radii up to half_taps the taps stay a
    // dense Gaussian (the old direct blur, and therefore still ramp-correct
    // under set_raster_scale). Above that the stride grows linearly with the
    // radius, keeping the outermost tap at the full scene-pixel support.
    let stride = max(1.0, 2.0 * params.radius / f32(TAPS - 1));
    var dir = vec2<f32>(1.0, 0.0);          // pass 0: horizontal
    if (ctx.pass_index != 0u) {
        dir = vec2<f32>(0.0, 1.0);          // pass 1: vertical
    }

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i: i32 = 0; i < TAPS; i = i + 1) {
        let t = (f32(i) - half_taps) * stride;
        let w = gaussian_weight(t, sigma);
        color = color + tap(center, dir * t) * w;
        weight_sum = weight_sum + w;
    }
    textureStore(dst, coord, color / weight_sum);
}
"#
);

const BLUR_PASSES: &[EffectPassSpec] = &[
    EffectPassSpec::new("blur-horizontal", BLUR_WGSL, "main"),
    EffectPassSpec::new("blur-vertical", BLUR_WGSL, "main"),
];

/// The `Blur` effect.
pub struct Blur;

/// Singleton instance of [`Blur`].
pub const BLUR: Blur = Blur;

impl Effect for Blur {
    fn type_name(&self) -> &str {
        "Blur"
    }

    fn params(&self) -> &[EffectParamSpec] {
        BLUR_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        BLUR_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
    }

    fn support(&self, params: &EffectParams) -> f32 {
        // The taps span ±radius scene pixels (outermost tap sits at the full
        // support, sigma = radius / 3). This is the true scene-pixel reach, so
        // EffectRegion padding and set_raster_scale keep the blur the same
        // size in scene pixels at a reduced raster.
        params.f32_at(0).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use animatix_core::effect::{Effect, EffectParamUnit, EffectParamValue, EffectParams};

    use super::{BLUR, BLUR_PARAMS, BLUR_PASSES, BLUR_WGSL};

    fn radius(value: f32) -> EffectParams {
        EffectParams {
            values: vec![EffectParamValue::F32(value)],
        }
    }

    /// `radius < 0.5` is a bit-exact copy, not an average of near-identical
    /// taps. Keep the early-out and its single `textureLoad` in the shader,
    /// and keep `pack` writing the radius where `BlurParams.radius` reads it.
    #[test]
    fn radius_zero_is_exact_passthrough() {
        let w = BLUR_WGSL;
        assert!(w.contains("if (params.radius < 0.5)"));
        assert!(w.contains("textureStore(dst, coord, textureLoad(src, coord, 0))"));
        assert_eq!(BLUR_PARAMS.len(), 1);
        assert!(BLUR_PARAMS[0].name == "radius");
        let mut out = [0xffu8; 16];
        BLUR.pack(&radius(0.0), &mut out);
        assert_eq!(&out[0..4], &0.0f32.to_le_bytes());
    }

    /// Two separable passes, each one `queue.submit`. Keep the pass count low
    /// and comfortably under the render-side `MAX_CHAIN_PASSES` (64) cap; a
    /// downsample/upsample pyramid would multiply the submit count and is
    /// deliberately deferred.
    #[test]
    fn pass_count_stays_bounded() {
        assert_eq!(BLUR_PASSES.len(), 2);
        assert!(BLUR_PASSES.len() <= 4);
        assert!(BLUR_PASSES.len() <= 64);
        assert!(BLUR_PASSES[0].label == "blur-horizontal");
        assert!(BLUR_PASSES[1].label == "blur-vertical");
    }

    /// `support()` is the true scene-pixel reach of the blur, and `radius` is
    /// declared in `Pixel` units so `set_raster_scale` halves it with the
    /// raster. Both are what keep the alpha-ramp width stable at half raster.
    #[test]
    fn support_scales_with_radius_and_is_scene_pixels() {
        assert_eq!(BLUR.support(&radius(0.0)), 0.0);
        assert_eq!(BLUR.support(&radius(2.0)), 2.0);
        assert_eq!(BLUR.support(&radius(8.0)), 8.0);
        assert_eq!(BLUR.support(&radius(44.0)), 44.0);
        assert_eq!(BLUR.support(&radius(89.0)), 89.0);
        assert_eq!(BLUR.support(&radius(-3.0)), 0.0);
        assert_eq!(BLUR_PARAMS[0].unit, EffectParamUnit::Pixel);
        assert!(BLUR_PARAMS[0].is_pixel());
    }

    /// The regression guard: the tap loop is bounded by a compile-time
    /// constant, never by `params.radius`. Reintroducing `i <= radius` (or the
    /// old `-radius..=radius` walk) would silently restore the O(radius) blur
    /// that made radius 44 cost 178 texture reads per pixel. `params.radius`
    /// may size the stride, but must never size the loop.
    #[test]
    fn tap_loop_bound_is_a_constant_not_the_radius() {
        let w = BLUR_WGSL;
        assert!(w.contains("const TAPS:"));
        assert!(w.contains("for (var i: i32 = 0; i < TAPS; i = i + 1)"));
        assert!(!w.contains("i <= radius"));
        assert!(!w.contains("-radius"));
        assert!(w.contains("let stride = max(1.0,"));
    }
}
