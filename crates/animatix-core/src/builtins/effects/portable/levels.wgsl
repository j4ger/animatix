struct LevelsParams {
    in_black: f32,
    in_white: f32,
    gamma: f32,
    out_black: f32,
    out_white: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: LevelsParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;

fn remap(c: f32) -> f32 {
    let span = params.in_white - params.in_black;
    if (abs(span) < 0.0001) {
        return params.out_black;
    }
    let x = clamp((c - params.in_black) / span, 0.0, 1.0);
    let gamma = max(params.gamma, 0.001);
    return params.out_black + pow(x, 1.0 / gamma) * (params.out_white - params.out_black);
}

@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coord = vec2<i32>(position.xy);
    let texel = textureLoad(src, coord, 0);
    return vec4<f32>(
        clamp(remap(texel.r), 0.0, 1.0),
        clamp(remap(texel.g), 0.0, 1.0),
        clamp(remap(texel.b), 0.0, 1.0),
        texel.a,
    );
}
