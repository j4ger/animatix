struct LensDistortionParams {
    amount: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: LensDistortionParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<u32>(position.xy);
    let size = ctx.tex_size;
    let uv = (vec2<f32>(coord) + vec2<f32>(0.5)) * ctx.inv_size;
    let offset = uv - vec2<f32>(0.5);
    let radius2 = dot(offset, offset) * 2.0;
    let diagonal = max(length(vec2<f32>(size)), 1.0);
    let scale = params.amount * radius2 * 2.0 / diagonal;
    let sample_uv = clamp(
        vec2<f32>(0.5) + offset * (1.0 - scale),
        vec2<f32>(0.0),
        vec2<f32>(1.0),
    );
    return textureSampleLevel(src, samp, sample_uv, 0.0);
}
