struct ColorGradeParams {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    m3: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: ColorGradeParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    let rgba = vec4<f32>(texel.r, texel.g, texel.b, texel.a);
    let r = dot(params.m0, rgba);
    let g = dot(params.m1, rgba);
    let b = dot(params.m2, rgba);
    let a = dot(params.m3, rgba);
    return vec4<f32>(
        clamp(r, 0.0, 1.0),
        clamp(g, 0.0, 1.0),
        clamp(b, 0.0, 1.0),
        clamp(a, 0.0, 1.0),
    );
}
