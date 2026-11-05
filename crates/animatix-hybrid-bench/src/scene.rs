//! Synthetic, backend-agnostic draw list.
//!
//! Both renderers build their scene from the *same* [`DrawSpec`] list, so the
//! comparison measures the renderer, not the content. No images, gradients or
//! real glyphs: just filled/stroked paths at a density chosen to resemble the
//! diagram scenes the site ships.

use vello::kurbo::{BezPath, Circle, Rect, RoundedRect, Shape};
use vello::peniko::Color;

/// One path plus how to paint it. Geometry is already in scene coordinates.
pub struct DrawSpec {
    pub path: BezPath,
    pub fill: Option<Color>,
    pub stroke: Option<(Color, f32)>,
}

/// Build the scene for `kind` at time `t`. `light` ~ a few dozen shapes (a
/// typical diagram); `dense` ~ several hundred with heavy overdraw.
pub fn specs(kind: &str, t: f64, width: u32, height: u32) -> Vec<DrawSpec> {
    let (cols, rows) = match kind {
        "dense" => (24u32, 16u32),
        _ => (10u32, 7u32),
    };
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let cw = w / cols as f64;
    let ch = h / rows as f64;
    let mut out = Vec::with_capacity((cols * rows) as usize + 32);

    for j in 0..rows {
        for i in 0..cols {
            let idx = (j * cols + i) as f64;
            // A small time-dependent wobble so a frame is not literally cached
            // by either backend between iterations.
            let dx = (t * 1.3 + idx * 0.7).sin() * (cw * 0.05);
            let dy = (t * 1.7 + idx * 0.4).cos() * (ch * 0.06);
            let x = i as f64 * cw + cw * 0.04 + dx;
            let y = j as f64 * ch + ch * 0.04 + dy;
            let bw = (cw * 0.92).max(1.0);
            let bh = (ch * 0.92).max(1.0);
            let radius = bw.min(bh) * 0.18;
            let rect = RoundedRect::from_rect(Rect::new(x, y, x + bw, y + bh), radius);
            let r = (60 + (i * 13 + j * 7) % 180) as u8;
            let g = (70 + (i * 29 + j * 11) % 160) as u8;
            let b = (90 + (i * 47 + j * 17) % 150) as u8;
            let stroke = if (i + j) % 4 == 0 {
                Some((Color::from_rgba8(235, 231, 219, 255), 1.5))
            } else {
                None
            };
            out.push(DrawSpec {
                path: rect.to_path(0.08),
                fill: Some(Color::from_rgba8(r, g, b, 210)),
                stroke,
            });
        }
    }

    // A few circles and a polyline to exercise curves, not just rounded rects.
    for k in 0..8u32 {
        let cx = w * (0.1 + 0.1 * k as f64);
        let cy = h * 0.5 + (t + k as f64).sin() * h * 0.08;
        let circle = Circle::new((cx, cy), (h * 0.03).max(2.0));
        out.push(DrawSpec {
            path: circle.to_path(0.08),
            fill: Some(Color::from_rgba8(245, 185, 66, 235)),
            stroke: None,
        });
    }
    let mut poly = BezPath::new();
    for s in 0..64 {
        let x = w * 0.05 + (w * 0.9) * (s as f64 / 63.0);
        let y = h * 0.85 + (s as f64 * 0.35 + t * 2.0).sin() * h * 0.04;
        if s == 0 {
            poly.move_to((x, y));
        } else {
            poly.line_to((x, y));
        }
    }
    out.push(DrawSpec {
        path: poly,
        fill: None,
        stroke: Some((Color::from_rgba8(138, 180, 248, 255), 2.0)),
    });

    out
}

/// Encode the shared draw list into a classic `vello::Scene` (compute backend).
pub fn build_vello(kind: &str, t: f64, width: u32, height: u32) -> vello::Scene {
    use vello::peniko::Fill;
    let mut scene = vello::Scene::new();
    let aff = vello::kurbo::Affine::IDENTITY;
    for s in specs(kind, t, width, height) {
        if let Some(c) = s.fill {
            scene.fill(Fill::NonZero, aff, c, None, &s.path);
        }
        if let Some((c, sw)) = s.stroke {
            let stroke = vello::kurbo::Stroke::new(sw as f64);
            scene.stroke(&stroke, aff, c, None, &s.path);
        }
    }
    scene
}

/// Encode the shared draw list into a `vello_hybrid::Scene` (sparse backends:
/// WebGL2 and wgpu both consume the same scene type).
pub fn build_hybrid(kind: &str, t: f64, width: u32, height: u32) -> vello_hybrid::Scene {
    let mut scene = vello_hybrid::Scene::new(width.max(1) as u16, height.max(1) as u16);
    scene.set_transform(vello::kurbo::Affine::IDENTITY);
    scene.set_fill_rule(vello::peniko::Fill::NonZero);
    for s in specs(kind, t, width, height) {
        if let Some(c) = s.fill {
            scene.set_paint(c);
            scene.fill_path(&s.path);
        }
        if let Some((c, sw)) = s.stroke {
            scene.set_paint(c);
            scene.set_stroke(vello::kurbo::Stroke::new(sw as f64));
            scene.stroke_path(&s.path);
        }
    }
    scene
}
