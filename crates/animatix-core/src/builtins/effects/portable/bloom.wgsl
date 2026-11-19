struct BloomParams {
    intensity: f32,
    keep: f32,
    threshold: f32,
    _pad0: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: BloomParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(5) var original: texture_2d<f32>;

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let base = textureLoad(original, coord, 0);
    let glow = textureLoad(src, coord, 0);
    let add = clamp(params.intensity, 0.0, 4.0);
    let keep = clamp(params.keep, 0.0, 1.0);
    let threshold = clamp(params.threshold, 0.0, 1.0);
    let luma = dot(glow.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let gate = select(1.0, smoothstep(threshold, threshold + 0.1, luma), threshold > 0.0);
    let added_alpha = glow.a * add * gate;
    let added_pm = glow.rgb * added_alpha;
    let base_pm = base.rgb * base.a * keep;
    let out_alpha = clamp(base.a + added_alpha, 0.0, 1.0);
    let out_pm = clamp(base_pm + added_pm, vec3<f32>(0.0), vec3<f32>(out_alpha));
    var rgb = vec3<f32>(0.0);
    if (out_alpha > 0.0) {
        rgb = out_pm / out_alpha;
    }
    return vec4<f32>(rgb, out_alpha);
}
