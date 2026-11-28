//! Sample the active births and paint their shared artwork in birth order.
use super::{Motion, Style, schedule::unit, template::Template};
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::{ActorPoint, VelloPath};
use kurbo::{Circle, Shape};

fn sample([lo, hi]: [f64; 2], unit: f64) -> f64 {
    lo * (1. - unit) + hi * unit
}

pub(crate) fn evaluate(
    ctx: &EvaluateCtx,
    _: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let Some(e) = ctx.world.and_then(|w| w.emitters.get(ctx.id)) else {
        return Ok(None);
    };
    let Some(resolver) = ctx.target_resolver else {
        return Ok(None);
    };
    let Some(to_local) = resolver.world_to_particle_space(ctx.id) else {
        return Ok(None);
    };
    let anchor = e.origin.unwrap_or((ctx.id, ActorPoint::Origin));
    let style =
        crate::primitives::sample_shape_style(ctx.style, ctx.caps, ctx.time_ms, ctx.overrides);
    let [r, g, b, a] = style.color;
    let color = |alpha: f32| {
        peniko::Color::from_rgba8(
            (r * 255.) as u8,
            (g * 255.) as u8,
            (b * 255.) as u8,
            (a * alpha * 255.) as u8,
        )
    };
    let bubble = e.style == Style::Bubble;
    let mut batches = e
        .schedule
        .batches(ctx.time_s, e.life * e.clock_scale, e.seed, e.jitter * e.clock_scale)
        .map_err(|msg| RenderError::FrameRender(msg.into()))?;
    batches.sort_unstable_by(|a, b| {
        a.time
            .total_cmp(&b.time)
            .then(a.track.cmp(&b.track))
            .then(a.group.cmp(&b.group))
    });
    let mut paths = Vec::new();
    let mut clip_frame = match &e.template {
        Some(Template::Animated(clip)) => Some(clip.frame().map_err(RenderError::FrameRender)?),
        _ => None,
    };
    let mut animated = Vec::new();
    let mut transforms = Vec::new();
    let motion = e.motion.unwrap_or(match (&e.template, e.style) {
        (Some(_), _) => Motion::Linear,
        (_, Style::Bubble) => Motion::Bubble,
        (_, Style::Spark) => Motion::Ballistic,
        (_, Style::Dust) => Motion::Dust,
    });
    let body_color = color(if bubble { 0.18 } else { 1.0 });
    let outline = bubble.then(|| (color(0.94), 1.15));
    let glint_color = color(0.95);
    let mut last_birth = None;
    let mut last_origin = None;
    for batch in batches {
        let age = (ctx.time_s - batch.time) / e.clock_scale;
        let ms = (batch.time * 1000.0).round() as u64;
        if last_birth != Some(ms) {
            last_origin = resolver.particle_birth_point(anchor.0, anchor.1, ms);
            last_birth = Some(ms);
        }
        let Some(base) = last_origin else { continue };
        for i in 0..batch.count {
            let random = |channel| unit(e.seed, batch.track, batch.group, i, channel);
            let life = e.life * (0.72 + 0.28 * random(1));
            if age >= life {
                continue;
            }
            let angle = (e.heading + (random(2) - 0.5) * e.spread).to_radians();
            let distance = e.speed * (0.65 + 0.65 * random(3)) * age;
            let mut x = base.x + angle.cos() * distance;
            let mut y = base.y + angle.sin() * distance;
            match motion {
                Motion::Linear => {},
                Motion::Bubble | Motion::Drift => {
                    let phase = random(4) * std::f64::consts::TAU;
                    let amplitude = if matches!(motion, Motion::Drift) {
                        e.speed * 0.18
                    } else {
                        e.radius
                    };
                    x += amplitude * ((age * 3.0 + phase).sin() - phase.sin());
                },
                Motion::Ballistic => y += 0.42 * e.speed * age * age,
                Motion::Dust => y -= 0.1 * e.speed * age * age,
            }
            let fade = if e.shrink {
                ((life - age) / (0.22 * life)).min(1.0)
            } else {
                1.
            };
            let scale = e.scale.map(|range| sample(range, random(5))).unwrap_or_else(|| {
                if e.template.is_some() {
                    1.
                } else {
                    0.58 + 0.75 * random(5)
                }
            }) * fade;
            let rotation =
                (sample(e.angle, random(6)) + age * sample(e.spin, random(7))).to_radians();
            if e.template.is_some() {
                let transform = to_local
                    * kurbo::Affine::translate((x, y))
                    * kurbo::Affine::rotate(rotation)
                    * kurbo::Affine::scale(scale);
                if let Some(frame) = &mut clip_frame {
                    if let Some(artwork) =
                        frame.sample(age / life).map_err(RenderError::FrameRender)?
                    {
                        if let Some(RenderCommand::VectorInstances {
                            artwork: previous,
                            transforms,
                        }) = animated.last_mut()
                            && std::sync::Arc::ptr_eq(previous, &artwork)
                        {
                            transforms.push(transform);
                        } else {
                            animated.push(RenderCommand::VectorInstances {
                                artwork,
                                transforms: vec![transform],
                            });
                        }
                    }
                } else {
                    transforms.push(transform);
                }
                continue;
            }
            let r = e.radius * scale;
            let circle = Circle::new((x, y), r).to_path(0.2);
            paths.push(VelloPath {
                path: (to_local * circle).into(),
                fill: Some(body_color),
                stroke: outline,
                ..Default::default()
            });
            if bubble {
                let offset =
                    kurbo::Affine::rotate(rotation) * kurbo::Point::new(-r * 0.32, -r * 0.34);
                let spot = Circle::new((x + offset.x, y + offset.y), r * 0.2).to_path(0.15);
                paths.push(VelloPath {
                    path: (to_local * spot).into(),
                    fill: Some(glint_color),
                    ..Default::default()
                });
            }
        }
    }
    if matches!(e.template, Some(Template::Animated(_))) {
        return Ok(Some(animated));
    }
    if let Some(Template::Static(template)) = &e.template {
        return Ok(Some(vec![RenderCommand::VectorInstances {
            artwork: template.clone(),
            transforms,
        }]));
    }
    Ok(Some(if paths.is_empty() {
        vec![]
    } else {
        vec![RenderCommand::Paths { paths }]
    }))
}
