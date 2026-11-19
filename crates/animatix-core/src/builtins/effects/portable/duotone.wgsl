struct DuotoneParams {
    amount: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    shadow: vec4<f32>,
    highlight: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: DuotoneParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    let luma = clamp(dot(texel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let mapped = mix(params.shadow.rgb, params.highlight.rgb, luma);
    let weight = clamp(params.amount, 0.0, 1.0);
    return vec4<f32>(mix(texel.rgb, mapped, weight), texel.a);
}
