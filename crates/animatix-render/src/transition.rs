use std::borrow::Cow;
use std::cell::RefCell;

use animatix::easing::Easing;
use animatix::timeline::scene_program::SceneItem;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

use crate::core::RendererCore;

/// GPU-based compositor for scene transition effects.
///
/// Renders a fullscreen quad with a WGSL shader that blends two input textures
/// based on transition progress and type. Supports cut, fade, directional
/// wipe/push, iris, clock-wipe and zoom-through transitions.
pub struct TransitionCompositor {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    bind_group_cache: RefCell<Option<((wgpu::TextureView, wgpu::TextureView), wgpu::BindGroup)>>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionUniforms {
    progress: f32,
    transition_type: u32,
    /// Physical aspect ratio (`width / height`) for the aspect-corrected
    /// radial/angular transitions.
    aspect: f32,
    _padding: f32,
}

impl TransitionUniforms {
    fn new(progress: f32, transition_id: &str, aspect: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            transition_type: animatix_syntax::transition_registry::shader_case(transition_id),
            aspect: if aspect.is_finite() && aspect > 0.0 {
                aspect
            } else {
                1.0
            },
            _padding: 0.0,
        }
    }
}

const TRANSITION_SHADER_WGSL: &str = r#"
struct Uniforms {
    progress: f32,
    transition_type: u32,
    aspect: f32,
    _padding: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var from_texture: texture_2d<f32>;
@group(0) @binding(1) var to_texture: texture_2d<f32>;
@group(0) @binding(2) var texture_sampler: sampler;
@group(0) @binding(3) var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );

    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Exact endpoint guards: before any transition math (and regardless of the
    // easing curve) progress 0 is the outgoing scene and 1 is the incoming one.
    if (uniforms.progress <= 0.0) {
        return textureSampleLevel(from_texture, texture_sampler, in.uv, 0.0);
    }
    if (uniforms.progress >= 1.0) {
        return textureSampleLevel(to_texture, texture_sampler, in.uv, 0.0);
    }

    let feather: f32 = 0.04;
    var alpha: f32 = uniforms.progress;

    switch uniforms.transition_type {
        case 0u: { // Cut
            alpha = select(0.0, 1.0, uniforms.progress >= 0.5);
        }
        case 1u: { // Fade
            alpha = uniforms.progress;
        }
        case 2u: { // WipeLeft
            alpha = 1.0 - smoothstep(uniforms.progress - feather, uniforms.progress + feather, in.uv.x);
        }
        case 3u: { // WipeRight
            alpha = smoothstep(1.0 - uniforms.progress - feather, 1.0 - uniforms.progress + feather, in.uv.x);
        }
        case 4u: { // WipeUp
            alpha = 1.0 - smoothstep(uniforms.progress - feather, uniforms.progress + feather, in.uv.y);
        }
        case 5u: { // WipeDown
            alpha = smoothstep(1.0 - uniforms.progress - feather, 1.0 - uniforms.progress + feather, in.uv.y);
        }
        case 6u: { // PushLeft
            let p = uniforms.progress;
            let uv_from = in.uv + vec2<f32>(p, 0.0);
            let uv_to = in.uv - vec2<f32>(1.0 - p, 0.0);
            if (uv_from.x <= 1.0) {
                return textureSampleLevel(from_texture, texture_sampler, uv_from, 0.0);
            } else {
                return textureSampleLevel(to_texture, texture_sampler, uv_to, 0.0);
            }
        }
        case 7u: { // PushRight
            let p = uniforms.progress;
            let uv_from = in.uv - vec2<f32>(p, 0.0);
            let uv_to = in.uv + vec2<f32>(1.0 - p, 0.0);
            if (uv_from.x >= 0.0) {
                return textureSampleLevel(from_texture, texture_sampler, uv_from, 0.0);
            } else {
                return textureSampleLevel(to_texture, texture_sampler, uv_to, 0.0);
            }
        }
        case 8u: { // PushUp
            let p = uniforms.progress;
            let uv_from = in.uv + vec2<f32>(0.0, p);
            let uv_to = in.uv - vec2<f32>(0.0, 1.0 - p);
            if (uv_from.y <= 1.0) {
                return textureSampleLevel(from_texture, texture_sampler, uv_from, 0.0);
            } else {
                return textureSampleLevel(to_texture, texture_sampler, uv_to, 0.0);
            }
        }
        case 9u: { // PushDown
            let p = uniforms.progress;
            let uv_from = in.uv - vec2<f32>(0.0, p);
            let uv_to = in.uv + vec2<f32>(0.0, 1.0 - p);
            if (uv_from.y >= 0.0) {
                return textureSampleLevel(from_texture, texture_sampler, uv_from, 0.0);
            } else {
                return textureSampleLevel(to_texture, texture_sampler, uv_to, 0.0);
            }
        }
        case 10u: { // Iris
            let centered = in.uv - vec2<f32>(0.5, 0.5);
            // Aspect-corrected distance from the centre (physical pixels, not
            // UV units); the radius reaches the farthest corner at progress 1.
            let dist = length(vec2<f32>(centered.x * uniforms.aspect, centered.y));
            let max_radius = 0.5 * length(vec2<f32>(uniforms.aspect, 1.0));
            let radius = uniforms.progress * max_radius;
            alpha = 1.0 - smoothstep(radius - feather, radius + feather, dist);
        }
        case 11u: { // ClockWipe
            let centered = in.uv - vec2<f32>(0.5, 0.5);
            // Clockwise angle from 12 o'clock, measured with the x axis
            // aspect-corrected so the sweep rate is angular in physical space.
            let angle = atan2(centered.x * uniforms.aspect, -centered.y);
            let angle01 = fract(angle / 6.28318530718 + 1.0);
            alpha = 1.0 - smoothstep(uniforms.progress - feather, uniforms.progress + feather, angle01);
        }
        case 12u: { // ZoomThrough
            let p = uniforms.progress;
            let center = vec2<f32>(0.5, 0.5);
            // Dividing the UV offset by the zoom magnifies the scene, so the
            // outgoing shot pushes in (1.0 -> 1.35) while the incoming one
            // settles from a wider framing (0.8 -> 1.0). Clamping keeps every
            // sample inside the source textures at both extremes.
            let zoom_from = mix(1.0, 1.35, p);
            let zoom_to = mix(0.8, 1.0, p);
            let uv_from = clamp(center + (in.uv - center) / zoom_from, vec2<f32>(0.0), vec2<f32>(1.0));
            let uv_to = clamp(center + (in.uv - center) / zoom_to, vec2<f32>(0.0), vec2<f32>(1.0));
            let from_color = textureSampleLevel(from_texture, texture_sampler, uv_from, 0.0);
            let to_color = textureSampleLevel(to_texture, texture_sampler, uv_to, 0.0);
            // Hold the outgoing scene while it zooms, then crossfade over the
            // last 45% so the incoming scene lands on its own zoom.
            let crossfade = smoothstep(0.55, 1.0, p);
            return mix(from_color, to_color, crossfade);
        }
        default: {
            alpha = uniforms.progress;
        }
    }

    let from_color = textureSampleLevel(from_texture, texture_sampler, in.uv, 0.0);
    let to_color = textureSampleLevel(to_texture, texture_sampler, in.uv, 0.0);
    return mix(from_color, to_color, alpha);
}
"#;

impl TransitionCompositor {
    /// Build the transition pipeline, bind-group layout, and sampler on `device`.
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Animatix Transition Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TRANSITION_SHADER_WGSL)),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Animatix Transition Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Animatix Transition Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Animatix Transition Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Transition Uniforms"),
            size: std::mem::size_of::<TransitionUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Transition Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            sampler,
            bind_group_cache: RefCell::new(None),
        })
    }

    /// Composite two scene textures into the output texture using the given
    /// transition progress, type, and easing.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        from_view: &wgpu::TextureView,
        to_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        progress: f32,
        transition_id: &str,
        easing: animatix::easing::Easing,
    ) -> Result<(), String> {
        let eased_progress = animatix::easing::apply_easing(progress, easing);
        // Update uniform buffer. `height.max(1)` keeps the aspect finite even
        // for a degenerate surface; the offscreen path rejects zero sizes
        // before this point.
        let aspect = width as f32 / height.max(1) as f32;
        let uniforms = TransitionUniforms::new(eased_progress, transition_id, aspect);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Reuse or create bind group for this frame
        let bind_group = {
            let mut cache = self.bind_group_cache.borrow_mut();
            match cache.as_ref() {
                Some(((cached_from, cached_to), bg))
                    if cached_from == from_view && cached_to == to_view =>
                {
                    bg.clone()
                },
                _ => {
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Animatix Transition Bind Group"),
                        layout: &self.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(from_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(to_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: self.uniform_buffer.as_entire_binding(),
                            },
                        ],
                    });
                    *cache = Some(((from_view.clone(), to_view.clone()), bg.clone()));
                    bg
                },
            }
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Transition Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Animatix Transition Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}

/// Labels matched by `shared_id` across the two scenes of a transition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedActorPlan {
    /// `(from_label, to_label)` pairs, ordered by `shared_id` so the FLIP
    /// overlay has a deterministic paint order.
    pub pairs: Vec<(String, String)>,
    /// Labels to suppress in the outgoing scene.
    pub suppressed_from: Vec<String>,
    /// Labels to suppress in the incoming scene.
    pub suppressed_to: Vec<String>,
}

impl SharedActorPlan {
    /// Whether any shared actor matched (and therefore any suppression or
    /// overlay work is needed).
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

/// One matched outgoing/incoming item pair drawn in the floating FLIP overlay.
pub type FlipItem = (SceneItem, SceneItem);

/// Match actors carrying the same non-empty `shared_id` at the two local
/// transition times.
///
/// The returned suppression lists drive
/// [`Timeline::evaluate_program_with_suppressed`]; the label pairs are resolved
/// into [`FlipItem`]s by [`collect_flip_pairs`]. Ordering follows the sorted
/// `shared_id` keys (not `HashMap` iteration order), so overlapping shared
/// elements paint consistently from frame to frame.
pub fn plan_shared_actors(
    from_timeline: &Timeline,
    from_time_s: f64,
    to_timeline: &Timeline,
    to_time_s: f64,
) -> SharedActorPlan {
    let from_shared = from_timeline.shared_id_actors((from_time_s * 1000.0) as u64);
    let to_shared = to_timeline.shared_id_actors((to_time_s * 1000.0) as u64);

    let mut shared_ids: Vec<&String> = from_shared.keys().collect();
    shared_ids.sort();

    let mut plan = SharedActorPlan::default();
    for shared_id in shared_ids {
        let Some(to_label) = to_shared.get(shared_id) else {
            continue;
        };
        let from_label = &from_shared[shared_id];
        plan.pairs.push((from_label.clone(), to_label.clone()));
        plan.suppressed_from.push(from_label.clone());
        plan.suppressed_to.push(to_label.clone());
    }
    plan
}

/// Evaluate both scenes without suppression and resolve each planned label
/// pair to the concrete [`SceneItem`]s the FLIP overlay interpolates.
///
/// Returns an empty vector when `plan` has no matches, so callers avoid the
/// extra pair of evaluations for ordinary transitions.
pub fn collect_flip_pairs(
    plan: &SharedActorPlan,
    from_timeline: &Timeline,
    from_time_s: f64,
    to_timeline: &Timeline,
    to_time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
) -> Vec<FlipItem> {
    if plan.is_empty() {
        return Vec::new();
    }
    let from_program = from_timeline.evaluate_program_with_debug(
        from_time_s,
        dimensions,
        debug_options,
        &mut None,
    );
    let to_program =
        to_timeline.evaluate_program_with_debug(to_time_s, dimensions, debug_options, &mut None);

    let mut items = Vec::new();
    for (from_label, to_label) in &plan.pairs {
        let from_item = from_program.items.iter().find(|item| item.label == *from_label).cloned();
        let to_item = to_program.items.iter().find(|item| item.label == *to_label).cloned();
        if let (Some(from_item), Some(to_item)) = (from_item, to_item) {
            items.push((from_item, to_item));
        }
    }
    items
}

/// Encode the floating FLIP overlay scene: each pair's transform is linearly
/// interpolated with the eased progress, and the from/to commands fade out/in
/// over it.
pub fn encode_flip_overlay(pairs: &[FlipItem], progress: f32, easing: Easing) -> vello::Scene {
    let mut overlay = vello::Scene::new();
    let p_eased = animatix::easing::apply_easing(progress, easing);
    let p = p_eased as f64;
    for (from_item, to_item) in pairs {
        let from_coeffs = from_item.transform.as_coeffs();
        let to_coeffs = to_item.transform.as_coeffs();
        let mut coeffs = [0.0; 6];
        for (index, coeff) in coeffs.iter_mut().enumerate() {
            *coeff = (1.0 - p) * from_coeffs[index] + p * to_coeffs[index];
        }
        let transform = kurbo::Affine::new(coeffs);

        let alpha_from = (1.0 - p_eased) * from_item.opacity;
        let alpha_to = p_eased * to_item.opacity;
        if alpha_from > 0.001 {
            for command in &from_item.commands {
                command.execute(&mut overlay, &transform, alpha_from);
            }
        }
        if alpha_to > 0.001 {
            for command in &to_item.commands {
                command.execute(&mut overlay, &transform, alpha_to);
            }
        }
    }
    overlay
}

/// Draw `overlay` into `overlay_view` with a transparent background, then
/// alpha-blit it over the already-composited transition in `output_view`.
///
/// `overlay_view` is a scratch texture owned by the caller; on the native
/// offscreen renderer it is scene A's texture after the blend has consumed it.
pub fn composite_flip_overlay(
    core: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    overlay_view: &wgpu::TextureView,
    output_view: &wgpu::TextureView,
    size: SceneDimensions,
    overlay: &vello::Scene,
) -> Result<(), String> {
    core.render_vello_scene_with_background(
        device,
        queue,
        overlay_view,
        size.width,
        size.height,
        overlay,
        vello::peniko::Color::TRANSPARENT,
    )
    .map_err(|e| e.to_string())?;
    core.blit_texture(device, queue, overlay_view, output_view, size.width, size.height, 1.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `case Nu:` arm in the shader, in source order.
    fn wgsl_switch_cases(shader: &str) -> Vec<u32> {
        shader
            .match_indices("case ")
            .filter_map(|(index, _)| {
                let rest = &shader[index + "case ".len()..];
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() {
                    return None;
                }
                rest[digits.len()..].strip_prefix('u')?.strip_prefix(':')?;
                digits.parse().ok()
            })
            .collect()
    }

    /// Registry-only check (no GPU): every transition id must point at a
    /// real WGSL switch arm, and the arm numbers must be unique and
    /// contiguous from zero. Without this a new registry row would silently
    /// fall through to the shader's default fade.
    #[test]
    fn registry_shader_cases_match_wgsl_switch_arms() {
        use animatix_syntax::transition_registry::REGISTRY;

        let mut registry_cases: Vec<u32> = REGISTRY.iter().map(|def| def.shader_case).collect();
        registry_cases.sort_unstable();
        assert_eq!(
            registry_cases,
            (0..registry_cases.len() as u32).collect::<Vec<_>>(),
            "REGISTRY shader cases must be unique and contiguous from 0"
        );

        let mut shader_cases = wgsl_switch_cases(TRANSITION_SHADER_WGSL);
        shader_cases.sort_unstable();
        assert_eq!(
            shader_cases, registry_cases,
            "every REGISTRY shader_case needs exactly one `case Nu:` arm in TRANSITION_SHADER_WGSL"
        );

        for def in REGISTRY {
            assert!(
                TRANSITION_SHADER_WGSL.contains(&format!("case {}u:", def.shader_case)),
                "transition '{}' has shader_case {} but no matching WGSL arm",
                def.id,
                def.shader_case
            );
        }
    }

    /// The CPU-side uniform must stay byte-compatible with the WGSL struct:
    /// 16 bytes, `aspect` occupying the old padding slot at offset 8.
    #[test]
    fn transition_uniforms_match_wgsl_layout() {
        use std::mem::{offset_of, size_of};

        assert_eq!(size_of::<TransitionUniforms>(), 16);
        assert_eq!(offset_of!(TransitionUniforms, progress), 0);
        assert_eq!(offset_of!(TransitionUniforms, transition_type), 4);
        assert_eq!(offset_of!(TransitionUniforms, aspect), 8);
        assert!(TRANSITION_SHADER_WGSL.contains("aspect: f32"));

        let uniforms = TransitionUniforms::new(2.0, "iris", f32::NAN);
        assert_eq!(uniforms.progress, 1.0, "progress is clamped to [0, 1]");
        assert_eq!(uniforms.transition_type, 10);
        assert_eq!(uniforms.aspect, 1.0, "invalid aspect falls back to square");
    }

    fn timeline(source: &str) -> Timeline {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("AST");
        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
    }

    fn dims() -> SceneDimensions {
        SceneDimensions {
            width: 800,
            height: 600,
        }
    }

    #[test]
    fn shared_actor_plan_matches_and_suppresses_both_labels() {
        let from = timeline(
            r#"
#0s
card: Rect, at: (100, 100), size: (50, 50), color: (1, 0, 0, 1), shared_id: "hero"
bystander: Rect, at: (0, 0), size: (10, 10), color: (0, 1, 0, 1)
"#,
        );
        let to = timeline(
            r#"
#0s
card_large: Rect, at: (400, 300), size: (150, 150), color: (1, 0, 0, 1), shared_id: "hero"
"#,
        );

        let plan = plan_shared_actors(&from, 0.0, &to, 0.0);
        assert_eq!(plan.pairs, vec![("card".to_string(), "card_large".to_string())]);
        assert_eq!(plan.suppressed_from, vec!["card".to_string()]);
        assert_eq!(plan.suppressed_to, vec!["card_large".to_string()]);

        let pairs =
            collect_flip_pairs(&plan, &from, 0.0, &to, 0.0, dims(), DebugRenderOptions::default());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.label, "card");
        assert_eq!(pairs[0].1.label, "card_large");
        assert_ne!(pairs[0].0.transform, pairs[0].1.transform);

        let overlay = encode_flip_overlay(&pairs, 0.5, Easing::Linear);
        assert!(!overlay.encoding().is_empty(), "mid-transition overlay must draw");
    }

    #[test]
    fn unmatched_or_empty_shared_ids_produce_no_plan() {
        let unsynced = timeline(
            r#"
#0s
a: Rect, at: (0, 0), size: (10, 10), color: (1, 1, 1, 1), shared_id: "only-here"
"#,
        );
        let plain_from = timeline(
            r#"
#0s
a: Rect, at: (0, 0), size: (10, 10), color: (1, 1, 1, 1)
"#,
        );
        let plain_to = timeline(
            r#"
#0s
b: Rect, at: (50, 50), size: (20, 20), color: (1, 1, 1, 1), shared_id: ""
"#,
        );

        for (from, to) in [(&unsynced, &plain_to), (&plain_from, &plain_to)] {
            let plan = plan_shared_actors(from, 0.0, to, 0.0);
            assert!(plan.is_empty());
            assert!(plan.suppressed_from.is_empty());
            assert!(plan.suppressed_to.is_empty());
            assert!(
                collect_flip_pairs(
                    &plan,
                    from,
                    0.0,
                    to,
                    0.0,
                    dims(),
                    DebugRenderOptions::default(),
                )
                .is_empty()
            );
        }
    }

    #[test]
    fn multiple_shared_ids_are_planned_in_deterministic_order() {
        let from = timeline(
            r#"
#0s
z_actor: Rect, at: (0, 0), size: (10, 10), color: (1, 1, 1, 1), shared_id: "z"
a_actor: Rect, at: (0, 50), size: (10, 10), color: (1, 1, 1, 1), shared_id: "a"
"#,
        );
        let to = timeline(
            r#"
#0s
z_next: Rect, at: (100, 0), size: (20, 20), color: (1, 1, 1, 1), shared_id: "z"
a_next: Rect, at: (100, 50), size: (20, 20), color: (1, 1, 1, 1), shared_id: "a"
"#,
        );

        let plan = plan_shared_actors(&from, 0.0, &to, 0.0);
        assert_eq!(
            plan.pairs,
            vec![
                ("a_actor".to_string(), "a_next".to_string()),
                ("z_actor".to_string(), "z_next".to_string()),
            ]
        );
    }
}
