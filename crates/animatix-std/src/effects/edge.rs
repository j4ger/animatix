//! `Edge` — Sobel gradient magnitude, for sketch and outline styles.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `Edge` parameters.
///
/// `amount` blends the edge magnitude over the original; `threshold` drops
/// magnitudes below it (in the normalised `0..1` range the kernel produces).
/// Both are identity at `0`, so an unauthored stage skips.
pub const EDGE_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("amount", EffectParamKind::F32, EffectParamValue::F32(0.0), 0, 4),
    EffectParamSpec::new("threshold", EffectParamKind::F32, EffectParamValue::F32(0.0), 4, 4),
];

const EDGE_WGSL: &str = r#"
struct EdgeParams {
    amount: f32,
    threshold: f32,
    _pad0: f32,
    _pad1: f32,
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
@group(0) @binding(2) var<uniform> params: EdgeParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

fn luma_at(coord: vec2<i32>, size: vec2<u32>) -> f32 {
    let clamped = clamp(coord, vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
    let texel = textureLoad(src, clamped, 0);
    return dot(texel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = ctx.tex_size;

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let c = vec2<i32>(coord);
    let tl = luma_at(c + vec2<i32>(-1, -1), size);
    let tc = luma_at(c + vec2<i32>(0, -1), size);
    let tr = luma_at(c + vec2<i32>(1, -1), size);
    let ml = luma_at(c + vec2<i32>(-1, 0), size);
    let mr = luma_at(c + vec2<i32>(1, 0), size);
    let bl = luma_at(c + vec2<i32>(-1, 1), size);
    let bc = luma_at(c + vec2<i32>(0, 1), size);
    let br = luma_at(c + vec2<i32>(1, 1), size);

    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;

    // A unit step edge peaks at 4 (the positive kernel weights sum to 4), so
    // dividing by four normalises the magnitude into 0..1.
    let magnitude = clamp(length(vec2<f32>(gx, gy)) * 0.25, 0.0, 1.0);
    let edge = select(0.0, magnitude, magnitude >= params.threshold);
    let weight = clamp(params.amount, 0.0, 1.0);
    let texel = textureLoad(src, c, 0);
    textureStore(dst, coord, vec4<f32>(mix(texel.rgb, vec3<f32>(edge), weight), texel.a));
}
"#;

const EDGE_PASSES: &[EffectPassSpec] = &[EffectPassSpec::new("edge", EDGE_WGSL, "main")];

/// The `Edge` effect.
pub struct Edge;

/// Singleton instance of [`Edge`].
pub const EDGE: Edge = Edge;

impl Effect for Edge {
    fn type_name(&self) -> &str {
        "Edge"
    }

    fn params(&self) -> &[EffectParamSpec] {
        EDGE_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        EDGE_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
        out[4..8].copy_from_slice(&params.f32_at(1).to_le_bytes());
    }

    /// The 3×3 Sobel kernel reads one texel outside the pixel.
    fn support(&self, _params: &EffectParams) -> f32 {
        1.0
    }
}
