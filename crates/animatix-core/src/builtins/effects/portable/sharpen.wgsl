struct SharpenParams {
    amount: f32,
    radius: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: SharpenParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    let size = vec2<i32>(ctx.tex_size);
    let radius = i32(clamp(params.radius, 0.0, 32.0));
    let amount = params.amount;
    if (radius == 0 || amount == 0.0) {
        return texel;
    }
    var total = vec3<f32>(0.0, 0.0, 0.0);
    var samples = 0.0;
    for (var dy = -radius; dy <= radius; dy = dy + 1) {
        for (var dx = -radius; dx <= radius; dx = dx + 1) {
            let p = clamp(
                coord + vec2<i32>(dx, dy),
                vec2<i32>(0),
                size - vec2<i32>(1),
            );
            total += textureLoad(src, p, 0).rgb;
            samples += 1.0;
        }
    }
    let average = total / samples;
    let sharpened = texel.rgb + (texel.rgb - average) * amount;
    return vec4<f32>(clamp(sharpened, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a);
}
