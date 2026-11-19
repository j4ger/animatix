struct EdgeParams {
    amount: f32,
    threshold: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: EdgeParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

fn luma_at(coord: vec2<i32>, size: vec2<u32>) -> f32 {
    let clamped = clamp(coord, vec2<i32>(0), vec2<i32>(size) - vec2<i32>(1));
    let texel = textureLoad(src, clamped, 0);
    return dot(texel.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let size = ctx.tex_size;
    let tl = luma_at(coord + vec2<i32>(-1, -1), size);
    let tc = luma_at(coord + vec2<i32>(0, -1), size);
    let tr = luma_at(coord + vec2<i32>(1, -1), size);
    let ml = luma_at(coord + vec2<i32>(-1, 0), size);
    let mr = luma_at(coord + vec2<i32>(1, 0), size);
    let bl = luma_at(coord + vec2<i32>(-1, 1), size);
    let bc = luma_at(coord + vec2<i32>(0, 1), size);
    let br = luma_at(coord + vec2<i32>(1, 1), size);
    let gx = -tl + tr - 2.0 * ml + 2.0 * mr - bl + br;
    let gy = -tl - 2.0 * tc - tr + bl + 2.0 * bc + br;
    let magnitude = clamp(length(vec2<f32>(gx, gy)) * 0.25, 0.0, 1.0);
    let edge = select(0.0, magnitude, magnitude >= params.threshold);
    let weight = clamp(params.amount, 0.0, 1.0);
    let texel = textureLoad(src, coord, 0);
    return vec4<f32>(mix(texel.rgb, vec3<f32>(edge), weight), texel.a);
}
