//! `ColorGrade` — a 4×4 colour matrix composed on the host from
//! brightness/contrast/saturate/hue-rotate/sepia.
//!
//! This file is the single source for the effect: parameters (name, kind,
//! identity), WGSL passes, `pack`, and `support`.

use animatix_core::effect::{
    Effect, EffectParamKind, EffectParamSpec, EffectParamValue, EffectParams, EffectPassSpec,
};

/// `ColorGrade` parameters.
pub const COLOR_GRADE_PARAMS: &[EffectParamSpec] = &[
    EffectParamSpec::new("brightness", EffectParamKind::F32, EffectParamValue::F32(1.0), 0, 4),
    EffectParamSpec::new("contrast", EffectParamKind::F32, EffectParamValue::F32(1.0), 4, 4),
    EffectParamSpec::new("saturate", EffectParamKind::F32, EffectParamValue::F32(1.0), 8, 4),
    EffectParamSpec::new("hue_rotate", EffectParamKind::F32, EffectParamValue::F32(0.0), 12, 4),
    EffectParamSpec::new("sepia", EffectParamKind::F32, EffectParamValue::F32(0.0), 16, 4),
];

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

const COLOR_GRADE_PASSES: &[EffectPassSpec] =
    &[EffectPassSpec::new("color-grade", COLOR_GRADE_WGSL, "main")];

/// The `ColorGrade` effect.
pub struct ColorGrade;

/// Singleton instance of [`ColorGrade`].
pub const COLOR_GRADE: ColorGrade = ColorGrade;

impl Effect for ColorGrade {
    fn type_name(&self) -> &str {
        "ColorGrade"
    }

    fn display_name(&self) -> &'static str {
        "Color Grade"
    }

    fn params(&self) -> &[EffectParamSpec] {
        COLOR_GRADE_PARAMS
    }

    fn passes(&self) -> &[EffectPassSpec] {
        COLOR_GRADE_PASSES
    }

    /// The WGSL uniform struct is the hand-padded 4×4 matrix (4 × `vec4`), not
    /// the five packed scalars, so the derived size would under-allocate.
    fn author_uniform_size(&self) -> u32 {
        64
    }

    fn pack(&self, params: &EffectParams, out: &mut [u8]) {
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

    fn support(&self, _params: &EffectParams) -> f32 {
        0.0
    }
}

// ── Colour matrix helpers ───────────────────────────────────────────────────

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
