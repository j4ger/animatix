use kurbo::Affine;
use vello::peniko::{BlendMode, Brush, Compose};
use vello::recorded_draws::Draw;

pub(super) fn scene(
    source: &vello::Scene,
    size: (u16, u16),
    raster: Affine,
    resources: &mut vello_gpu::Resources,
    bindings: &std::collections::HashMap<vello_gpu::TextureId, wgpu::TextureView>,
    load_image: &mut impl FnMut(&vello::peniko::ImageBrush) -> Result<vello_gpu::Image, String>,
) -> Result<vello_gpu::Scene, String> {
    let mut scene = vello_gpu::Scene::new(size.0, size.1);
    let mut clips = Vec::new();
    for draw in source.recorded_draws() {
        match draw {
            Draw::Fill {
                path,
                rule,
                transform,
                brush,
                brush_transform,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(*rule);
                set_brush(&mut scene, brush, *brush_transform, load_image)?;
                scene.fill_path(path);
            },
            Draw::Stroke {
                path,
                style,
                transform,
                brush,
                brush_transform,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_stroke(style.clone());
                set_brush(&mut scene, brush, *brush_transform, load_image)?;
                scene.stroke_path(path);
            },
            Draw::Push {
                clip,
                fill,
                transform,
                blend,
                alpha,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(*fill);
                match blend {
                    None => scene.push_clip_path(clip),
                    Some(mode) if *mode == BlendMode::default() && *alpha == 1.0 => {
                        scene.push_layer(Some(clip), None, None, None, None);
                    },
                    Some(mode) if mode.compose == Compose::SrcOver => {
                        scene.push_layer(Some(clip), Some(*mode), Some(*alpha), None, None);
                    },
                    Some(_) => {
                        return Err(
                            "WebGL2 experiment does not support this layer composition".into()
                        );
                    },
                }
            },
            Draw::Pop => {
                // The matching Push determines which stack API to use.
                if clips.pop().ok_or("unbalanced scene clip")? {
                    scene.pop_clip();
                } else {
                    scene.pop_layer();
                }
            },
            Draw::BlurredRect {
                clip,
                transform,
                rect,
                color,
                corner,
                std_dev,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(vello::peniko::Fill::NonZero);
                scene.reset_paint_transform();
                scene.set_paint(*color);
                scene.fill_blurred_rounded_rect_in(clip, rect, *corner, *std_dev, false);
            },
            Draw::Glyphs {
                font,
                size,
                transform,
                glyph_transform,
                coords,
                glyphs,
                hint,
                color,
            } => {
                scene.set_transform(raster * *transform);
                scene.reset_paint_transform();
                scene.set_paint(*color);
                let mut run = scene
                    .glyph_run(resources, font)
                    .font_size(*size)
                    .normalized_coords(coords)
                    .hint(*hint);
                if let Some(transform) = glyph_transform {
                    run = run.glyph_transform(*transform);
                }
                run.fill_glyphs(glyphs.iter().map(|g| glifo::Glyph {
                    id: g.id,
                    x: g.x,
                    y: g.y,
                }))
                .map_err(|e| e.to_string())?;
            },
            Draw::Texture {
                id,
                rect,
                opacity,
                transform,
            } => {
                scene.set_transform(raster * *transform);
                scene.set_fill_rule(vello::peniko::Fill::NonZero);
                // External texture coordinates are in source texels, while
                // the recorded destination is in the authored scene's space.
                let view = bindings
                    .get(&vello_gpu::TextureId(*id))
                    .ok_or("missing inline Filter texture")?;
                let width = f64::from(view.texture().width());
                let height = f64::from(view.texture().height());
                scene.set_paint_transform(
                    Affine::translate((rect.x0, rect.y0))
                        * Affine::scale_non_uniform(rect.width() / width, rect.height() / height),
                );
                let source = vello_gpu::ImageSource::external_texture(
                    vello_gpu::TextureId(*id),
                    vello_gpu::TextureRegion::Full,
                    true,
                );
                scene.set_paint(vello_gpu::Image {
                    image: source,
                    sampler: vello::peniko::ImageSampler::new().with_alpha(*opacity),
                });
                scene.fill_rect(rect);
            },
            Draw::Unsupported(kind) => {
                return Err(format!("WebGL2 experiment has no encoder for {kind}"));
            },
        }
        if let Draw::Push { blend, .. } = draw {
            clips.push(blend.is_none());
        }
    }
    if !clips.is_empty() {
        return Err("unbalanced scene clip".into());
    }
    Ok(scene)
}

fn set_brush(
    scene: &mut vello_gpu::Scene,
    brush: &Brush,
    transform: Option<Affine>,
    load_image: &mut impl FnMut(&vello::peniko::ImageBrush) -> Result<vello_gpu::Image, String>,
) -> Result<(), String> {
    scene.set_paint_transform(transform.unwrap_or(Affine::IDENTITY));
    match brush {
        Brush::Solid(color) => scene.set_paint(*color),
        Brush::Gradient(gradient) => scene.set_paint(gradient.clone()),
        Brush::Image(brush) => scene.set_paint(load_image(brush)?),
    }
    Ok(())
}
