struct Parameters {
    clip: vec4f,
    // mode, radius, corner radius, opacity
    settings: vec4f,
}

@group(0) @binding(0) var first: texture_2d<f32>;
@group(0) @binding(1) var second: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var<uniform> params: Parameters;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs(@builtin(vertex_index) vertex: u32) -> Vertex {
    let xy = vec2f(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return Vertex(vec4f(xy * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), 0.0, 1.0), xy);
}

@fragment
fn fs(vertex: Vertex) -> @location(0) vec4f {
    let mode = u32(params.settings.x);
    let radius = params.settings.y;
    let uv = vertex.uv;
    let base = textureSampleLevel(first, linear_sampler, uv, 0.0);
    if mode == 0u {
        return base;
    }
    if mode == 1u || mode == 2u {
        if radius < 0.5 {
            return base;
        }
        // Same 17-tap kernel as the compute Blur. Vello GPU produces premultiplied
        // textures, so sampling interpolates premultiplied pixels directly.
        let direction = select(vec2f(0.0, 1.0), vec2f(1.0, 0.0), mode == 1u);
        let delta = direction / vec2f(textureDimensions(first));
        let stride = max(1.0, radius / 8.0);
        let sigma = radius / 3.0;
        var filtered = vec4f(0.0);
        var weight_sum = 0.0;
        for (var i = -8; i <= 8; i++) {
            let offset = f32(i) * stride;
            let weight = exp(-(offset * offset) / (2.0 * sigma * sigma));
            filtered += textureSampleLevel(first, linear_sampler, uv + delta * offset, 0.0) * weight;
            weight_sum += weight;
        }
        return filtered / weight_sum;
    }
    let other = textureSampleLevel(second, linear_sampler, uv, 0.0);
    if mode == 3u {
        let point = uv * vec2f(textureDimensions(first));
        let half_size = (params.clip.zw - params.clip.xy) * 0.5;
        let corner = min(params.settings.z, min(half_size.x, half_size.y));
        let q = abs(point - (params.clip.xy + half_size)) - half_size + corner;
        let distance = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - corner;
        return mix(base, other, params.settings.w * (1.0 - smoothstep(0.0, 1.0, distance)));
    }
    let overlay = other * params.settings.w;
    return overlay + base * (1.0 - overlay.a);
}
