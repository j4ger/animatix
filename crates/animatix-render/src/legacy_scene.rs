//! Adapter from the engine-owned display list to Vello's classic scene.
//!
//! This is a temporary bridge while the classic renderer is still the default.
//! New renderer code should consume `animatix::scene::Draw` directly.

use animatix::scene::{Draw, Scene};
use vello::peniko::Fill;

/// Convert the neutral scene vocabulary into a classic Vello scene.
///
/// Scene-level effects and symbolic external textures are deliberately rejected:
/// those are orchestrated by the render layer and cannot be represented by a
/// single classic Vello scene.
pub(crate) fn to_vello_scene(source: &Scene) -> Result<vello::Scene, String> {
    let mut scene = vello::Scene::new();
    let mut stack_depth = 0usize;

    for draw in source.draws() {
        match draw {
            Draw::Fill {
                path,
                rule,
                transform,
                brush,
                brush_transform,
            } => {
                scene.fill(*rule, *transform, brush, *brush_transform, path);
            },
            Draw::Stroke {
                path,
                style,
                transform,
                brush,
                brush_transform,
            } => {
                scene.stroke(style, *transform, brush, *brush_transform, path);
            },
            Draw::Push {
                clip,
                fill,
                transform,
                blend,
                alpha,
            } => {
                if let Some(blend) = blend {
                    scene.push_layer(*fill, *blend, *alpha, *transform, clip);
                } else {
                    scene.push_clip_layer(*fill, *transform, clip);
                }
                stack_depth += 1;
            },
            Draw::Pop => {
                if stack_depth == 0 {
                    return Err("neutral scene contains an unmatched pop".into());
                }
                scene.pop_layer();
                stack_depth -= 1;
            },
            Draw::BlurredRect {
                clip,
                transform,
                rect,
                color,
                corner,
                std_dev,
            } => {
                scene.draw_blurred_rounded_rect_in(
                    clip,
                    *transform,
                    *rect,
                    *color,
                    f64::from(*corner),
                    f64::from(*std_dev),
                );
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
                let glyphs = glyphs.iter().map(|glyph| vello::Glyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                });
                scene
                    .draw_glyphs(font)
                    .font_size(*size)
                    .normalized_coords(coords)
                    .hint(*hint)
                    .transform(*transform)
                    .glyph_transform(*glyph_transform)
                    .brush(*color)
                    .draw(Fill::NonZero, glyphs);
            },
            Draw::Texture { .. } => {
                return Err(
                    "symbolic external textures must be rendered by the Vello GPU path".into()
                );
            },
            Draw::Filter { .. } | Draw::Glass { .. } => {
                return Err("scene effects must be rendered by the effect compositor".into());
            },
        }
    }

    if stack_depth != 0 {
        return Err("neutral scene contains an unclosed layer".into());
    }
    Ok(scene)
}
