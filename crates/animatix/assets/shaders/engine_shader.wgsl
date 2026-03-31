// Vertex shader

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct SdfInstance {
    position: vec2<f32>,
    size: vec2<f32>,
    uv_rect: vec4<f32>,
    shape_params: vec4<f32>,
    fill_color: vec4<f32>,
    stroke_color: vec4<f32>,
    stroke_width: f32,
    glow_radius: f32,
    opacity: f32,
    shape_type: u32,
    target_position: vec2<f32>,
    target_size: vec2<f32>,
    target_shape_params: vec4<f32>,
    target_shape_type: u32,
    shape_blend: f32,
    _padding1: vec2<f32>,
    morph_params: vec4<f32>,
};

@group(0) @binding(1)
var<storage, read> instances: array<SdfInstance>;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) shape_type: u32,
};

@vertex
fn vs_main(
    model: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let instance = instances[instance_idx];

    // Scale and translate the vertex (model.position is expected to be in range [-1, 1])
    // Basic implementation for now: uses current position/size.
    // Target morphing variables (target_position, shape_blend) can be factored in later.
    let world_position = (model.position * instance.size) + instance.position;

    // Project to clip space
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 0.0, 1.0);

    out.color = instance.fill_color;
    out.color.a *= instance.opacity;
    out.uv = model.position;
    out.shape_type = instance.shape_type;

    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // If it is a circle (shape_type == 1), discard pixels outside the radius
    if in.shape_type == 1u {
        let dist = length(in.uv);
        if dist > 1.0 {
            discard;
        }
    }

    return in.color;
}
