//! Direct rounded surfaces for measured layout boxes and text.

use std::sync::OnceLock;

use animatix_core::caps::{ChildProcessingKind, TextKind};
use kurbo::{Rect, RoundedRect};
use peniko::Color;

use super::{EvaluateCtx, RenderCommand, styling};
use crate::timeline::{PropertyValue, Value, property_id};

pub(crate) fn prepend(
    ctx: &EvaluateCtx<'_>,
    half_size: [f32; 2],
    commands: &mut Vec<RenderCommand>,
) {
    let is_text = ctx.caps.text == Some(TextKind::Text);
    let is_layout =
        ctx.caps.layout_container && ctx.caps.child_processing == ChildProcessingKind::Generic;
    if !(is_text || is_layout) || !enabled(ctx) {
        return;
    }

    let half_size = if is_layout && !ctx.overrides.is_some_and(|values| values.contains_key("size"))
    {
        ctx.geometry.layout_size_get(ctx.time_ms).unwrap_or(half_size)
    } else {
        half_size
    };
    let bounds = if is_text {
        let Some(bounds) = commands
            .iter()
            .filter_map(|command| command.local_bounds(None))
            .reduce(|a, b| a.union(b))
        else {
            return;
        };
        let [x, y] = match ctx.overrides.and_then(|values| values.get("backdrop_padding")) {
            Some(Value::Vec2([x, y])) => [*x as f32, *y as f32],
            _ => ctx.property_plan.sample_vec2_prop("backdrop_padding", ctx.time_ms, [16.0, 8.0]),
        };
        bounds.inflate(f64::from(x), f64::from(y))
    } else {
        let [x, y] = half_size.map(f64::from);
        Rect::new(-x, -y, x, y)
    };
    if !(bounds.width() > 0.0 && bounds.height() > 0.0) {
        return;
    }

    let radius = number(ctx, "backdrop_radius", 8.0);
    let fill = paint_bytes(color(ctx, "backdrop_color", [0.0, 0.0, 0.0, 0.6]));
    let stroke = paint_bytes(color(ctx, "backdrop_stroke", [0.0; 4]));
    let width = number(ctx, "backdrop_stroke_width", 1.0);
    if fill[3] > 0 || (stroke[3] > 0 && width > 0.0) {
        commands.insert(
            0,
            RenderCommand::RoundedSurface {
                rect: bounds,
                radius,
                fill,
                stroke,
                stroke_width: width,
            },
        );
    }
    if is_layout && let Some((params, color)) = styling::sample_shape_shadow(ctx) {
        commands.insert(
            0,
            RenderCommand::Shadow {
                size: half_size,
                corner_radius: radius,
                params,
                color,
            },
        );
    }
}

fn enabled(ctx: &EvaluateCtx<'_>) -> bool {
    if let Some(Value::Bool(enabled)) = ctx.overrides.and_then(|values| values.get("backdrop")) {
        return *enabled;
    }
    static ID: OnceLock<animatix_core::property::PropertyId> = OnceLock::new();
    let id = *ID.get_or_init(|| property_id("backdrop").expect("built-in backdrop property"));
    matches!(
        ctx.property_plan.get(id).and_then(|slot| slot.track.sample(ctx.time_ms)),
        Some(PropertyValue::Bool(true))
    )
}

fn number(ctx: &EvaluateCtx<'_>, name: &str, default: f32) -> f32 {
    match ctx.overrides.and_then(|values| values.get(name)) {
        Some(Value::Num(value)) if (*value as f32).is_finite() => (*value as f32).max(0.0),
        _ => ctx.sample_f32_prop(name, ctx.time_ms, default).max(0.0),
    }
}

fn color(ctx: &EvaluateCtx<'_>, name: &str, default: [f32; 4]) -> [f32; 4] {
    ctx.overrides
        .and_then(|values| styling::override_color(values, name))
        .unwrap_or_else(|| ctx.sample_color_prop(name, ctx.time_ms, default))
}

pub(crate) fn rounded_rect(bounds: Rect, radius: f32) -> RoundedRect {
    RoundedRect::from_rect(bounds, f64::from(radius.max(0.0)))
}

pub(crate) fn paint_color([r, g, b, a]: [f32; 4], opacity: f32) -> Color {
    let [r, g, b, a] = paint_bytes([r, g, b, a * opacity]);
    Color::from_rgba8(r, g, b, a)
}

fn paint_bytes(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel * 255.0).clamp(0.0, 255.0) as u8)
}
