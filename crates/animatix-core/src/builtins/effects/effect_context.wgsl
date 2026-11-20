// Host-owned per-pass effect context (bind group 0, binding 3). Shared by
// every built-in effect; keep `EffectContextUniform` in the render backend in
// sync with this layout.
struct EffectContext {
    tex_size: vec2<u32>,
    _pad0: vec2<u32>,
    inv_size: vec2<f32>,
    _pad1: vec2<f32>,
    pass_index: u32,
    pass_count: u32,
    time_ms: f32,
    _pad2: f32,
}
