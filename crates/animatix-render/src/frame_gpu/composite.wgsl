struct Parameters {
    clip: vec4f,
    source: vec4f,
    settings: vec4f,
}

@group(0) @binding(0) var base_texture: texture_2d<f32>;
@group(0) @binding(1) var source_texture: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Parameters;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs(@builtin(vertex_index) index: u32) -> Vertex {
    let xy = vec2f(f32((index << 1u) & 2u), f32(index & 2u));
    return Vertex(vec4f(xy * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), 0.0, 1.0), xy);
}

fn straight(color: vec4f) -> vec4f {
    if color.a <= 0.0 {
        return vec4f(0.0);
    }
    return vec4f(color.rgb / color.a, color.a);
}

@fragment
fn fs(vertex: Vertex) -> @location(0) vec4f {
    let base = textureSampleLevel(base_texture, linear_sampler, vertex.uv, 0.0);
    let mode = u32(params.settings.x);
    if mode == 0u {
        return base;
    }
    if mode == 1u {
        return straight(base);
    }
    let point = vertex.position.xy;
    let half_size = (params.clip.zw - params.clip.xy) * 0.5;
    let corner = min(params.settings.z, min(half_size.x, half_size.y));
    let q = abs(point - (params.clip.xy + half_size)) - half_size + corner;
    let distance = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - corner;
    let coverage = 1.0 - smoothstep(0.0, 1.0, distance);
    let uv = (point - params.source.xy) / params.source.zw;
    let foreground = textureSampleLevel(source_texture, linear_sampler, uv, 0.0);
    let alpha = coverage * params.settings.y;
    if mode == 2u {
        return mix(base, foreground, alpha);
    }
    let overlay = foreground * alpha;
    return overlay + base * (1.0 - overlay.a);
}
