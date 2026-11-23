struct GrainParams {
    amount: f32,
    seed: f32,
    monochrome: u32,
    animated: u32,
    size: f32,
    // WebGL2 requires the uniform struct itself to span a multiple of 16 bytes.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

// PCG-style integer hash shared by both GPU paths.
fn unit_noise(key: u32) -> f32 {
    let hash = key * 747796405u + 2891336453u;
    let shifted = ((hash >> ((hash >> 28u) + 4u)) ^ hash) * 277803737u;
    return f32(((shifted >> 22u) ^ shifted) & 0x00ffffffu) / f32(0x01000000u);
}

fn lattice_noise(cell: vec2<u32>, key: u32) -> f32 {
    return unit_noise((cell.x * 0x9e3779b9u) ^ (cell.y * 0x85ebca6bu) ^ key);
}

fn grain_noise(coord: vec2<u32>, width: u32, size: f32, key: u32) -> f32 {
    if (size <= 1.0) {
        return unit_noise((coord.x + coord.y * width) ^ key);
    }
    // Smoothly interpolate the coarser field to avoid square pixel blocks.
    let point = vec2<f32>(coord) / size;
    let cell = vec2<u32>(point);
    let t = smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), fract(point));
    let top = mix(lattice_noise(cell, key), lattice_noise(cell + vec2<u32>(1u, 0u), key), t.x);
    let bottom = mix(
        lattice_noise(cell + vec2<u32>(0u, 1u), key),
        lattice_noise(cell + vec2<u32>(1u, 1u), key),
        t.x,
    );
    return mix(top, bottom, t.y);
}

fn apply_grain(texel: vec4<f32>, coord: vec2<u32>, params: GrainParams, ctx: EffectContext) -> vec4<f32> {
    let amount = clamp(params.amount, 0.0, 1.0);
    if (amount <= 0.0) {
        return texel;
    }
    let frame = select(0u, u32(ctx.time_ms * 0.06), params.animated != 0u);
    let salt = u32(params.seed * 4096.0);
    let width = ctx.tex_size.x;
    let r = grain_noise(coord, width, params.size, (frame << 12u) ^ salt);
    var noise = vec3<f32>(r);
    if (params.monochrome == 0u) {
        noise.g = grain_noise(coord, width, params.size, (frame << 13u) ^ (salt + 7919u));
        noise.b = grain_noise(coord, width, params.size, (frame << 14u) ^ (salt + 15485u));
    }
    let rgb = texel.rgb + (noise - vec3<f32>(0.5)) * amount;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), texel.a);
}
