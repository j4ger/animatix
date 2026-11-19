struct ChromaOffsetParams {
    offset: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: ChromaOffsetParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    if (params.offset < 0.25) {
        return texel;
    }
    let size = vec2<f32>(ctx.tex_size);
    let center = size * 0.5;
    let pos = vec2<f32>(coord);
    let to_pixel = pos - center;
    let dir = to_pixel / max(length(to_pixel), 1.0);
    let duv = dir * params.offset / size;
    let uv = (pos + vec2<f32>(0.5)) / size;
    let r = textureSampleLevel(src, samp, uv + duv, 0.0).r;
    let g = textureSampleLevel(src, samp, uv, 0.0).g;
    let b = textureSampleLevel(src, samp, uv - duv, 0.0).b;
    return vec4<f32>(r, g, b, texel.a);
}
