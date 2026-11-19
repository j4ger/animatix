struct BlurParams {
    radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: BlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

const TAPS: i32 = 17;

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

fn premultiplied_texel(coord: vec2<i32>) -> vec4<f32> {
    let color = textureLoad(src, coord, 0);
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn tap(center: vec2<f32>, offset: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let position = center + offset;
    let base = vec2<i32>(floor(position));
    let fraction = dot(fract(position), dir);
    let size = vec2<i32>(ctx.tex_size);
    let max_coord = size - vec2<i32>(1);
    let p0 = clamp(base, vec2<i32>(0), max_coord);
    if (fraction == 0.0) {
        return premultiplied_texel(p0);
    }
    let p1 = clamp(base + vec2<i32>(dir), vec2<i32>(0), max_coord);
    return mix(premultiplied_texel(p0), premultiplied_texel(p1), fraction);
}

fn blur_pixel(coord: vec2<i32>) -> vec4<f32> {
    if (params.radius < 0.5) {
        return textureLoad(src, coord, 0);
    }
    let center = vec2<f32>(coord);
    let sigma = params.radius / 3.0;
    let half_taps = f32(TAPS - 1) * 0.5;
    let stride = max(1.0, 2.0 * params.radius / f32(TAPS - 1));
    var dir = vec2<f32>(1.0, 0.0);
    if (ctx.pass_index != 0u) {
        dir = vec2<f32>(0.0, 1.0);
    }
    var premultiplied = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i: i32 = 0; i < TAPS; i = i + 1) {
        let t = (f32(i) - half_taps) * stride;
        let weight = gaussian_weight(t, sigma);
        premultiplied += tap(center, dir * t, dir) * weight;
        weight_sum += weight;
    }
    let filtered = premultiplied / weight_sum;
    if (filtered.a > 0.0) {
        return vec4<f32>(filtered.rgb / filtered.a, filtered.a);
    }
    return vec4<f32>(0.0);
}

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return blur_pixel(vec2<i32>(position.xy));
}
