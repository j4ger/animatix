struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct TextInstance {
    position: vec2<f32>,
    scale: vec2<f32>,
    color: vec4<f32>,
    uv_rect: vec4<f32>,
}
@group(0) @binding(1)
var<storage, read> instances: array<TextInstance>;

@group(1) @binding(0)
var font_sampler: sampler;
@group(1) @binding(1)
var font_texture: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    let instance = instances[instance_index];
    
    // Scale and translate
    let scaled_pos = model.position * instance.scale;
    let world_pos = scaled_pos + instance.position;
    
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    
    // Calculate UV based on the quad vertices
    // model.position is from [-1, 1]. Map it to [0, 1]
    let normalized_pos = model.position * 0.5 + vec2<f32>(0.5, 0.5);
    
    // uv_rect is [x, y, w, h]
    let uv_x = instance.uv_rect.x + normalized_pos.x * instance.uv_rect.z;
    let uv_y = instance.uv_rect.y + normalized_pos.y * instance.uv_rect.w;
    out.uv = vec2<f32>(uv_x, uv_y);
    out.color = instance.color;
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(font_texture, font_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
