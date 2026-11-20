// Full-screen triangle used by the portable render-pass implementations.
// Fragment coordinates are pixel centres; converting position.xy to an
// integer recovers the same texel coordinate used by the compute entry point.
struct FullscreenVertex {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn fullscreen_vs(@builtin(vertex_index) vertex: u32) -> FullscreenVertex {
    let xy = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return FullscreenVertex(
        vec4<f32>(xy * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0),
    );
}
