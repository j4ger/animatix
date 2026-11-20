struct VignetteParams {
    amount: f32,
    radius: f32,
    softness: f32,
    _pad0: f32,
    color: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: VignetteParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<u32>(position.xy);
    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let size = ctx.tex_size;
    let p = (vec2<f32>(coord) - vec2<f32>(size) * 0.5) / min(f32(size.x), f32(size.y));
    let dist = length(p);
    let inner = max(params.radius, 0.0);
    let outer = max(inner + max(params.softness, 0.001), inner + 0.001);
    let t = smoothstep(inner, outer, dist);
    let weight = clamp(params.amount, 0.0, 1.0) * t;
    let rgb = mix(texel.rgb, params.color.rgb, weight);
    return vec4<f32>(rgb, texel.a);
}
