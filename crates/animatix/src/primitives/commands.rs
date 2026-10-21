//! Render commands emitted by primitive evaluation.
//!
//! # Extension boundary
//!
//! The set of commands a plugin can emit is currently fixed to the five
//! variants below (the plugin ABI exposes one `append_*` callback per
//! variant). Adding a new variant requires updating the ABI
//! (`animatix-plugin-api`), [`RenderCommand::execute`], and
//! [`RenderCommand::local_bounds`] together. The enum is `#[non_exhaustive]`
//! so external code must already handle unknown variants.

use crate::renderer::types::{TextPath, VelloPath};

/// A command to render content into a scene.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum RenderCommand {
    /// Draw a set of vector paths, each with its own fill and stroke.
    Paths {
        /// The vector paths to draw.
        paths: Vec<VelloPath>,
    },
    /// Draw text glyphs.
    Text {
        /// Text glyph paths with per-glyph color and opacity.
        paths: std::sync::Arc<[TextPath]>,
        /// Optional paint that ramps across the text fill.
        fill_gradient: Option<Box<crate::renderer::types::GradientSpec>>,
        /// Optional clip bounds (e.g. for split-text baseline reveal).
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
    /// A highlight layer drawn with a specific blend mode (for equation fragment highlights).
    HighlightLayer {
        /// The rounded rectangle geometry.
        rect: kurbo::Rect,
        /// Fill color.
        color: vello::peniko::Color,
        /// Blend mode (e.g. Difference, Multiply).
        blend: vello::peniko::Mix,
        /// Layer alpha (0.0–1.0).
        alpha: f32,
        /// Corner radius.
        corner_radius: f64,
    },
    /// Analytical erf Gaussian drop shadow for cards and shapes.
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
    let translate = kurbo::Affine::translate((x, y));
    paths
        .iter()
        .map(|tp| {
            let mut p = tp.clone();
            p.path.apply_affine(translate);
            p
        })
        .collect::<Vec<_>>()
        .into()
}

/// Compute the transform that maps a texture's pixel rect onto its display box.
///
/// `vello::Scene::draw_image` fills a rect of `image_width × image_height`
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
    /// Execute this command into a Vello scene with the given transform and opacity.
    pub fn execute(&self, scene: &mut vello::Scene, transform: &kurbo::Affine, opacity: f32) {
        match self {
            RenderCommand::Paths { paths } => {
                use kurbo::Shape as _;
                for path in paths {
                    match (&path.fill, &path.fill_gradient) {
                        (Some(fc), Some(grad)) => {
                            let g = grad
                                .to_peniko(path.path.bounding_box(), fc.components[3] * opacity);
                            scene.fill(
                                vello::peniko::Fill::NonZero,
                                *transform,
                                &g,
                                None,
                                path.path.as_ref(),
                            );
                        },
                        (Some(fc), None) => {
                            let fc = fc.with_alpha(fc.components[3] * opacity);
                            scene.fill(
                                vello::peniko::Fill::NonZero,
                                *transform,
                                fc,
                                None,
                                path.path.as_ref(),
                            );
                        },
                        // A ramp authored on a stroke-only shape paints no fill.
                        (None, Some(_)) => {},
                        (None, None) => {},
                    }
                    if let Some((mut sc, sw)) = path.stroke {
                        sc = sc.with_alpha(sc.components[3] * opacity);
                        let cap = match path.line_cap {
                            1 => vello::kurbo::Cap::Round,
                            2 => vello::kurbo::Cap::Square,
                            _ => vello::kurbo::Cap::Butt,
                        };
                        let join = match path.line_join {
                            1 => vello::kurbo::Join::Round,
                            2 => vello::kurbo::Join::Bevel,
                            _ => vello::kurbo::Join::Miter,
                        };
                        let dash: vello::kurbo::Dashes = path
                            .dash_pattern
                            .as_ref()
                            .map(|p| p.iter().map(|f| f64::from(*f)).collect())
                            .unwrap_or_default();
                        let stroke = vello::kurbo::Stroke {
                            width: sw as f64,
                            join,
                            miter_limit: 10.0,
                            start_cap: cap,
                            end_cap: cap,
                            dash_pattern: dash,
                            dash_offset: f64::from(path.dash_offset),
                        };
                        match &path.stroke_gradient {
                            Some(grad) => {
                                let g = grad.to_peniko(
                                    path.path.bounding_box(),
                                    sc.components[3] * opacity,
                                );
                                scene.stroke(&stroke, *transform, &g, None, path.path.as_ref());
                            },
                            None => scene.stroke(&stroke, *transform, sc, None, path.path.as_ref()),
                        }
                    }
                }
            },
            RenderCommand::Text {
                paths,
                fill_gradient,
                clip_path,
                tint_color,
                hardware_layout,
            } => {
                if let Some(clip) = clip_path {
                    scene.push_layer(
                        vello::peniko::Fill::NonZero,
                        vello::peniko::BlendMode::default(),
                        1.0,
                        *transform,
                        clip,
                    );
                }

                if fill_gradient.is_none() && opacity > 0.0 {
                    if let Some(hw) = hardware_layout {
                        let [r, g, b, a] = if let Some(tc) = tint_color {
                            *tc
                        } else if let Some(first) = paths.first() {
                            first.color
                        } else {
                            [255, 255, 255, 255]
                        };
                        let brush = vello::peniko::Brush::Solid(vello::peniko::Color::from_rgba8(
                            r, g, b, a,
                        ));

                        for run in &hw.runs {
                            let font_data = vello::peniko::FontData {
                                data: vello::peniko::Blob::new(std::sync::Arc::new(run.font_data)),
                                index: run.font_index,
                            };
                            let glyphs_iter = run.glyphs.iter().map(|g| vello::Glyph {
                                id: g.id,
                                x: g.x,
                                y: g.y,
                            });
                            let draw = scene.draw_glyphs(&font_data);
                            draw.font_size(run.font_size)
                                .transform(*transform)
                                .brush(&brush)
                                .brush_alpha(opacity)
                                .draw(vello::peniko::Fill::NonZero, glyphs_iter);
                        }

                        if clip_path.is_some() {
                            scene.pop_layer();
                        }
                        return;
                    }
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
                    if let Some(grad) = fill_gradient {
                        let g = grad.to_peniko(bounds, glyph_alpha);
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            &g,
                            None,
                            &text_path.path,
                        );
                    } else {
                        let [r, g, b, a] = if let Some(tc) = tint_color {
                            *tc
                        } else {
                            text_path.color
                        };
                        let color = vello::peniko::Color::from_rgba8(
                            r,
                            g,
                            b,
                            (a as f32 * glyph_alpha) as u8,
                        );
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            color,
                            None,
                            &text_path.path,
                        );
                    }
                }

                if clip_path.is_some() {
                    scene.pop_layer();
                }
            },
            RenderCommand::Image {
                image,
                natural_size,
                offset,
            } => {
                let image_transform = *transform
                    * image_display_transform(*natural_size, image.natural_size, *offset);
                let brush = vello::peniko::ImageBrush::new(image.data.clone())
                    .with_extend(vello::peniko::Extend::Pad)
                    .with_quality(vello::peniko::ImageQuality::Medium)
                    .with_alpha(opacity);
                scene.draw_image(&brush, image_transform);
            },
            RenderCommand::HighlightLayer {
                rect,
                color,
                blend,
                alpha,
                corner_radius,
            } => {
                let rounded = kurbo::RoundedRect::from_rect(*rect, *corner_radius);
                scene.push_layer(
                    vello::peniko::Fill::NonZero,
                    vello::peniko::BlendMode::new(*blend, vello::peniko::Compose::SrcOver),
                    *alpha * opacity,
                    *transform,
                    &rounded,
                );
                scene.fill(
                    vello::peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    *color,
                    None,
                    &rounded,
                );
                scene.pop_layer();
            },
            RenderCommand::Shadow {
                size,
                corner_radius,
                params,
                color,
            } => {
                let [dx, dy, blur, spread] = *params;
                let [r, g, b, a] = *color;
                let final_alpha = a * opacity;
                if final_alpha <= 0.001 {
                    return;
                }
                let half_w = size[0] + spread;
                let half_h = size[1] + spread;
                if blur <= 0.5 {
                    let shadow_rect = kurbo::Rect::new(
                        (dx - half_w) as f64,
                        (dy - half_h) as f64,
                        (dx + half_w) as f64,
                        (dy + half_h) as f64,
                    );
                    let peniko_color = vello::peniko::Color::from_rgba8(
                        (r * 255.0) as u8,
                        (g * 255.0) as u8,
                        (b * 255.0) as u8,
                        (final_alpha * 255.0) as u8,
                    );
                    if *corner_radius > 0.0 {
                        let rounded =
                            kurbo::RoundedRect::from_rect(shadow_rect, *corner_radius as f64);
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            peniko_color,
                            None,
                            &rounded,
                        );
                    } else {
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            peniko_color,
                            None,
                            &shadow_rect,
                        );
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
                        let peniko_color = vello::peniko::Color::from_rgba8(
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8,
                            (slice_alpha * 255.0).clamp(0.0, 255.0) as u8,
                        );
                        let cr = (*corner_radius + pad) as f64;
                        let rounded = kurbo::RoundedRect::from_rect(shadow_rect, cr);
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            peniko_color,
                            None,
                            &rounded,
                        );
                    }
                }
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
            RenderCommand::Text { paths, .. } => {
                for text_path in paths.iter() {
                    bounds = union(bounds, text_path.bounds);
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
            RenderCommand::HighlightLayer { rect, .. } => {
                bounds = union(bounds, *rect);
            },
            RenderCommand::Shadow { size, params, .. } => {
                let (half_w, half_h) =
                    (size[0] as f64 + params[3] as f64, size[1] as f64 + params[3] as f64);
                let blur_pad = params[2] as f64 * 3.0;
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
        }
        bounds
    }
}
