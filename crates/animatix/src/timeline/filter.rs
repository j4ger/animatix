//! Filter system — backend trait and CPU-based image processing.

use crate::timeline::image::SceneImage;
use crate::timeline::SceneDimensions;

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

/// Backend that can render a [`vello::Scene`] to a [`SceneImage`].
///
/// The timeline uses this to capture a Filter actor's children into a bitmap,
/// then applies CPU-based post-processing (blur, brightness, etc.) and draws
/// the result back into the main scene.
pub trait FilterBackend: Send {
    /// Render `scene` (which covers `dimensions`) into a [`SceneImage`].
    fn render_scene_to_image(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String>;

    /// GPU-accelerated version: render the scene and apply filters on the GPU,
    /// then readback once.  The default implementation delegates to
    /// [`render_scene_to_image`] followed by [`apply_cpu_filters`].
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        blur: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
    ) -> Result<SceneImage, String> {
        let image = self.render_scene_to_image(scene, dimensions)?;
        Ok(apply_cpu_filters(
            image, blur, brightness, contrast, saturate, hue_rotate, sepia,
        ))
    }

    /// Render a scene with GPU filtering and store the result as a pending
    /// composite that can be blitted onto the render target without CPU readback.
    /// Returns Err if this backend doesn't support zero-readback compositing.
    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        blur: f32,
        brightness: f32,
        contrast: f32,
        saturate: f32,
        hue_rotate: f32,
        sepia: f32,
        alpha: f32,
    ) -> Result<(), String> {
        let _ = (scene, dimensions, blur, brightness, contrast, saturate, hue_rotate, sepia, alpha);
        Err("zero-readback filter compositing is not supported by this backend".to_string())
    }

    /// Drain any pending composites produced by `render_scene_to_pending_composite`.
    fn take_pending_composites(&mut self) -> Vec<PendingComposite> {
        Vec::new()
    }
}

/// Apply CPU-based filter operations to a [`SceneImage`].
///
/// Uses the `image` crate for Gaussian blur, brightness, contrast, hue rotate,
/// and grayscale. Sepia is implemented as a custom color matrix.
///
/// The pipeline order is: **blur → color matrix**.
pub fn apply_cpu_filters(
    image: SceneImage,
    blur: f32,
    brightness: f32,
    contrast: f32,
    saturate: f32,
    hue_rotate: f32,
    sepia: f32,
) -> SceneImage {
    let width = image.natural_size[0] as u32;
    let height = image.natural_size[1] as u32;

    // Convert peniko ImageData to image::RgbaImage
    let raw: Vec<u8> = image.data.data.data().to_vec();
    let mut img = match image::RgbaImage::from_raw(width, height, raw) {
        Some(img) => img,
        None => return image,
    };

    // ── Blur ──
    if blur > 0.5 {
        let sigma = blur / 3.0;
        img = image::imageops::blur(&img, sigma);
    }

    // ── Color matrix (brightness, contrast, saturate, hue, sepia) ──
    let needs_color_matrix = (brightness - 1.0).abs() > 0.001
        || (contrast - 1.0).abs() > 0.001
        || saturate < 0.999
        || hue_rotate.abs() > 0.5
        || sepia > 0.001;

    if needs_color_matrix {
        apply_color_matrix(&mut img, brightness, contrast, saturate, hue_rotate, sepia);
    }

    let raw_out = img.into_raw();
    let data = vello::peniko::ImageData {
        data: raw_out.into(),
        format: vello::peniko::ImageFormat::Rgba8,
        alpha_type: vello::peniko::ImageAlphaType::Alpha,
        width,
        height,
    };

    SceneImage {
        data,
        natural_size: image.natural_size,
    }
}

/// Compose a 4×4 color matrix from individual transforms.
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

/// Apply a combined color matrix for brightness, contrast, saturation,
/// hue rotation, and sepia.
fn apply_color_matrix(
    img: &mut image::RgbaImage,
    brightness: f32,
    contrast: f32,
    saturate: f32,
    hue_rotate: f32,
    sepia: f32,
) {
    let m = compose_color_matrix(brightness, contrast, saturate, hue_rotate, sepia);

    for pixel in img.pixels_mut() {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;
        let a = pixel[3] as f32 / 255.0;

        let nr = m[0][0] * r + m[0][1] * g + m[0][2] * b + m[0][3] * a;
        let ng = m[1][0] * r + m[1][1] * g + m[1][2] * b + m[1][3] * a;
        let nb = m[2][0] * r + m[2][1] * g + m[2][2] * b + m[2][3] * a;
        let na = m[3][0] * r + m[3][1] * g + m[3][2] * b + m[3][3] * a;

        pixel[0] = (nr.clamp(0.0, 1.0) * 255.0) as u8;
        pixel[1] = (ng.clamp(0.0, 1.0) * 255.0) as u8;
        pixel[2] = (nb.clamp(0.0, 1.0) * 255.0) as u8;
        pixel[3] = (na.clamp(0.0, 1.0) * 255.0) as u8;
    }
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
        [lr + cos_a * (1.0 - lr) + sin_a * (-lr), lg + cos_a * (-lg) + sin_a * (-lg), lb + cos_a * (-lb) + sin_a * (1.0 - lb), 0.0],
        [lr + cos_a * (-lr) + sin_a * 0.143, lg + cos_a * (1.0 - lg) + sin_a * 0.140, lb + cos_a * (-lb) + sin_a * (-0.283), 0.0],
        [lr + cos_a * (-lr) + sin_a * (-(1.0 - lr)), lg + cos_a * (-lg) + sin_a * lg, lb + cos_a * (1.0 - lb) + sin_a * lb, 0.0],
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
