//! Smart orthogonal connector primitive.

use std::sync::Arc;
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{
    AssignmentCtx, BuildCtx, EvaluateCtx, Primitive, RenderCommand, sample_shape_style,
};
use crate::renderer::error::RenderError;
use crate::timeline::callout_geometry::TargetResolver;
use crate::timeline::{
    ActorCaps, AnimationTrack, ConnectorEndpoint, ConnectorRouting, Environment,
    SceneAnchor, SceneDimensions, TrackAccessor, Value, VelloPath,
};

/// Helper to parse a `ConnectorEndpoint` from an expression in the build environment.
pub fn parse_connector_endpoint(expr: &Expr, env: &Environment) -> Option<ConnectorEndpoint> {
    match expr {
        Expr::Tuple(items) if items.len() == 2 => {
            // Check if (x, y) numeric coordinate tuple
            let eval_x = crate::timeline::evaluate_expr(&items[0], env).ok();
            let eval_y = crate::timeline::evaluate_expr(&items[1], env).ok();
            if let (Some(Value::Num(x)), Some(Value::Num(y))) = (eval_x, eval_y) {
                return Some(ConnectorEndpoint::Point([x as f32, y as f32]));
            }

            // Otherwise check (node, anchor)
            let node_name = match &items[0] {
                Expr::Ident(s) | Expr::Str(s) => s.clone(),
                Expr::Path(parts) => parts.join("."),
                _ => return None,
            };
            let anchor_name = match &items[1] {
                Expr::Ident(s) | Expr::Str(s) => s.clone(),
                _ => return None,
            };
            let anchor = SceneAnchor::from_str(&anchor_name.replace('-', "_"));
            Some(ConnectorEndpoint::Target {
                actor: node_name,
                anchor,
            })
        },
        Expr::Path(parts) if parts.len() == 2 => {
            let anchor = SceneAnchor::from_str(&parts[1].replace('-', "_"));
            if anchor.is_some() {
                Some(ConnectorEndpoint::Target {
                    actor: parts[0].clone(),
                    anchor,
                })
            } else {
                Some(ConnectorEndpoint::Target {
                    actor: parts.join("."),
                    anchor: None,
                })
            }
        },
        Expr::Ident(s) | Expr::Str(s) => {
            Some(ConnectorEndpoint::Target {
                actor: s.clone(),
                anchor: None,
            })
        },
        _ => None,
    }
}

/// Resolve a connector endpoint to world-space coordinates and normal direction.
pub fn resolve_connector_endpoint(
    endpoint: &ConnectorEndpoint,
    target_resolver: Option<&dyn TargetResolver>,
    time_ms: u64,
    scene_dimensions: SceneDimensions,
    other_pos: Option<[f32; 2]>,
) -> ([f32; 2], [f32; 2]) {
    match endpoint {
        ConnectorEndpoint::Point(pt) => {
            let norm = match other_pos {
                Some(other) => {
                    let dx = other[0] - pt[0];
                    let dy = other[1] - pt[1];
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 1e-4 { [dx / len, dy / len] } else { [1.0, 0.0] }
                },
                None => [1.0, 0.0],
            };
            (*pt, norm)
        },
        ConnectorEndpoint::Target { actor, anchor } => {
            if let Some(resolver) = target_resolver {
                if let Some((center, half_size)) = resolver.target_bounds(actor, time_ms, scene_dimensions) {
                    let resolved_anchor = anchor.unwrap_or_else(|| {
                        // Auto-choose the most direct anchor side facing the other position
                        match other_pos {
                            Some(other) => {
                                let dx = other[0] - center[0];
                                let dy = other[1] - center[1];
                                if dx.abs() >= dy.abs() {
                                    if dx >= 0.0 { SceneAnchor::Right } else { SceneAnchor::Left }
                                } else {
                                    if dy >= 0.0 { SceneAnchor::Bottom } else { SceneAnchor::Top }
                                }
                            },
                            None => SceneAnchor::Right,
                        }
                    });

                    const FRAC: f32 = std::f32::consts::FRAC_1_SQRT_2;
                    return match resolved_anchor {
                        SceneAnchor::Left => ([center[0] - half_size[0], center[1]], [-1.0, 0.0]),
                        SceneAnchor::Right => ([center[0] + half_size[0], center[1]], [1.0, 0.0]),
                        SceneAnchor::Top => ([center[0], center[1] - half_size[1]], [0.0, -1.0]),
                        SceneAnchor::Bottom => ([center[0], center[1] + half_size[1]], [0.0, 1.0]),
                        SceneAnchor::Center => (center, [0.0, 0.0]),
                        SceneAnchor::TopLeft => ([center[0] - half_size[0], center[1] - half_size[1]], [-FRAC, -FRAC]),
                        SceneAnchor::TopRight => ([center[0] + half_size[0], center[1] - half_size[1]], [FRAC, -FRAC]),
                        SceneAnchor::BottomLeft => ([center[0] - half_size[0], center[1] + half_size[1]], [-FRAC, FRAC]),
                        SceneAnchor::BottomRight => ([center[0] + half_size[0], center[1] + half_size[1]], [FRAC, FRAC]),
                    };
                }
            }
            ([0.0, 0.0], [1.0, 0.0])
        },
    }
}

/// Compute waypoints for connector routing.
pub fn route_connector(
    from: [f32; 2],
    norm_from: [f32; 2],
    to: [f32; 2],
    norm_to: [f32; 2],
    routing: ConnectorRouting,
    margin: f32,
) -> Vec<[f32; 2]> {
    match routing {
        ConnectorRouting::Straight => vec![from, to],
        ConnectorRouting::LBend => {
            if norm_from[0].abs() > 0.5 {
                vec![from, [to[0], from[1]], to]
            } else if norm_from[1].abs() > 0.5 {
                vec![from, [from[0], to[1]], to]
            } else {
                vec![from, [to[0], from[1]], to]
            }
        },
        ConnectorRouting::Elbow => {
            // Both horizontal (e.g. Right -> Left or Left -> Right)
            if norm_from[0].abs() > 0.5 && norm_to[0].abs() > 0.5 {
                let forward = (norm_from[0] > 0.0 && from[0] + margin <= to[0] - margin)
                    || (norm_from[0] < 0.0 && from[0] - margin >= to[0] + margin);
                if forward {
                    let mid_x = (from[0] + to[0]) * 0.5;
                    vec![from, [mid_x, from[1]], [mid_x, to[1]], to]
                } else {
                    let mid_y = if from[1] <= to[1] {
                        (from[1] + to[1]) * 0.5
                    } else {
                        from[1] - margin * 2.0
                    };
                    let x1 = from[0] + norm_from[0] * margin;
                    let x2 = to[0] + norm_to[0] * margin;
                    vec![from, [x1, from[1]], [x1, mid_y], [x2, mid_y], [x2, to[1]], to]
                }
            }
            // Both vertical (e.g. Bottom -> Top or Top -> Bottom)
            else if norm_from[1].abs() > 0.5 && norm_to[1].abs() > 0.5 {
                let forward = (norm_from[1] > 0.0 && from[1] + margin <= to[1] - margin)
                    || (norm_from[1] < 0.0 && from[1] - margin >= to[1] + margin);
                if forward {
                    let mid_y = (from[1] + to[1]) * 0.5;
                    vec![from, [from[0], mid_y], [to[0], mid_y], to]
                } else {
                    let mid_x = (from[0] + to[0]) * 0.5;
                    let y1 = from[1] + norm_from[1] * margin;
                    let y2 = to[1] + norm_to[1] * margin;
                    vec![from, [from[0], y1], [mid_x, y1], [mid_x, y2], [to[0], y2], to]
                }
            }
            // Horizontal start, Vertical end
            else if norm_from[0].abs() > 0.5 && norm_to[1].abs() > 0.5 {
                vec![from, [to[0], from[1]], to]
            }
            // Vertical start, Horizontal end
            else if norm_from[1].abs() > 0.5 && norm_to[0].abs() > 0.5 {
                vec![from, [from[0], to[1]], to]
            }
            // Default step elbow
            else {
                let mid_x = (from[0] + to[0]) * 0.5;
                vec![from, [mid_x, from[1]], [mid_x, to[1]], to]
            }
        },
    }
}

/// Build shaft Bézier path with fillets and an optional arrowhead.
pub fn build_connector_path(
    waypoints: &[[f32; 2]],
    corner_radius: f32,
    has_arrow: bool,
    head_size: f32,
) -> (kurbo::BezPath, Option<kurbo::BezPath>) {
    if waypoints.len() < 2 {
        let mut empty = kurbo::BezPath::new();
        if let Some(p) = waypoints.first() {
            empty.move_to(kurbo::Point::new(p[0] as f64, p[1] as f64));
        }
        return (empty, None);
    }

    // Deduplicate consecutive identical points
    let mut clean_pts: Vec<[f32; 2]> = Vec::with_capacity(waypoints.len());
    for &p in waypoints {
        if let Some(last) = clean_pts.last() {
            if (p[0] - last[0]).hypot(p[1] - last[1]) < 0.1 {
                continue;
            }
        }
        clean_pts.push(p);
    }

    if clean_pts.len() < 2 {
        return (kurbo::BezPath::new(), None);
    }

    let end_pt = clean_pts[clean_pts.len() - 1];
    let prev_to_end = clean_pts[clean_pts.len() - 2];
    let dx = end_pt[0] - prev_to_end[0];
    let dy = end_pt[1] - prev_to_end[1];
    let last_seg_len = (dx * dx + dy * dy).sqrt();

    let (arrow_path, shaft_end_pt) = if has_arrow && last_seg_len > 1e-3 {
        let dir_x = (dx / last_seg_len) as f64;
        let dir_y = (dy / last_seg_len) as f64;
        let perp_x = -dir_y;
        let perp_y = dir_x;

        let head_size = (head_size as f64).max(2.0);
        let tip_len = head_size;
        let half_width = head_size * 0.4;

        let to_x = end_pt[0] as f64;
        let to_y = end_pt[1] as f64;
        let base_x = to_x - dir_x * tip_len;
        let base_y = to_y - dir_y * tip_len;

        let mut arrow = kurbo::BezPath::new();
        arrow.move_to(kurbo::Point::new(to_x, to_y));
        arrow.line_to(kurbo::Point::new(base_x + perp_x * half_width, base_y + perp_y * half_width));
        arrow.line_to(kurbo::Point::new(base_x - perp_x * half_width, base_y - perp_y * half_width));
        arrow.close_path();

        let adjusted_end = [base_x as f32, base_y as f32];
        (Some(arrow), adjusted_end)
    } else {
        (None, end_pt)
    };

    let n = clean_pts.len();
    let mut shaft = kurbo::BezPath::new();
    shaft.move_to(kurbo::Point::new(clean_pts[0][0] as f64, clean_pts[0][1] as f64));

    for i in 1..n - 1 {
        let prev = clean_pts[i - 1];
        let curr = clean_pts[i];
        let next = if i == n - 2 && has_arrow {
            shaft_end_pt
        } else {
            clean_pts[i + 1]
        };

        let in_dx = (prev[0] - curr[0]) as f64;
        let in_dy = (prev[1] - curr[1]) as f64;
        let in_len = (in_dx * in_dx + in_dy * in_dy).sqrt();

        let out_dx = (next[0] - curr[0]) as f64;
        let out_dy = (next[1] - curr[1]) as f64;
        let out_len = (out_dx * out_dx + out_dy * out_dy).sqrt();

        let r = (corner_radius as f64).min(in_len * 0.5).min(out_len * 0.5);

        if r > 0.5 && in_len > 1e-4 && out_len > 1e-4 {
            let start_x = curr[0] as f64 + (in_dx / in_len) * r;
            let start_y = curr[1] as f64 + (in_dy / in_len) * r;
            let end_x = curr[0] as f64 + (out_dx / out_len) * r;
            let end_y = curr[1] as f64 + (out_dy / out_len) * r;

            shaft.line_to(kurbo::Point::new(start_x, start_y));
            shaft.quad_to(
                kurbo::Point::new(curr[0] as f64, curr[1] as f64),
                kurbo::Point::new(end_x, end_y),
            );
        } else {
            shaft.line_to(kurbo::Point::new(curr[0] as f64, curr[1] as f64));
        }
    }

    shaft.line_to(kurbo::Point::new(shaft_end_pt[0] as f64, shaft_end_pt[1] as f64));
    (shaft, arrow_path)
}

/// The `Connector` primitive.
pub struct ConnectorPrimitive;

/// Singleton instance of `ConnectorPrimitive`.
pub const CONNECTOR: ConnectorPrimitive = ConnectorPrimitive;

impl Primitive for ConnectorPrimitive {
    fn type_name(&self) -> &str {
        "Connector"
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    fn handle_assignment(
        &self,
        track: &mut AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
    ) -> bool {
        match property {
            "from" => {
                if let Some(endpoint) = parse_connector_endpoint(value, env) {
                    track.geometry
                        .connector_from
                        .ensure(endpoint.clone())
                        .add_keyframe(ctx.t_start_ms, endpoint, ctx.easing);
                    return true;
                }
            },
            "to" => {
                if let Some(endpoint) = parse_connector_endpoint(value, env) {
                    track.geometry
                        .connector_to
                        .ensure(endpoint.clone())
                        .add_keyframe(ctx.t_start_ms, endpoint, ctx.easing);
                    return true;
                }
            },
            "routing" => {
                let routing_str = match value {
                    Expr::Ident(s) | Expr::Str(s) => s.as_str(),
                    _ => "elbow",
                };
                let routing = routing_str.parse::<ConnectorRouting>().unwrap_or(ConnectorRouting::Elbow);
                track.geometry
                    .connector_routing
                    .ensure(routing)
                    .add_keyframe(ctx.t_start_ms, routing, ctx.easing);
                return true;
            },
            "arrow" => {
                let arrow_val = match value {
                    Expr::Bool(b) => *b,
                    Expr::Ident(s) => s == "true",
                    _ => true,
                };
                track.geometry
                    .connector_arrow
                    .ensure(arrow_val)
                    .add_keyframe(ctx.t_start_ms, arrow_val, ctx.easing);
                return true;
            },
            _ => {},
        }
        false
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        let ep_from = ctx.track.geometry.connector_from.get(ctx.time_ms, ConnectorEndpoint::Point([0.0, 0.0]));
        let ep_to = ctx.track.geometry.connector_to.get(ctx.time_ms, ConnectorEndpoint::Point([100.0, 100.0]));
        let routing = ctx.track.geometry.connector_routing.get(ctx.time_ms, ConnectorRouting::Elbow);
        let mut arrow = ctx.track.geometry.connector_arrow.get(ctx.time_ms, true);
        let mut corner_radius = ctx.track.shape.corner_radius.get(ctx.time_ms, 8.0);
        let mut head_size = ctx.track.shape.head_size.get(ctx.time_ms, 10.0);

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

        // Two-pass resolve: resolve start with rough target hint, then resolve end, then refine start
        let (pos_from_init, _) = resolve_connector_endpoint(&ep_from, ctx.target_resolver, ctx.time_ms, ctx.scene_dimensions, None);
        let (pos_to, norm_to) = resolve_connector_endpoint(&ep_to, ctx.target_resolver, ctx.time_ms, ctx.scene_dimensions, Some(pos_from_init));
        let (pos_from, norm_from) = resolve_connector_endpoint(&ep_from, ctx.target_resolver, ctx.time_ms, ctx.scene_dimensions, Some(pos_to));

        let waypoints = route_connector(pos_from, norm_from, pos_to, norm_to, routing, 20.0);
        let (shaft_path, arrowhead_path) = build_connector_path(&waypoints, corner_radius, arrow, head_size);

        let style = sample_shape_style(ctx.track, ctx.time_ms, ctx.overrides);
        let mut paths = Vec::new();

        let shaft_vello = VelloPath {
            path: Arc::new(shaft_path),
            fill: None,
            stroke: crate::timeline::shapes::shape_stroke(style.stroke_color, style.stroke_width)
                .or_else(|| {
                    Some((
                        vello::peniko::Color::from_rgba8(
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
                fill: Some(vello::peniko::Color::from_rgba8(
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

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("from", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            Property::new("to", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)])),
            Property::new("routing", Expr::Str("elbow".into())),
            Property::new("arrow", Expr::Bool(true)),
            Property::new("corner_radius", Expr::Num(8.0)),
        ]
    }

    fn default_color_key(&self, property: &str, _caps: &ActorCaps) -> Option<&'static str> {
        match property {
            "stroke" | "color" => Some("stroke.default"),
            _ => None,
        }
    }
}
