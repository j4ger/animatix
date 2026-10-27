//! Shape family evaluation and render systems (Rect, Ellipse, Line, Arrow, Polygon, Path).

use crate::primitives::{
    EvaluateCtx, RenderCommand, RenderCtx, TextCompileCtx, build_arrow_path, evaluate_shape_render,
    resolve_half_size,
};
use crate::renderer::error::RenderError;
use crate::timeline::callout_geometry::bounds_anchor_point;
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::shapes::{
    ArrowState, EllipseState, LineState, PathState, PolygonState, RectState,
};
use crate::timeline::{
    DEFAULT_LAYOUT_HALF_SIZE, TrackAccessor, Value, VectorShapeState, VelloPath,
};

// ── Render functions ─────────────────────────────────────────────────────────

/// Render a `Rect` shape into Vello paths.
pub fn render_rect(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Rect(state) = ctx.state else {
        return None;
    };
    let (x0, y0) = (-(state.size[0] as f64), -(state.size[1] as f64));
    let (x1, y1) = (state.size[0] as f64, state.size[1] as f64);
    let shape = if state.corner_radius > 0.0 {
        KurboShape::rounded_rect(x0, y0, x1, y1, state.corner_radius as f64)
    } else {
        KurboShape::Rect { x0, y0, x1, y1 }
    };
    let path = ctx.shape.expect("shape tier").shape_path_memoized(&shape);
    Some(vec![crate::timeline::shapes::build_vello_path(
        path,
        ctx.style.color,
        ctx.style.stroke_color,
        ctx.style.stroke_width,
        ctx.style.fill_opacity,
        false,
    )])
}

/// Render an `Ellipse` shape into Vello paths.
pub fn render_ellipse(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Ellipse(state) = ctx.state else {
        return None;
    };
    let shape = KurboShape::Ellipse {
        center: kurbo::Point::new(0.0, 0.0),
        radii: kurbo::Vec2::new(state.size[0] as f64, state.size[1] as f64),
        rotation: state.rotation as f64,
    };
    let path = ctx.shape.expect("shape tier").shape_path_memoized(&shape);
    Some(vec![crate::timeline::shapes::build_vello_path(
        path,
        ctx.style.color,
        ctx.style.stroke_color,
        ctx.style.stroke_width,
        ctx.style.fill_opacity,
        false,
    )])
}

/// Render a `Line` shape into Vello paths.
pub fn render_line(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Line(state) = ctx.state else {
        return None;
    };
    let shape = KurboShape::Line {
        p0: kurbo::Point::new(state.line_from[0] as f64, state.line_from[1] as f64),
        p1: kurbo::Point::new(state.line_to[0] as f64, state.line_to[1] as f64),
    };
    let path = ctx.shape.expect("shape tier").shape_path_memoized(&shape);
    Some(vec![crate::timeline::shapes::build_vello_path(
        path,
        ctx.style.color,
        ctx.style.stroke_color,
        ctx.style.stroke_width,
        0.0,
        true,
    )])
}

/// Render an `Arrow` shape into Vello paths.
pub fn render_arrow(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Arrow(state) = ctx.state else {
        return None;
    };
    let path = build_arrow_path(state.from, state.to, state.head_size);
    Some(vec![VelloPath {
        path: std::sync::Arc::new(path),
        fill: Some(vello::peniko::Color::from_rgba8(
            (ctx.style.stroke_color[0] * 255.0) as u8,
            (ctx.style.stroke_color[1] * 255.0) as u8,
            (ctx.style.stroke_color[2] * 255.0) as u8,
            (ctx.style.stroke_color[3] * 255.0) as u8,
        )),
        stroke: crate::timeline::shapes::shape_stroke(
            ctx.style.stroke_color,
            ctx.style.stroke_width,
        )
        .or_else(|| {
            Some((
                vello::peniko::Color::from_rgba8(
                    (ctx.style.stroke_color[0] * 255.0) as u8,
                    (ctx.style.stroke_color[1] * 255.0) as u8,
                    (ctx.style.stroke_color[2] * 255.0) as u8,
                    (ctx.style.stroke_color[3] * 255.0) as u8,
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
    }])
}

/// Render a `Polygon` shape into Vello paths.
pub fn render_polygon(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Polygon(state) = ctx.state else {
        return None;
    };
    let shape = if !state.points.is_empty() {
        let points = state
            .points
            .iter()
            .map(|p| kurbo::Point::new(p[0] as f64, p[1] as f64))
            .collect();
        KurboShape::Polygon { points }
    } else if let Some(custom_path) = &state.custom_path {
        KurboShape::Path {
            path: custom_path.clone(),
        }
    } else {
        KurboShape::Polygon { points: Vec::new() }
    };
    let path = ctx.shape.expect("shape tier").shape_path_memoized(&shape);
    Some(vec![crate::timeline::shapes::build_vello_path(
        path,
        ctx.style.color,
        ctx.style.stroke_color,
        ctx.style.stroke_width,
        ctx.style.fill_opacity,
        false,
    )])
}

/// Render a `Path` shape into Vello paths.
pub fn render_path(ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
    let VectorShapeState::Path(state) = ctx.state else {
        return None;
    };
    let path = std::sync::Arc::new(state.custom_path.clone().unwrap_or_else(kurbo::BezPath::new));
    Some(vec![crate::timeline::shapes::build_vello_path(
        path,
        ctx.style.color,
        ctx.style.stroke_color,
        ctx.style.stroke_width,
        ctx.style.fill_opacity,
        false,
    )])
}

// ── Evaluate functions ───────────────────────────────────────────────────────

/// Evaluate draw commands for a `Rect` actor.
pub fn evaluate_rect(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let half_size = resolve_half_size(
        ctx.geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE),
        ctx.overrides,
    );
    let mut state = RectState {
        size: half_size,
        corner_radius: ctx.shape.map(|s| s.corner_radius.get(ctx.time_ms, 0.0)).unwrap_or(0.0),
    };

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Num(radius)) = overrides.get("corner_radius") {
            state.corner_radius = *radius as f32;
        }
    }

    evaluate_shape_render(render_rect, "Rect", ctx, &VectorShapeState::Rect(state))
}

/// Evaluate draw commands for an `Ellipse` actor.
pub fn evaluate_ellipse(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let geometry = ctx.geometry;
    let half_size =
        resolve_half_size(geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE), ctx.overrides);
    let arc_angles = ctx
        .shape
        .expect("ellipse primitive requires shape component")
        .arc_angles
        .get(ctx.time_ms, [0.0, 0.0]);
    let rot = geometry.rotation.get(ctx.time_ms, 0.0);

    let mut state = EllipseState {
        size: half_size,
        arc_angles,
        rotation: if rot != 0.0 { rot } else { 0.0 },
    };

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Num(r)) = overrides.get("rotation") {
            state.rotation = *r as f32;
        }
    }

    evaluate_shape_render(render_ellipse, "Ellipse", ctx, &VectorShapeState::Ellipse(state))
}

/// Evaluate draw commands for a `Line` actor.
pub fn evaluate_line(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let shape = ctx.shape.expect("line primitive requires shape component");
    let mut line_from = shape.line_from.get(ctx.time_ms, [-50.0, 0.0]);
    let mut line_to = shape.line_to.get(ctx.time_ms, [50.0, 0.0]);

    if let Some((actor, anchor)) = shape.from_anchor.as_ref() {
        if let Some(resolver) = ctx.target_resolver {
            if let Some((centre, half)) =
                resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
            {
                line_from = bounds_anchor_point(*anchor, centre, half);
            }
        }
    }
    if let Some((actor, anchor)) = shape.to_anchor.as_ref() {
        if let Some(resolver) = ctx.target_resolver {
            if let Some((centre, half)) =
                resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
            {
                line_to = bounds_anchor_point(*anchor, centre, half);
            }
        }
    }

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Vec2(from)) = overrides.get("from") {
            line_from = [from[0] as f32, from[1] as f32];
        }
        if let Some(Value::Vec2(to)) = overrides.get("to") {
            line_to = [to[0] as f32, to[1] as f32];
        }
    }

    let state = LineState {
        line_from,
        line_to,
        size: [0.0, 0.0],
    };

    evaluate_shape_render(render_line, "Line", ctx, &VectorShapeState::Line(state))
}

/// Evaluate draw commands for an `Arrow` actor.
pub fn evaluate_arrow(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let shape = ctx.shape.expect("arrow primitive requires shape component");
    let mut line_from = shape.line_from.get(ctx.time_ms, [-50.0, 0.0]);
    let mut line_to = shape.line_to.get(ctx.time_ms, [50.0, 0.0]);
    let head_size = shape.head_size.get(ctx.time_ms, 10.0);

    if let Some((actor, anchor)) = shape.from_anchor.as_ref() {
        if let Some(resolver) = ctx.target_resolver {
            if let Some((centre, half)) =
                resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
            {
                line_from = bounds_anchor_point(*anchor, centre, half);
            }
        }
    }
    if let Some((actor, anchor)) = shape.to_anchor.as_ref() {
        if let Some(resolver) = ctx.target_resolver {
            if let Some((centre, half)) =
                resolver.target_bounds(actor, ctx.time_ms, ctx.scene_dimensions)
            {
                line_to = bounds_anchor_point(*anchor, centre, half);
            }
        }
    }

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Vec2(from)) = overrides.get("from") {
            line_from = [from[0] as f32, from[1] as f32];
        }
        if let Some(Value::Vec2(to)) = overrides.get("to") {
            line_to = [to[0] as f32, to[1] as f32];
        }
    }

    let state = ArrowState {
        from: line_from,
        to: line_to,
        head_size,
    };

    evaluate_shape_render(render_arrow, "Arrow", ctx, &VectorShapeState::Arrow(state))
}

/// Evaluate draw commands for a `Polygon` actor.
pub fn evaluate_polygon(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let geometry = ctx.geometry;
    let half_size =
        resolve_half_size(geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE), ctx.overrides);
    let shape = ctx.shape.expect("polygon primitive requires shape component");
    let points = shape.points.get(ctx.time_ms, Vec::new());
    let rot = geometry.rotation.get(ctx.time_ms, 0.0);
    let vector_paths = shape.evaluate_vector_paths(ctx.style, ctx.time_ms);

    let mut state = PolygonState {
        size: half_size,
        regular_polygon_sides: 0,
        regular_polygon_radius: half_size[0],
        custom_path: vector_paths.first().map(|vp| vp.path.as_ref().clone()),
        rotation: if rot != 0.0 { rot } else { 0.0 },
        points,
    };

    if let Some(overrides) = ctx.overrides {
        if let Some(Value::Num(r)) = overrides.get("rotation") {
            state.rotation = *r as f32;
        }
    }

    evaluate_shape_render(render_polygon, "Polygon", ctx, &VectorShapeState::Polygon(state))
}

/// Evaluate draw commands for a `Path` actor.
pub fn evaluate_path(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let geometry = ctx.geometry;
    let half_size = geometry.size.get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE);
    let vector_paths = ctx
        .shape
        .expect("path primitive requires shape component")
        .evaluate_vector_paths(ctx.style, ctx.time_ms);

    let mut state = PathState {
        size: half_size,
        custom_path: vector_paths.first().map(|vp| vp.path.as_ref().clone()),
    };

    state.size = resolve_half_size(state.size, ctx.overrides);

    evaluate_shape_render(render_path, "Path", ctx, &VectorShapeState::Path(state))
}
