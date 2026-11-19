struct MotionBlurParams {
    length: f32,
    angle: f32,
    _pad0: f32,
    _pad1: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: MotionBlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

fn premultiplied_texel(coord: vec2<i32>) -> vec4<f32> {
    let color = textureLoad(src, coord, 0);
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn bilinear_premultiplied(position: vec2<f32>) -> vec4<f32> {
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    let size = vec2<i32>(ctx.tex_size);
    let max_coord = size - vec2<i32>(1);
    let p00 = clamp(base, vec2<i32>(0), max_coord);
    if (all(fraction == vec2<f32>(0.0))) {
        return premultiplied_texel(p00);
    }
    if (fraction.y == 0.0) {
        let p10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
        return mix(premultiplied_texel(p00), premultiplied_texel(p10), fraction.x);
    }
    if (fraction.x == 0.0) {
        let p01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
        return mix(premultiplied_texel(p00), premultiplied_texel(p01), fraction.y);
    }
    let p10 = clamp(base + vec2<i32>(1, 0), vec2<i32>(0), max_coord);
    let p01 = clamp(base + vec2<i32>(0, 1), vec2<i32>(0), max_coord);
    let p11 = clamp(base + vec2<i32>(1, 1), vec2<i32>(0), max_coord);
    let top = mix(premultiplied_texel(p00), premultiplied_texel(p10), fraction.x);
    let bottom = mix(premultiplied_texel(p01), premultiplied_texel(p11), fraction.x);
    return mix(top, bottom, fraction.y);
}

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    let taps = i32(clamp(params.length, 0.0, 64.0));
    if (taps <= 1) {
        return texel;
    }
    let radians = params.angle * 0.017453292519943295;
    let step_direction = vec2<f32>(cos(radians), -sin(radians));
    let center = vec2<f32>(coord);
    var total_premultiplied = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    let half_taps = f32(taps) * 0.5;
    for (var i = 0; i < taps; i = i + 1) {
        let t = f32(i) + 0.5 - half_taps;
        total_premultiplied += bilinear_premultiplied(center + step_direction * t);
    }
    let blurred = total_premultiplied / f32(taps);
    var straight_rgb = vec3<f32>(0.0);
    if (blurred.a > 0.0) {
        straight_rgb = blurred.rgb / blurred.a;
    }
    return vec4<f32>(straight_rgb, max(texel.a, blurred.a));
}
