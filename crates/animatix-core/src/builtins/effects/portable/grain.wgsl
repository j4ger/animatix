@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: GrainParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<u32>(position.xy);
    return apply_grain(textureLoad(src, vec2<i32>(coord), 0), coord, params, ctx);
}
