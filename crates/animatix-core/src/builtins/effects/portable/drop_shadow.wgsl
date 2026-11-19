struct DropShadowParams {
    offset: vec2<f32>,
    softness: f32,
    _pad0: f32,
    color: vec4<f32>,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: DropShadowParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let size = ctx.tex_size;
    let shadow_coord = coord - vec2<i32>(params.offset);
    let radius = max(params.softness, 0.0);
    var shadow_cover = 0.0;
    if (radius <= 0.0) {
        if (shadow_coord.x >= 0 && shadow_coord.y >= 0
            && shadow_coord.x < i32(size.x) && shadow_coord.y < i32(size.y)) {
            shadow_cover = textureLoad(src, shadow_coord, 0).a;
        }
    } else {
        let taps: u32 = 16u;
        var shadow_sum = 0.0;
        for (var i: u32 = 0u; i < taps; i = i + 1u) {
            let f = (f32(i) + 0.5) / f32(taps);
            let angle = f32(i) * 2.399963229728653;
            let reach = sqrt(f) * radius;
            let tap = shadow_coord + vec2<i32>(vec2<f32>(cos(angle), sin(angle)) * reach);
            if (tap.x >= 0 && tap.y >= 0 && tap.x < i32(size.x) && tap.y < i32(size.y)) {
                shadow_sum += textureLoad(src, tap, 0).a;
            }
        }
        shadow_cover = shadow_sum / f32(taps);
    }
    let shadow_alpha = shadow_cover * params.color.a;
    let texel = textureLoad(src, coord, 0);
    let source_alpha = texel.a;
    let out_alpha = source_alpha + shadow_alpha * (1.0 - source_alpha);
    let source_premul = texel.rgb * source_alpha;
    let shadow_premul = params.color.rgb * shadow_alpha;
    let out_premul = source_premul + shadow_premul * (1.0 - source_alpha);
    var out_rgb = vec3<f32>(0.0);
    if (out_alpha > 0.0) {
        out_rgb = out_premul / out_alpha;
    }
    return vec4<f32>(out_rgb, out_alpha);
}
