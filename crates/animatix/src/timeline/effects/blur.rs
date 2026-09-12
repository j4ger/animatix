//! `Blur` — separable Gaussian blur (horizontal pass, then vertical).
//!
//! Parameter names, kinds, and identity values live in the shared contract
//! table (`animatix-syntax/src/schema.rs::effect_specs()`); this file holds
//! only the shader and the runtime behaviour.

use super::{Effect, EffectParams, EffectPassSpec};

const BLUR_WGSL: &str = r#"
struct BlurParams {
    radius: f32,
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
@group(0) @binding(2) var<uniform> params: BlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let size = vec2<i32>(ctx.tex_size);

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    if (params.radius < 0.5) {
        textureStore(dst, coord, textureLoad(src, coord, 0));
        return;
    }

    let sigma = params.radius / 3.0;
    let radius = i32(ceil(params.radius));

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var i = -radius; i <= radius; i = i + 1) {
        var offset: vec2<i32>;
        if (ctx.pass_index == 0u) {
            offset = vec2<i32>(i, 0);
        } else {
            offset = vec2<i32>(0, i);
        }
        let sample_coord = clamp(coord + offset, vec2<i32>(0), size - vec2<i32>(1));
        let w = gaussian_weight(f32(i), sigma);
        color = color + textureLoad(src, sample_coord, 0) * w;
        weight_sum = weight_sum + w;
    }

    textureStore(dst, coord, color / weight_sum);
}
"#;

const BLUR_PASSES: &[EffectPassSpec] = &[
    EffectPassSpec {
        label: "blur-horizontal",
        wgsl: BLUR_WGSL,
        entry: "main",
    },
    EffectPassSpec {
        label: "blur-vertical",
        wgsl: BLUR_WGSL,
        entry: "main",
    },
];

/// The `Blur` effect.
pub struct Blur;

/// Singleton instance of [`Blur`].
pub const BLUR: Blur = Blur;

impl Effect for Blur {
    fn type_name(&self) -> &'static str {
        "Blur"
    }

    fn passes(&self) -> &'static [EffectPassSpec] {
        BLUR_PASSES
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
        out.fill(0);
        out[0..4].copy_from_slice(&params.f32_at(0).to_le_bytes());
    }

    fn support(&self, params: &EffectParams) -> f32 {
        // The H/V passes sample ±radius texels (σ = radius / 3).
        params.f32_at(0).max(0.0)
    }
}
