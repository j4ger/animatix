//! Render commands emitted by primitive evaluation.
//!
//! # Extension boundary
//!
//! Plugins emit commands through the fixed `append_*` callbacks in
//! `animatix-plugin-api`. Engine-only variants need no new ABI callback.
//! Keep [`RenderCommand::execute`] and [`RenderCommand::local_bounds`] in sync.

use crate::renderer::types::{TextPath, VelloPath};
use crate::scene::{Glyph, Scene};

/// A command to render content into a scene.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum RenderCommand {
    /// Draw a set of vector paths, each with its own fill and stroke.
    Paths {
        /// The vector paths to draw.
        paths: Vec<VelloPath>,
    },
    /// Paint a rounded surface with solid colors.
    RoundedSurface {
        /// Local-space geometry.
        rect: kurbo::Rect,
        /// Corner radius in scene units.
        radius: f32,
        /// Fill color as sRGB bytes. A zero alpha skips paint.
        fill: [u8; 4],
        /// Stroke color as sRGB bytes. A zero alpha skips paint.
        stroke: [u8; 4],
        /// Stroke width in scene units.
        stroke_width: f32,
    },
    /// Draw text glyphs.
    Text {
        /// Text glyph paths with per-glyph color and opacity.
        paths: std::sync::Arc<[TextPath]>,
        /// Optional paint that ramps across the text fill.
        fill_gradient: Option<Box<crate::renderer::types::GradientSpec>>,
        /// Optional clip bounds (e.g. for a horizontal text wipe).
        clip_path: Option<kurbo::BezPath>,
        /// Optional uniform tint color override (for Text and Counter primitives).
        tint_color: Option<[u8; 4]>,
        /// Optional direct hardware glyph layout for Vello GPU draw_glyphs streaming.
        hardware_layout: Option<std::sync::Arc<animatix_text::HardwareTextLayout>>,
    },
    /// Draw an image.
    Image {
        /// The image data.
        image: crate::timeline::image::SceneImage,
        /// Natural (display) width and height in scene units.
        natural_size: [f32; 2],
        /// Local-space placement offset.
        offset: [f64; 2],
    },
    /// A video frame with its fitted image transform and local clip.
    ClippedImage {
        /// Decoded image data.
        image: crate::timeline::image::SceneImage,
        /// Full display size of the source image before clipping.
        size: [f32; 2],
        /// Offset of the full image in actor space.
        offset: [f64; 2],
        /// Visible portion in actor space.
        clip: kurbo::Rect,
    },
    /// A highlight layer drawn with a specific blend mode (for equation fragment highlights).
    HighlightLayer {
        /// The rounded rectangle geometry.
        rect: kurbo::Rect,
        /// Fill color.
        color: peniko::Color,
        /// Blend mode (e.g. Difference, Multiply).
        blend: peniko::Mix,
        /// Layer alpha (0.0–1.0).
        alpha: f32,
        /// Corner radius.
        corner_radius: f64,
    },
    /// Rounded-rectangle drop shadow for cards and other rectangular surfaces.
    Shadow {
        /// Half size [w/2, h/2] of the base shape.
        size: [f32; 2],
        /// Corner radius.
        corner_radius: f32,
        /// [offset_x, offset_y, blur, spread].
        params: [f32; 4],
        /// Shadow color RGBA.
        color: [f32; 4],
    },
    /// Drop shadow following the filled surfaces and stroke outlines of a vector shape.
    PathShadow {
        /// Filled silhouettes in the actor's local coordinates.
        outlines: Vec<std::sync::Arc<kurbo::BezPath>>,
        /// [offset_x, offset_y, blur, spread].
        params: [f32; 4],
        /// Shadow color RGBA.
        color: [f32; 4],
    },
}

/// Translate text glyph paths by a local-space offset.
pub fn translate_text_paths(
    paths: std::sync::Arc<[crate::renderer::types::TextPath]>,
    x: f64,
    y: f64,
) -> std::sync::Arc<[crate::renderer::types::TextPath]> {
    if x == 0.0 && y == 0.0 {
        return paths;
    }
    paths
        .iter()
        .map(|tp| {
            let mut p = tp.clone();
            p.translate((x, y));
            p
        })
        .collect::<Vec<_>>()
        .into()
}

/// Compute the transform that maps a texture's pixel rect onto its display box.
///
/// `crate::scene::Scene::draw_image` fills a rect of `image_width × image_height`
/// scene units (1 texture pixel = 1 unit), so a texture of `pixels` pixels
/// needs a scale of `display / pixels` to appear at its requested display
/// size. `display` is the command's `natural_size` (full actor size in scene
/// units), `offset` shifts the box in local space before the actor transform.
pub fn image_display_transform(
    display: [f32; 2],
    pixels: [f32; 2],
    offset: [f64; 2],
) -> kurbo::Affine {
    let sx = (display[0] / pixels[0].max(1.0)) as f64;
    let sy = (display[1] / pixels[1].max(1.0)) as f64;
    kurbo::Affine::translate((offset[0], offset[1])) * kurbo::Affine::scale_non_uniform(sx, sy)
}

impl RenderCommand {
    /// Execute this command into an owned scene with the given transform and opacity.
    pub fn execute(&self, scene: &mut Scene, transform: &kurbo::Affine, opacity: f32) {
        match self {
            RenderCommand::Paths { paths } => execute_paths(paths, scene, transform, opacity),
            RenderCommand::RoundedSurface {
                rect,
                radius,
                fill,
                stroke,
                stroke_width,
            } => {
                let rounded = super::surface::rounded_rect(*rect, *radius);
                let paint = |[r, g, b, a]: [u8; 4]| {
                    (a > 0).then(|| {
                        let color = peniko::Color::from_rgba8(r, g, b, a);
                        color.with_alpha(color.components[3] * opacity)
                    })
                };
                let stroke = if *stroke_width > 0.0 {
                    paint(*stroke).map(|color| {
                        let style = kurbo::Stroke::new(f64::from(*stroke_width))
                            .with_join(kurbo::Join::Miter)
                            .with_miter_limit(10.0)
                            .with_caps(kurbo::Cap::Butt);
                        (style, color)
                    })
                } else {
                    None
                };
                scene.draw_rounded_rect(&rounded, *transform, paint(*fill), stroke);
            },
            RenderCommand::Text {
                paths,
                fill_gradient,
                clip_path,
                tint_color,
                hardware_layout,
            } => execute_text(
                paths,
                fill_gradient,
                clip_path,
                tint_color,
                hardware_layout,
                scene,
                transform,
                opacity,
            ),
            RenderCommand::Image {
                image,
                natural_size,
                offset,
            } => execute_image(image, natural_size, offset, scene, transform, opacity),
            RenderCommand::ClippedImage {
                image,
                size,
                offset,
                clip,
            } => {
                scene.push_clip(peniko::Fill::NonZero, *transform, clip);
                execute_image(image, size, offset, scene, transform, opacity);
                scene.pop_layer();
            },
            RenderCommand::HighlightLayer {
                rect,
                color,
                blend,
                alpha,
                corner_radius,
            } => execute_highlight_layer(
                rect,
                color,
                blend,
                alpha,
                corner_radius,
                scene,
                transform,
                opacity,
            ),
            RenderCommand::Shadow {
                size,
                corner_radius,
                params,
                color,
            } => execute_shadow(size, corner_radius, params, color, scene, transform, opacity),
            RenderCommand::PathShadow {
                outlines,
                params,
                color,
            } => {
                super::shadow::draw(outlines, *params, *color, scene, *transform, opacity);
            },
        }
    }

    /// Compute the local-space bounding box of this command's geometry.
    ///
    /// Returns `None` if the command has no drawable content.
    /// The bounding box is in local coordinates (before transform).
    pub fn local_bounds(&self, display_size: Option<[f32; 2]>) -> Option<kurbo::Rect> {
        use kurbo::Shape;
        let mut bounds: Option<kurbo::Rect> = None;
        let union = |acc: Option<kurbo::Rect>, rect: kurbo::Rect| -> Option<kurbo::Rect> {
            Some(match acc {
                Some(existing) => existing.union(rect),
                None => rect,
            })
        };
        match self {
            RenderCommand::Paths { paths } => {
                for path in paths {
                    bounds = union(bounds, path.path.bounding_box());
                }
            },
            RenderCommand::RoundedSurface { rect, .. } => return Some(*rect),
            RenderCommand::Text {
                paths, clip_path, ..
            } => {
                for text_path in paths.iter() {
                    if text_path.bounds.area() == 0.0 {
                        continue;
                    }
                    bounds = union(bounds, text_path.bounds);
                }
                if let Some(clip) = clip_path {
                    let clip_bounds = clip.bounding_box();
                    bounds =
                        bounds.map(|ink| ink.intersect(clip_bounds)).filter(|ink| ink.area() > 0.0);
                }
            },
            RenderCommand::Image { .. } => {
                if let Some([half_w, half_h]) = display_size {
                    // Centered on the local origin, matching the box the
                    // command actually draws.
                    bounds = union(
                        bounds,
                        kurbo::Rect::new(
                            (-half_w) as f64,
                            (-half_h) as f64,
                            half_w as f64,
                            half_h as f64,
                        ),
                    );
                }
            },
            RenderCommand::ClippedImage { clip, .. } => return Some(*clip),
            RenderCommand::HighlightLayer { rect, .. } => {
                bounds = union(bounds, *rect);
            },
            RenderCommand::Shadow { size, params, .. } => {
                let (half_w, half_h) =
                    (size[0] as f64 + params[3] as f64, size[1] as f64 + params[3] as f64);
                if half_w <= 0.0 || half_h <= 0.0 {
                    return None;
                }
                let blur_pad = params[2].max(0.0) as f64 * 3.0;
                let ox = params[0] as f64;
                let oy = params[1] as f64;
                bounds = union(
                    bounds,
                    kurbo::Rect::new(
                        ox - half_w - blur_pad,
                        oy - half_h - blur_pad,
                        ox + half_w + blur_pad,
                        oy + half_h + blur_pad,
                    ),
                );
            },
            RenderCommand::PathShadow {
                outlines, params, ..
            } => {
                bounds = super::shadow::bounds(outlines, *params);
            },
        }
        bounds
    }

    /// Count segments for commands that paint explicit vector geometry.
    pub fn path_segment_count(&self) -> Option<usize> {
        use kurbo::Shape;
        match self {
            Self::Paths { paths } => Some(paths.iter().map(|p| p.path.segments().count()).sum()),
            Self::RoundedSurface { rect, radius, .. } => {
                let rounded = super::surface::rounded_rect(*rect, *radius);
                Some(kurbo::segments(rounded.path_elements(0.1)).count())
            },
            _ => None, // Other commands do not represent ordinary vector paths.
        }
    }
}

/// Execute one `RenderCommand::Paths` arm: the fill (solid or gradient) and,
/// when present, the stroke of every path.
fn execute_paths(paths: &[VelloPath], scene: &mut Scene, transform: &kurbo::Affine, opacity: f32) {
    use kurbo::Shape as _;
    for path in paths {
        match (&path.fill, &path.fill_gradient) {
            (Some(fc), Some(grad)) => {
                let g = grad.to_peniko(path.path.bounding_box(), fc.components[3] * opacity);
                scene.fill(
                    peniko::Fill::NonZero,
                    *transform,
                    peniko::Brush::Gradient(g),
                    None,
                    path.path.as_ref(),
                );
            },
            (Some(fc), None) => {
                let fc = fc.with_alpha(fc.components[3] * opacity);
                scene.fill(peniko::Fill::NonZero, *transform, fc, None, path.path.as_ref());
            },
            // A ramp authored on a stroke-only shape paints no fill.
            (None, Some(_)) => {},
            (None, None) => {},
        }
        if let Some((mut sc, sw)) = path.stroke {
            sc = sc.with_alpha(sc.components[3] * opacity);
            let stroke = path_stroke(path, sw);
            match &path.stroke_gradient {
                Some(grad) => {
                    let g = grad.to_peniko(path.path.bounding_box(), sc.components[3] * opacity);
                    scene.stroke(
                        &stroke,
                        *transform,
                        peniko::Brush::Gradient(g),
                        None,
                        path.path.as_ref(),
                    );
                },
                None => scene.stroke(&stroke, *transform, sc, None, path.path.as_ref()),
            }
        }
    }
}

pub(super) fn path_stroke(path: &VelloPath, width: f32) -> kurbo::Stroke {
    let cap = match path.line_cap {
        1 => kurbo::Cap::Round,
        2 => kurbo::Cap::Square,
        _ => kurbo::Cap::Butt,
    };
    let join = match path.line_join {
        1 => kurbo::Join::Round,
        2 => kurbo::Join::Bevel,
        _ => kurbo::Join::Miter,
    };
    let dash: Vec<f64> = path
        .dash_pattern
        .as_ref()
        .map(|p| p.iter().map(|f| f64::from(*f)).collect())
        .unwrap_or_default();
    kurbo::Stroke {
        width: f64::from(width),
        join,
        miter_limit: 10.0,
        start_cap: cap,
        end_cap: cap,
        dash_pattern: dash.into(),
        dash_offset: f64::from(path.dash_offset),
    }
}

/// Execute one `RenderCommand::Text` arm: the hardware-glyph fast path, then the
/// per-glyph fill (gradient or tinted color), under an optional clip layer.
fn execute_text(
    paths: &std::sync::Arc<[TextPath]>,
    fill_gradient: &Option<Box<crate::renderer::types::GradientSpec>>,
    clip_path: &Option<kurbo::BezPath>,
    tint_color: &Option<[u8; 4]>,
    hardware_layout: &Option<std::sync::Arc<animatix_text::HardwareTextLayout>>,
    scene: &mut Scene,
    transform: &kurbo::Affine,
    opacity: f32,
) {
    if let Some(clip) = clip_path {
        scene.push_clip(peniko::Fill::NonZero, *transform, clip);
    }

    if fill_gradient.is_none()
        && opacity > 0.0
        && let Some(hw) = hardware_layout
    {
        let [r, g, b, a] = if let Some(tc) = tint_color {
            *tc
        } else if let Some(first) = paths.first() {
            first.color
        } else {
            [255, 255, 255, 255]
        };
        let brush = peniko::Brush::Solid(peniko::Color::from_rgba8(r, g, b, a));

        for run in &hw.runs {
            let font_data = peniko::FontData {
                data: peniko::Blob::new(std::sync::Arc::new(std::sync::Arc::clone(&run.font_data))),
                index: run.font_index,
            };
            let glyphs_iter = run.glyphs.iter().map(|g| Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            });
            let draw = scene.draw_glyphs(&font_data);
            draw.font_size(run.font_size)
                .transform(*transform)
                .brush(&brush)
                .brush_alpha(opacity)
                .normalized_coords(&run.normalized_coords)
                .draw(peniko::Fill::NonZero, glyphs_iter);
        }

        if clip_path.is_some() {
            scene.pop_layer();
        }
        return;
    }

    let bounds = if fill_gradient.is_some() {
        let mut b: Option<kurbo::Rect> = None;
        for tp in paths.iter() {
            let tp_b = tp.bounds;
            b = Some(match b {
                Some(existing) => existing.union(tp_b),
                None => tp_b,
            });
        }
        b.unwrap_or_else(|| kurbo::Rect::new(0.0, 0.0, 1.0, 1.0))
    } else {
        kurbo::Rect::ZERO
    };

    for text_path in paths.iter() {
        let glyph_alpha = opacity * text_path.opacity;
        if glyph_alpha <= 0.0 {
            continue;
        }
        for clip in &text_path.clips {
            scene.push_clip(peniko::Fill::NonZero, *transform, clip);
        }
        if let Some(grad) = fill_gradient {
            let g = grad.to_peniko(bounds, glyph_alpha);
            scene.fill(
                peniko::Fill::NonZero,
                *transform,
                peniko::Brush::Gradient(g),
                None,
                &text_path.path,
            );
        } else {
            let [r, g, b, a] = if let Some(tc) = tint_color {
                *tc
            } else {
                text_path.color
            };
            let color = peniko::Color::from_rgba8(r, g, b, (a as f32 * glyph_alpha) as u8);
            scene.fill(peniko::Fill::NonZero, *transform, color, None, &text_path.path);
        }
        for _ in &text_path.clips {
            scene.pop_layer();
        }
    }

    if clip_path.is_some() {
        scene.pop_layer();
    }
}

/// Execute one `RenderCommand::Image` arm: scale the texture's pixel rect onto
/// its display box, then draw it with the command opacity.
fn execute_image(
    image: &crate::timeline::image::SceneImage,
    natural_size: &[f32; 2],
    offset: &[f64; 2],
    scene: &mut Scene,
    transform: &kurbo::Affine,
    opacity: f32,
) {
    let image_transform =
        *transform * image_display_transform(*natural_size, image.natural_size, *offset);
    let brush = peniko::ImageBrush::new(image.data.clone())
        .with_extend(peniko::Extend::Pad)
        .with_quality(peniko::ImageQuality::Medium)
        .with_alpha(opacity);
    scene.draw_image(&brush, image_transform);
}

/// Execute one `RenderCommand::HighlightLayer` arm: push a blend-mode layer,
/// fill the rounded rect, then pop the layer.
fn execute_highlight_layer(
    rect: &kurbo::Rect,
    color: &peniko::Color,
    blend: &peniko::Mix,
    alpha: &f32,
    corner_radius: &f64,
    scene: &mut Scene,
    transform: &kurbo::Affine,
    opacity: f32,
) {
    let rounded = kurbo::RoundedRect::from_rect(*rect, *corner_radius);
    scene.push_layer(
        peniko::Fill::NonZero,
        peniko::BlendMode::new(*blend, peniko::Compose::SrcOver),
        *alpha * opacity,
        *transform,
        &rounded,
    );
    scene.fill(peniko::Fill::NonZero, *transform, *color, None, &rounded);
    scene.pop_layer();
}

/// Execute one `RenderCommand::Shadow` arm: a single rect for a sharp shadow, or
/// six erf-approximated slices for a blurred one.
fn execute_shadow(
    size: &[f32; 2],
    corner_radius: &f32,
    params: &[f32; 4],
    color: &[f32; 4],
    scene: &mut Scene,
    transform: &kurbo::Affine,
    opacity: f32,
) {
    let [dx, dy, blur, spread] = *params;
    let [r, g, b, a] = *color;
    let final_alpha = a * opacity;
    if final_alpha <= 0.001 {
        return;
    }
    let half_w = size[0] + spread;
    let half_h = size[1] + spread;
    if half_w <= 0.0 || half_h <= 0.0 {
        return;
    }
    if blur <= 0.5 {
        let shadow_rect = kurbo::Rect::new(
            (dx - half_w) as f64,
            (dy - half_h) as f64,
            (dx + half_w) as f64,
            (dy + half_h) as f64,
        );
        let peniko_color = peniko::Color::from_rgba8(
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8,
            (final_alpha * 255.0) as u8,
        );
        if *corner_radius > 0.0 {
            let rounded = kurbo::RoundedRect::from_rect(shadow_rect, *corner_radius as f64);
            scene.fill(peniko::Fill::NonZero, *transform, peniko_color, None, &rounded);
        } else {
            scene.fill(peniko::Fill::NonZero, *transform, peniko_color, None, &shadow_rect);
        }
    } else {
        let sigma = blur * 0.5;
        let steps = 6;
        for i in 0..steps {
            let t0 = i as f32 / steps as f32;
            let t1 = (i + 1) as f32 / steps as f32;
            let erf0 = (1.2 * (t0 * 3.0)).tanh();
            let erf1 = (1.2 * (t1 * 3.0)).tanh();
            let slice_alpha = (erf1 - erf0) * final_alpha;
            let pad = t1 * 3.0 * sigma;
            let shadow_rect = kurbo::Rect::new(
                (dx - half_w - pad) as f64,
                (dy - half_h - pad) as f64,
                (dx + half_w + pad) as f64,
                (dy + half_h + pad) as f64,
            );
            let peniko_color = peniko::Color::from_rgba8(
                (r * 255.0) as u8,
                (g * 255.0) as u8,
                (b * 255.0) as u8,
                (slice_alpha * 255.0).clamp(0.0, 255.0) as u8,
            );
            let cr = (*corner_radius + pad) as f64;
            let rounded = kurbo::RoundedRect::from_rect(shadow_rect, cr);
            scene.fill(peniko::Fill::NonZero, *transform, peniko_color, None, &rounded);
        }
    }
}
