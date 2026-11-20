struct PosterizeParams {
    levels: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: PosterizeParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    let steps = max(params.levels - 1.0, 1.0);
    let rgb = round(texel.rgb * steps) / steps;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a);
}
