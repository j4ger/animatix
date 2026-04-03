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
    @location(0) fill_color: vec4<f32>,
    @location(1) stroke_color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) shape_type: u32,
    @location(4) size: vec2<f32>,
    @location(5) stroke_width: f32,
};

@vertex
fn vs_main(
    model: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let instance = instances[instance_idx];

    let pad = instance.stroke_width + 1.0;
    let padded_size = instance.size + vec2<f32>(pad, pad);

    // Scale and translate the vertex (model.position is expected to be in range [-1, 1])
    let world_position = (model.position * padded_size) + instance.position;

    // Project to clip space
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 0.0, 1.0);

    out.fill_color = instance.fill_color;
    out.fill_color.a *= instance.opacity;
    out.stroke_color = instance.stroke_color;
    out.stroke_color.a *= instance.opacity;
    out.uv = model.position * padded_size;
    out.shape_type = instance.shape_type;
    out.size = instance.size;
    out.stroke_width = instance.stroke_width;

    return out;
}

// Fragment shader

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var d: f32 = 0.0;

    if in.shape_type == 1u {
        d = length(in.uv) - in.size.x;
    } else {
        let d2 = abs(in.uv) - in.size;
        d = length(max(d2, vec2<f32>(0.0))) + min(max(d2.x, d2.y), 0.0);
    }

    let aa = 1.0;

    let fill_alpha = 1.0 - smoothstep(-aa, aa, d);
    let fill_col = vec4<f32>(in.fill_color.rgb, in.fill_color.a * fill_alpha);

    let stroke_d = abs(d) - in.stroke_width / 2.0;
    let stroke_alpha = 1.0 - smoothstep(-aa, aa, stroke_d);
    let stroke_weight = select(0.0, 1.0, in.stroke_width > 0.0);
    let stroke_col = vec4<f32>(in.stroke_color.rgb, in.stroke_color.a * stroke_alpha * stroke_weight);

    var final_color = fill_col;
    if in.stroke_width > 0.0 {
        let out_a = stroke_col.a + final_color.a * (1.0 - stroke_col.a);
        var out_rgb = vec3<f32>(0.0);
        if out_a > 0.0 {
            out_rgb = (stroke_col.rgb * stroke_col.a + final_color.rgb * final_color.a * (1.0 - stroke_col.a)) / out_a;
        }
        final_color = vec4<f32>(out_rgb, out_a);
    }

    if final_color.a < 0.001 {
        discard;
    }

    return final_color;
}
