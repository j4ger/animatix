// Vertex shader

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) is_circle: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) is_circle: u32,
};

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    // Scale and translate the vertex (model.position is expected to be in range [-1, 1])
    let world_position = (model.position * instance.size) + instance.position;

    // Project to clip space
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 0.0, 1.0);

    out.color = instance.color;
    out.uv = model.position;
    out.is_circle = instance.is_circle;

    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // If it is a circle, discard pixels outside the radius
    if in.is_circle == 1u {
        let dist = length(in.uv);
        if dist > 1.0 {
            discard;
        }
    }

    return in.color;
}
