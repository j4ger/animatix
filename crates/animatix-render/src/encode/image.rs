//! Image (PNG) single-frame export.
//!
//! Single-timeline and multi-scene composition image rendering to PNG files
//! using the `image` crate.

use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use tracing::info;

use crate::encode::{ExportError, ExportOptions};
use crate::offscreen::OffscreenRenderer;

/// Render a single timeline to a PNG image at the given time with default debug
/// options.
pub fn render_image_timeline(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_image_timeline_with_options(
        timeline,
        width,
        height,
        time,
        output_file,
        DebugRenderOptions::default(),
        ExportOptions::default(),
    )
}

/// Render a single timeline to a PNG image at the given time.
pub fn render_image_timeline_with_options(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
) -> Result<(), ExportError> {
    render_image_timeline_with_options_and_grayscale(
        timeline,
        width,
        height,
        time,
        output_file,
        debug_options,
        options,
        false,
    )
}

/// Render a single timeline to an image at the given time, optionally
/// converting the rendered RGB channels to grayscale before encoding.
///
/// The alpha channel is preserved. Existing callers can continue using
/// [`render_image_timeline_with_options`] for color output.
pub fn render_image_timeline_with_options_and_grayscale(
    timeline: Timeline,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
    options: ExportOptions<'_>,
    grayscale: bool,
) -> Result<(), ExportError> {
    // A single frame has no thread configuration to apply and no frames to
    // count; cancellation is still honoured before the GPU work starts.
    let ExportOptions { cancel, .. } = options;
    if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
        return Err(ExportError::Cancelled);
    }
    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let frame = renderer
        .render_canvas(&timeline, time as f64, SceneDimensions { width, height }, debug_options)
        .map_err(|e| ExportError::FrameRender {
            frame: 0,
            message: e,
        })?;
    save_rendered_image(frame.width, frame.height, &frame.rgba, output_file, grayscale)?;
    // Headless workflows (CI, scripts) have no GUI diagnostics panel — drain
    // the frame's runtime diagnostics so frame-time failures (plot closure
    // errors, filter fallbacks) are visible on stderr too.
    for diagnostic in timeline.runtime_diagnostics() {
        let location = diagnostic
            .location
            .subject
            .clone()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        tracing::warn!("[{}] {}{location}", diagnostic.severity, diagnostic.message);
    }
    Ok(())
}

/// Render a multi-scene composition to a PNG image at the given time.
///
/// A composition frame is a single raster with no frames to count, so there is
/// no progress/cancel variant of this entry point.
pub fn render_image_composition(
    composition: &Composition,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    render_image_composition_with_grayscale(composition, width, height, time, output_file, false)
}

/// Render a multi-scene composition to an image, optionally converting its
/// final composited RGB channels to grayscale before encoding.
///
/// Transitions are rendered and composited before this conversion. The alpha
/// channel is preserved. Existing callers can continue using
/// [`render_image_composition`] for color output.
pub fn render_image_composition_with_grayscale(
    composition: &Composition,
    width: u32,
    height: u32,
    time: f32,
    output_file: &std::path::Path,
    grayscale: bool,
) -> Result<(), ExportError> {
    if !composition.has_scenes() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "Composition has no scenes to render".into(),
        });
    }

    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let (scene_name, local_time_s, transition_blend) = composition.evaluate(time as f64);

    let dims = SceneDimensions { width, height };

    let frame =
        if let Some(blend) = transition_blend {
            let from_scene = composition.scenes.get(&blend.from_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: 0,
                    message: format!("From scene '{}' not found in composition", blend.from_scene),
                }
            })?;
            let to_scene = composition.scenes.get(&blend.to_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: 0,
                    message: format!("To scene '{}' not found in composition", blend.to_scene),
                }
            })?;
            renderer
                .render_canvas_transition(
                    &from_scene.timeline,
                    blend.from_local,
                    &to_scene.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    dims,
                    DebugRenderOptions::default(),
                )
                .map_err(|e| ExportError::FrameRender {
                    frame: 0,
                    message: e,
                })?
        } else {
            let scene =
                composition.scenes.get(&scene_name).ok_or_else(|| ExportError::FrameRender {
                    frame: 0,
                    message: format!("Scene '{}' not found in composition", scene_name),
                })?;

            renderer
                .render_canvas(&scene.timeline, local_time_s, dims, DebugRenderOptions::default())
                .map_err(|e| ExportError::FrameRender {
                    frame: 0,
                    message: e,
                })?
        };

    save_rendered_image(frame.width, frame.height, &frame.rgba, output_file, grayscale)
}

fn save_rendered_image(
    width: u32,
    height: u32,
    rgba: &[u8],
    output_file: &std::path::Path,
    grayscale: bool,
) -> Result<(), ExportError> {
    let img = image_for_output(width, height, rgba, grayscale)?;
    img.save(output_file).map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    info!("Image saved to {}", output_file.display());
    Ok(())
}

fn image_for_output(
    width: u32,
    height: u32,
    rgba: &[u8],
    grayscale: bool,
) -> Result<image::RgbaImage, ExportError> {
    let mut img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| ExportError::ImageEncode("Failed to create image buffer".into()))?;
    if grayscale {
        grayscale_rgba(&mut img);
    }
    Ok(img)
}

fn grayscale_rgba(rgba: &mut image::RgbaImage) {
    for pixel in rgba.pixels_mut() {
        let [red, green, blue, alpha] = pixel.0;
        // Match Pillow's ImageOps.grayscale RGB conversion, including its
        // fixed-point coefficients and rounding, while retaining source alpha.
        let gray = ((u32::from(red) * 19_595
            + u32::from(green) * 38_470
            + u32::from(blue) * 7_471
            + 32_768)
            >> 16) as u8;
        *pixel = image::Rgba([gray, gray, gray, alpha]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_matches_pillow_rgb_weights_and_preserves_alpha() {
        let source = [
            255, 0, 0, 0, // transparent red
            0, 255, 0, 37, // translucent green
            0, 0, 255, 128, // translucent blue
            100, 150, 200, 64, // mixed color
            255, 255, 255, 255,
        ];
        let mut image = image::RgbaImage::from_raw(5, 1, source.to_vec()).unwrap();

        grayscale_rgba(&mut image);

        assert_eq!(
            image.as_raw(),
            &[
                76, 76, 76, 0, 150, 150, 150, 37, 29, 29, 29, 128, 141, 141, 141, 64, 255, 255,
                255, 255,
            ]
        );
    }

    #[test]
    fn color_output_keeps_source_rgba_unchanged() {
        let source = [12, 95, 201, 0, 31, 143, 220, 77];

        let image = image_for_output(2, 1, &source, false).unwrap();

        assert_eq!(image.as_raw(), &source);
    }

    #[test]
    fn grayscale_png_and_webp_keep_alpha_and_gray_channels_equal() {
        let source = [255, 0, 0, 0, 100, 150, 200, 83];
        let root = tempfile::tempdir().unwrap();

        for extension in ["png", "webp"] {
            let output = root.path().join(format!("frame.{extension}"));
            save_rendered_image(2, 1, &source, &output, true).unwrap();
            let decoded = image::open(&output).unwrap().to_rgba8();
            assert_eq!(decoded.get_pixel(0, 0).0, [76, 76, 76, 0]);
            let pixel = decoded.get_pixel(1, 0).0;
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
            assert_eq!(pixel[3], 83);
        }
    }
}
