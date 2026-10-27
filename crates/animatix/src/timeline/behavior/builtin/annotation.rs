//! Annotation family evaluation and build systems (Callout, Connector, Legend).

use std::sync::Arc;

use kurbo::Affine;
use vello::peniko::Color;

use crate::primitives::build_arrow_path;
use crate::primitives::connector::{
    build_connector_path, resolve_connector_endpoint, route_connector,
};
use crate::primitives::{
    EvaluateCtx, RenderCommand, TextCompileCtx, evaluate_text_paths, sample_shape_style,
};
use crate::renderer::error::RenderError;
use crate::renderer::text::TextKind;
use crate::timeline::animation_track::{ConnectorEndpoint, ConnectorRouting};
use crate::timeline::callout_geometry::derive_callout_geometry;
use crate::timeline::{TrackAccessor, Value, VelloPath};

pub use crate::primitives::legend::{build_extra_legend, evaluate_legend};

// ── Callout evaluation ───────────────────────────────────────────────────────

/// Evaluate draw commands for a `Callout` actor.
pub fn evaluate_callout(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let geometry = ctx.geometry;
    let shape = ctx.shape.expect("callout has shape");
    let geom = derive_callout_geometry(
        geometry,
        shape,
        ctx.time_ms,
        ctx.target_resolver,
        ctx.scene_dimensions,
    );
    let mut from = geom.from;
    let mut to = geom.to;
    let mut head_size = shape.head_size.get(ctx.time_ms, 10.0);

    let target_name = geometry.callout_target.get(ctx.time_ms, String::new());
    if !target_name.is_empty() && ctx.target_resolver.is_none() {
        tracing::debug!(
            "callout '{}': target actor '{}' not found in timeline",
            ctx.label,
            target_name
        );
    }

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Vec2(f)) = overrides.get("from") {
            from = [f[0] as f32, f[1] as f32];
        }
        if let Some(Value::Vec2(t)) = overrides.get("to") {
            to = [t[0] as f32, t[1] as f32];
        }
        if let Some(Value::Num(n)) = overrides.get("head_size") {
            head_size = *n as f32;
        }
    }
    let path = build_arrow_path(from, to, head_size);

    let style = sample_shape_style(ctx.style, ctx.caps, ctx.time_ms, ctx.overrides);

    let arrow_vello = VelloPath {
        path: Arc::new(path),
        fill: Some(Color::from_rgba8(
            (style.stroke_color[0] * 255.0) as u8,
            (style.stroke_color[1] * 255.0) as u8,
            (style.stroke_color[2] * 255.0) as u8,
            (style.stroke_color[3] * 255.0) as u8,
        )),
        stroke: crate::timeline::shapes::shape_stroke(style.stroke_color, style.stroke_width)
            .or_else(|| {
                Some((
                    Color::from_rgba8(
                        (style.stroke_color[0] * 255.0) as u8,
                        (style.stroke_color[1] * 255.0) as u8,
                        (style.stroke_color[2] * 255.0) as u8,
                        (style.stroke_color[3] * 255.0) as u8,
                    ),
                    1.0,
                ))
            }),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    };

    let mut commands = vec![RenderCommand::Paths {
        paths: vec![arrow_vello],
    }];

    let mut label_text = ctx
        .text
        .map(|t| t.text_content.get(ctx.time_ms, String::new()))
        .unwrap_or_default();
    let mut label_at = geometry.label_at.get(ctx.time_ms, [0.0, 50.0]);

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Str(s)) = overrides.get("label") {
            label_text = s.clone();
        }
        if let Some(Value::Vec2(la)) = overrides.get("label_at") {
            label_at = [la[0] as f32, la[1] as f32];
        }
    }

    if !label_text.is_empty() {
        let offset_x = (to[0] + label_at[0]) as f64;
        let offset_y = (to[1] + label_at[1]) as f64;

        if let Some(text_ctx) = text_ctx {
            let (paths, _) = evaluate_text_paths(ctx, text_ctx, TextKind::Text, 24.0)?;

            let translate = Affine::translate((offset_x, offset_y));
            let translated_paths: Vec<crate::renderer::types::TextPath> = paths
                .iter()
                .map(|tp| {
                    let mut p = tp.clone();
                    p.path.apply_affine(translate);
                    p
                })
                .collect();

            if !translated_paths.is_empty() {
                commands.push(RenderCommand::Text {
                    paths: Arc::from(translated_paths.into_boxed_slice()),
                    fill_gradient: None,
                    clip_path: None,
                    tint_color: None,
                    hardware_layout: None,
                });
            }
        }
    }

    Ok(Some(commands))
}

// ── Connector evaluation ─────────────────────────────────────────────────────

/// Evaluate draw commands for a `Connector` actor.
pub fn evaluate_connector(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let geometry = ctx.geometry;
    let ep_from = geometry.connector_from.get(ctx.time_ms, ConnectorEndpoint::Point([0.0, 0.0]));
    let ep_to = geometry.connector_to.get(ctx.time_ms, ConnectorEndpoint::Point([100.0, 100.0]));
    let routing = geometry.connector_routing.get(ctx.time_ms, ConnectorRouting::Elbow);
    let mut arrow = geometry.connector_arrow.get(ctx.time_ms, true);
    let shape = ctx.shape.expect("connector has shape");
    let mut corner_radius = shape.corner_radius.get(ctx.time_ms, 8.0);
    let mut head_size = shape.head_size.get(ctx.time_ms, 10.0);

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Bool(b)) = overrides.get("arrow") {
            arrow = *b;
        }
        if let Some(Value::Num(n)) = overrides.get("corner_radius") {
            corner_radius = *n as f32;
        }
        if let Some(Value::Num(n)) = overrides.get("head_size") {
            head_size = *n as f32;
        }
    }

    let (pos_from_init, _) = resolve_connector_endpoint(
        &ep_from,
        ctx.target_resolver,
        ctx.time_ms,
        ctx.scene_dimensions,
        None,
    );
    let (pos_to, norm_to) = resolve_connector_endpoint(
        &ep_to,
        ctx.target_resolver,
        ctx.time_ms,
        ctx.scene_dimensions,
        Some(pos_from_init),
    );
    let (pos_from, norm_from) = resolve_connector_endpoint(
        &ep_from,
        ctx.target_resolver,
        ctx.time_ms,
        ctx.scene_dimensions,
        Some(pos_to),
    );

    let waypoints = route_connector(pos_from, norm_from, pos_to, norm_to, routing, 20.0);
    let (shaft_path, arrowhead_path) =
        build_connector_path(&waypoints, corner_radius, arrow, head_size);

    let style = sample_shape_style(ctx.style, ctx.caps, ctx.time_ms, ctx.overrides);
    let mut paths = Vec::new();

    let shaft_vello = VelloPath {
        path: Arc::new(shaft_path),
        fill: None,
        stroke: crate::timeline::shapes::shape_stroke(style.stroke_color, style.stroke_width)
            .or_else(|| {
                Some((
                    Color::from_rgba8(
                        (style.stroke_color[0] * 255.0) as u8,
                        (style.stroke_color[1] * 255.0) as u8,
                        (style.stroke_color[2] * 255.0) as u8,
                        (style.stroke_color[3] * 255.0) as u8,
                    ),
                    1.5,
                ))
            }),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    };
    paths.push(shaft_vello);

    if let Some(arrow_p) = arrowhead_path {
        let arrow_vello = VelloPath {
            path: Arc::new(arrow_p),
            fill: Some(Color::from_rgba8(
                (style.stroke_color[0] * 255.0) as u8,
                (style.stroke_color[1] * 255.0) as u8,
                (style.stroke_color[2] * 255.0) as u8,
                (style.stroke_color[3] * 255.0) as u8,
            )),
            stroke: None,
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        };
        paths.push(arrow_vello);
    }

    Ok(Some(vec![RenderCommand::Paths { paths }]))
}
