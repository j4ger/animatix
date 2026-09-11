//! Effect chain model and the GPU filter backend boundary.
//!
//! Post-processing is modelled as an ordered [`EffectChain`] of
//! [`EffectInstance`]s, each described by an [`EffectDescriptor`] (parameters,
//! spatial support, and an ordered list of compute passes). The renderer
//! implements [`FilterBackend`] and owns all device resources; this module is
//! pure data so it stays free of `wgpu`/`vello` scene types.
//!
//! The normative contract for the pass layout, uniforms, identity semantics,
//! and failure policy lives in `docs/effects.md`.

use crate::timeline::SceneDimensions;
use crate::timeline::image::SceneImage;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A GPU texture that should be composited after the main Vello scene render.
/// Used by the zero-readback filter compositing path.
pub struct PendingComposite {
    /// Owns the copied filtered texture so `view` remains valid.
    pub texture: wgpu::Texture,
    /// Texture view sampled by the fullscreen compositor.
    pub view: wgpu::TextureView,
    /// Opacity to apply during compositing.
    pub alpha: f32,
}

// ── Effect identity ─────────────────────────────────────────────────────────

/// Stable identity of an effect, used as the pipeline-cache key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum EffectId {
    /// Gaussian blur; two passes (horizontal, then vertical).
    Blur,
    /// Colour matrix built from brightness/contrast/saturate/hue/sepia.
    ColorGrade,
}

// ── Parameter schema ────────────────────────────────────────────────────────

/// Value type of an effect parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectParamKind {
    /// 32-bit float.
    F32,
    /// 32-bit unsigned integer.
    U32,
    /// Two 32-bit floats.
    Vec2,
    /// Four 32-bit floats.
    Vec4,
    /// Boolean, marshalled as `u32`.
    Bool,
}

/// One resolved parameter value for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectParamValue {
    /// `f32`
    F32(f32),
    /// `u32`
    U32(u32),
    /// `[f32; 2]`
    Vec2([f32; 2]),
    /// `[f32; 4]`
    Vec4([f32; 4]),
    /// `bool`
    Bool(bool),
}

/// Declares one author parameter: its type, the identity value that means "no
/// contribution", and where it lands in the author uniform buffer.
#[derive(Clone, Copy, Debug)]
pub struct EffectParamSpec {
    /// Parameter name as authored in `.amx`.
    pub name: &'static str,
    /// Value type.
    pub kind: EffectParamKind,
    /// Value at which this parameter contributes nothing.
    pub identity: EffectParamValue,
    /// Byte offset in the author uniform buffer (generic packing).
    pub offset: u32,
    /// Byte size in the author uniform buffer.
    pub size: u32,
}

/// Resolved parameters for one effect instance in a frame.
#[derive(Clone, Debug, Default)]
pub struct EffectParams {
    /// Values index-aligned with [`EffectDescriptor::params`].
    pub values: Vec<EffectParamValue>,
}

impl EffectParams {
    /// Read parameter `index` as `f32` (0.0 for non-numeric or out of range).
    pub fn f32_at(&self, index: usize) -> f32 {
        match self.values.get(index) {
            Some(EffectParamValue::F32(v)) => *v,
            Some(EffectParamValue::U32(v)) => *v as f32,
            _ => 0.0,
        }
    }
}

// ── Passes and descriptors ──────────────────────────────────────────────────

/// One compute pass of an effect: shader source plus entry point.
#[derive(Clone, Copy, Debug)]
pub struct EffectPassSpec {
    /// Human-readable label for the pipeline/pass.
    pub label: &'static str,
    /// WGSL source. May be shared between passes (blur uses one shader twice).
    pub wgsl: &'static str,
    /// Entry point name.
    pub entry: &'static str,
}

/// Packs author parameters into the uniform bytes for one effect.
pub type EffectPackFn = fn(&EffectParams, &mut [u8]);

/// A complete effect description.
pub struct EffectDescriptor {
    /// Pipeline-cache identity.
    pub id: EffectId,
    /// Authored type name (`.amx`).
    pub type_name: &'static str,
    /// Parameter schema, index-aligned with [`EffectParams::values`].
    pub params: &'static [EffectParamSpec],
    /// Ordered compute passes.
    pub passes: &'static [EffectPassSpec],
    /// Size in bytes of the author uniform buffer (multiple of 16).
    pub author_uniform_size: u32,
    /// Marshals parameters into `author_uniform_size` bytes.
    pub pack: EffectPackFn,
}

impl EffectDescriptor {
    /// `true` when every parameter equals its identity value — the effect
    /// contributes nothing and its passes can be skipped.
    pub fn is_identity(&self, params: &EffectParams) -> bool {
        self.params
            .iter()
            .enumerate()
            .all(|(i, spec)| params.values.get(i).is_none_or(|value| *value == spec.identity))
    }
}

/// One resolved effect in a chain.
#[derive(Clone, Debug)]
pub struct EffectInstance {
    /// Which effect to run.
    pub id: EffectId,
    /// Whether the instance is enabled this frame.
    pub enabled: bool,
    /// Resolved parameters.
    pub params: EffectParams,
}

/// An ordered list of effects to apply to a compositing scope.
#[derive(Clone, Debug, Default)]
pub struct EffectChain {
    /// Active instances, in application order.
    pub instances: Vec<EffectInstance>,
    /// Timeline time in milliseconds, supplied to shaders as `time_ms`.
    pub time_ms: f32,
}

impl EffectChain {
    /// `true` when no effect will run (empty or every instance disabled).
    pub fn is_empty(&self) -> bool {
        self.instances.iter().all(|instance| !instance.enabled)
    }

    /// Build a chain from the legacy flat `Filter` properties.
    ///
    /// Temporary bridge while the flat `blur:`/`brightness:`/... properties
    /// still exist; removed when effects become child primitives. Instances at
    /// their identity values are omitted, so `is_empty` mirrors the old
    /// "needs filter" test.
    pub fn from_flat_filter(
        time_ms: f32,
        blur: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
    ) -> Self {
        let mut instances = Vec::new();
        if blur > 0.5 {
            instances.push(EffectInstance {
                id: EffectId::Blur,
                enabled: true,
                params: EffectParams {
                    values: vec![EffectParamValue::F32(blur)],
                },
            });
        }
        let color_is_identity = (brightness - 1.0).abs() <= 0.001
            && (contrast - 1.0).abs() <= 0.001
            && (saturate - 1.0).abs() <= 0.001
            && hue_rotate.abs() <= 0.5
            && sepia <= 0.001;
        if !color_is_identity {
            instances.push(EffectInstance {
                id: EffectId::ColorGrade,
                enabled: true,
                params: EffectParams {
                    values: vec![
                        EffectParamValue::F32(brightness),
                        EffectParamValue::F32(contrast),
                        EffectParamValue::F32(saturate),
                        EffectParamValue::F32(hue_rotate),
                        EffectParamValue::F32(sepia),
                    ],
                },
            });
        }
        Self { instances, time_ms }
    }
}

/// Descriptor for a built-in effect.
pub fn descriptor(id: EffectId) -> &'static EffectDescriptor {
    match id {
        EffectId::Blur => &BLUR_DESCRIPTOR,
        EffectId::ColorGrade => &COLOR_GRADE_DESCRIPTOR,
    }
}

/// Look up a built-in effect descriptor by its authored type name.
///
/// Returns `None` for an unknown type; plugin effects go through the
/// extension registry instead (Stage 4).
pub fn descriptor_for_type(type_name: &str) -> Option<&'static EffectDescriptor> {
    match type_name {
        "Blur" => Some(&BLUR_DESCRIPTOR),
        "ColorGrade" => Some(&COLOR_GRADE_DESCRIPTOR),
        _ => None,
    }
}

// ── Built-in parameter schemas ──────────────────────────────────────────────

/// `Blur` parameters.
pub const BLUR_PARAMS: &[EffectParamSpec] = &[EffectParamSpec {
    name: "radius",
    kind: EffectParamKind::F32,
    identity: EffectParamValue::F32(0.0),
    offset: 0,
    size: 4,
}];

/// `ColorGrade` parameters.
pub const COLOR_GRADE_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec {
        name: "brightness",
        kind: EffectParamKind::F32,
        identity: EffectParamValue::F32(1.0),
        offset: 0,
        size: 4,
    },
    EffectParamSpec {
        name: "contrast",
        kind: EffectParamKind::F32,
        identity: EffectParamValue::F32(1.0),
        offset: 4,
        size: 4,
    },
    EffectParamSpec {
        name: "saturate",
        kind: EffectParamKind::F32,
        identity: EffectParamValue::F32(1.0),
        offset: 8,
        size: 4,
    },
    EffectParamSpec {
        name: "hue_rotate",
        kind: EffectParamKind::F32,
        identity: EffectParamValue::F32(0.0),
        offset: 12,
        size: 4,
    },
    EffectParamSpec {
        name: "sepia",
        kind: EffectParamKind::F32,
        identity: EffectParamValue::F32(0.0),
        offset: 16,
        size: 4,
    },
];

// ── Built-in WGSL ───────────────────────────────────────────────────────────

const BLUR_WGSL: &str = r#"
struct BlurParams {
    radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

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

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: BlurParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    let size = vec2<i32>(ctx.tex_size);

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    if (params.radius < 0.5) {
        textureStore(dst, coord, textureLoad(src, coord, 0));
        return;
    }

    let sigma = params.radius / 3.0;
    let radius = i32(ceil(params.radius));

    var color = vec4<f32>(0.0);
    var weight_sum = 0.0;

    for (var i = -radius; i <= radius; i = i + 1) {
        var offset: vec2<i32>;
        if (ctx.pass_index == 0u) {
            offset = vec2<i32>(i, 0);
        } else {
            offset = vec2<i32>(0, i);
        }
        let sample_coord = clamp(coord + offset, vec2<i32>(0), size - vec2<i32>(1));
        let w = gaussian_weight(f32(i), sigma);
        color = color + textureLoad(src, sample_coord, 0) * w;
        weight_sum = weight_sum + w;
    }

    textureStore(dst, coord, color / weight_sum);
}
"#;

const COLOR_GRADE_WGSL: &str = r#"
struct ColorGradeParams {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    m3: vec4<f32>,
}

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

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> params: ColorGradeParams;
@group(0) @binding(3) var<uniform> ctx: EffectContext;
@group(0) @binding(4) var samp: sampler;

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let coord = vec2<u32>(gid.x, gid.y);
    let size = vec2<u32>(textureDimensions(src));

    if (coord.x >= size.x || coord.y >= size.y) {
        return;
    }

    let texel = textureLoad(src, vec2<i32>(coord), 0);
    let rgba = vec4<f32>(texel.r, texel.g, texel.b, texel.a);

    let r = dot(params.m0, rgba);
    let g = dot(params.m1, rgba);
    let b = dot(params.m2, rgba);
    let a = dot(params.m3, rgba);

    let out = vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), clamp(a, 0.0, 1.0));
    textureStore(dst, coord, out);
}
"#;

// ── Built-in descriptors ────────────────────────────────────────────────────

/// `Blur` descriptor: one shader, two passes (horizontal then vertical).
pub static BLUR_DESCRIPTOR: EffectDescriptor = EffectDescriptor {
    id: EffectId::Blur,
    type_name: "Blur",
    params: BLUR_PARAMS,
    passes: &[
        EffectPassSpec {
            label: "blur-horizontal",
            wgsl: BLUR_WGSL,
            entry: "main",
        },
        EffectPassSpec {
            label: "blur-vertical",
            wgsl: BLUR_WGSL,
            entry: "main",
        },
    ],
    author_uniform_size: 16,
    pack: pack_blur,
};

/// `ColorGrade` descriptor: one pass mapping five scalars to a colour matrix.
pub static COLOR_GRADE_DESCRIPTOR: EffectDescriptor = EffectDescriptor {
    id: EffectId::ColorGrade,
    type_name: "ColorGrade",
    params: COLOR_GRADE_PARAMS,
    passes: &[EffectPassSpec {
        label: "color-grade",
        wgsl: COLOR_GRADE_WGSL,
        entry: "main",
    }],
    author_uniform_size: 64,
    pack: pack_color_grade,
};

fn pack_blur(params: &EffectParams, out: &mut [u8]) {
    out.fill(0);
    let radius = params.f32_at(0);
    out[0..4].copy_from_slice(&radius.to_le_bytes());
}

fn pack_color_grade(params: &EffectParams, out: &mut [u8]) {
    out.fill(0);
    let matrix = compose_color_matrix(
        params.f32_at(0),
        params.f32_at(1),
        params.f32_at(2),
        params.f32_at(3),
        params.f32_at(4),
    );
    for (row, values) in matrix.iter().enumerate() {
        let base = row * 16;
        for (column, value) in values.iter().enumerate() {
            let start = base + column * 4;
            out[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
}

// ── Backend boundary ────────────────────────────────────────────────────────

/// Backend that can render a [`vello::Scene`] and apply an [`EffectChain`].
///
/// The timeline captures a `Filter` scope's content children into an offscreen
/// scene, hands the chain to the backend, and composites the result back into
/// the parent scene. There is no CPU fallback: a backend that cannot run the
/// chain reports an error and the timeline renders the children unfiltered with
/// a diagnostic (`docs/effects.md` §5).
pub trait FilterBackend: Send {
    /// Render `scene` (covering `dimensions`), apply `chain`, and read the
    /// result back as a [`SceneImage`].
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        chain: &EffectChain,
    ) -> Result<SceneImage, String>;

    /// Render a scene with `chain` and store the result as a pending composite
    /// that can be blitted onto the render target without CPU readback.
    /// Returns `Err` if this backend doesn't support zero-readback compositing.
    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        let _ = (scene, dimensions, chain, alpha);
        Err("zero-readback effect compositing is not supported by this backend".to_string())
    }

    /// Drain any pending composites produced by `render_scene_to_pending_composite`.
    fn take_pending_composites(&mut self) -> Vec<PendingComposite> {
        Vec::new()
    }
}

// ── Colour matrix helpers (used by the ColorGrade packer) ───────────────────

/// Compose a 4×4 colour matrix from individual transforms.
///
/// Order of composition: **sepia → hue → saturate → contrast → brightness**.
/// Returns the matrix in row-major form.
pub fn compose_color_matrix(
    brightness: f32,
    contrast: f32,
    saturate: f32,
    hue_rotate: f32,
    sepia: f32,
) -> [[f32; 4]; 4] {
    let mut m = identity_matrix();

    if sepia > 0.001 {
        m = multiply_matrix(&sepia_matrix(sepia), &m);
    }
    if hue_rotate.abs() > 0.5 {
        m = multiply_matrix(&hue_matrix(hue_rotate.to_radians()), &m);
    }
    if (saturate - 1.0).abs() > 0.001 {
        m = multiply_matrix(&saturation_matrix(saturate), &m);
    }
    if (contrast - 1.0).abs() > 0.001 {
        m = multiply_matrix(&contrast_matrix(contrast), &m);
    }
    if (brightness - 1.0).abs() > 0.001 {
        m = multiply_matrix(&brightness_matrix(brightness), &m);
    }

    m
}

fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn brightness_matrix(b: f32) -> [[f32; 4]; 4] {
    [
        [b, 0.0, 0.0, 0.0],
        [0.0, b, 0.0, 0.0],
        [0.0, 0.0, b, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn contrast_matrix(c: f32) -> [[f32; 4]; 4] {
    let t = (1.0 - c) * 0.5;
    [
        [c, 0.0, 0.0, t],
        [0.0, c, 0.0, t],
        [0.0, 0.0, c, t],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn saturation_matrix(s: f32) -> [[f32; 4]; 4] {
    let lr = 0.2126;
    let lg = 0.7152;
    let lb = 0.0722;
    let is = 1.0 - s;
    [
        [lr * is + s, lg * is, lb * is, 0.0],
        [lr * is, lg * is + s, lb * is, 0.0],
        [lr * is, lg * is, lb * is + s, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn hue_matrix(angle: f32) -> [[f32; 4]; 4] {
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let lr = 0.2126;
    let lg = 0.7152;
    let lb = 0.0722;
    [
        [
            lr + cos_a * (1.0 - lr) + sin_a * (-lr),
            lg + cos_a * (-lg) + sin_a * (-lg),
            lb + cos_a * (-lb) + sin_a * (1.0 - lb),
            0.0,
        ],
        [
            lr + cos_a * (-lr) + sin_a * 0.143,
            lg + cos_a * (1.0 - lg) + sin_a * 0.140,
            lb + cos_a * (-lb) + sin_a * (-0.283),
            0.0,
        ],
        [
            lr + cos_a * (-lr) + sin_a * (-(1.0 - lr)),
            lg + cos_a * (-lg) + sin_a * lg,
            lb + cos_a * (1.0 - lb) + sin_a * lb,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn sepia_matrix(s: f32) -> [[f32; 4]; 4] {
    let is = 1.0 - s;
    [
        [0.393 * s + is, 0.769 * s, 0.189 * s, 0.0],
        [0.349 * s, 0.686 * s + is, 0.168 * s, 0.0],
        [0.272 * s, 0.534 * s, 0.131 * s + is, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn multiply_matrix(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                result[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    result
}
