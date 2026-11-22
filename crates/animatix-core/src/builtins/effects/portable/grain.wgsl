struct GrainParams {
    amount: f32,
    seed: f32,
    monochrome: u32,
    animated: u32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: GrainParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

fn hash32(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    let shifted = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    h = (shifted >> 22u) ^ shifted;
    return h;
}

fn unit_noise(x: u32) -> f32 {
    return f32(hash32(x) & 0x00ffffffu) / f32(0x01000000u);
}

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<u32>(position.xy);
    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let amount = clamp(params.amount, 0.0, 1.0);
    if (amount <= 0.0) {
        return texel;
    }
    let frame = select(0u, u32(ctx.time_ms * 0.06), params.animated != 0u);
    let pixel = coord.x + coord.y * ctx.tex_size.x;
    let salt = u32(params.seed * 4096.0);
    if (params.monochrome != 0u) {
        let n = unit_noise(pixel ^ (frame << 12u) ^ salt) - 0.5;
        let rgb = texel.rgb + vec3<f32>(n * amount);
        return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a);
    }
    let r = unit_noise(pixel ^ (frame << 12u) ^ salt);
    let g = unit_noise(pixel ^ (frame << 13u) ^ (salt + 7919u));
    let b = unit_noise(pixel ^ (frame << 14u) ^ (salt + 15485u));
    let rgb = texel.rgb + (vec3<f32>(r, g, b) - vec3<f32>(0.5)) * amount;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a);
}
