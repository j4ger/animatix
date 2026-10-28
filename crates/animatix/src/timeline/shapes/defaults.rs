use super::*;

/// Map an actor type string to its corresponding `ShapeType`, if any.
pub fn shape_type_for_actor(ty: &str) -> Option<ShapeType> {
    // Resolve through the primitive's derived caps instead of a parallel
    // string table, so the mapping cannot drift from the primitive.
    let caps = animatix_std::caps_for_type(ty)?;
    if let Some(kind) = caps.shape {
        return Some(shape_kind_to_shape_type(kind));
    }
    match ty {
        // Plot-host kinds reuse the Graph command geometry for edit handles.
        "Graph" | "NumberPlane" => Some(ShapeType::Graph),
        "PlotCurve" => Some(ShapeType::Plot),
        _ => None,
    }
}

fn shape_kind_to_shape_type(kind: ShapeKind) -> ShapeType {
    match kind {
        ShapeKind::Rect => ShapeType::Rect,
        ShapeKind::Ellipse => ShapeType::Ellipse,
        ShapeKind::Line => ShapeType::Line,
        ShapeKind::Polygon => ShapeType::Polygon,
        ShapeKind::Path => ShapeType::Path,
        ShapeKind::Arrow => ShapeType::Arrow,
    }
}

/// Apply primitive-specific default values to a `VectorShapeState`.
#[allow(deprecated)]
pub fn apply_vector_shape_defaults(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.apply_defaults(state);
    }
}

/// Apply a single property expression to a `VectorShapeState`.
///
/// Returns `true` if the property was recognised and applied.
#[allow(deprecated)]
pub fn apply_vector_shape_property(
    actor_type: &str,
    name: &str,
    value: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    state: &mut VectorShapeState,
) -> bool {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        return primitive.apply_property(name, value, env, diagnostics, subject, state);
    }
    false
}

/// Run any post-processing / cleanup on a `VectorShapeState` after all properties have been
/// applied.
#[allow(deprecated)]
pub fn finalize_vector_shape_state(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.finalize_state(state);
    }
}

/// Whether this shape type exposes a tip-size property (currently only `Line`).
pub fn vector_shape_exposes_tip_size(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Line)
}

/// Whether this shape type can use a custom Bézier path (Polygon or Path).
pub fn vector_shape_uses_custom_path(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Polygon | ShapeType::Path)
}

/// Whether this shape type represents an arrow with a dedicated arrowhead.
pub fn vector_shape_is_arrow(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Arrow)
}

/// Extract the individual shape-state values for backward-compatible APIs.
///
/// Returns `(size, line_from, line_to, arc_angles, corner_radius)`.
pub fn extract_shape_state_values(
    state: &VectorShapeState,
) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2], f32) {
    let size = state.size();
    let (line_from, line_to, arc_angles) = match state {
        VectorShapeState::Line(line) => (line.line_from, line.line_to, [0.0, 0.0]),
        VectorShapeState::Arrow(arrow) => (arrow.from, arrow.to, [0.0, 0.0]),
        VectorShapeState::Callout(callout) => (callout.from, callout.to, [0.0, 0.0]),
        VectorShapeState::Ellipse(ellipse) => ([-50.0, 0.0], [50.0, 0.0], ellipse.arc_angles),
        _ => ([-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]),
    };
    let corner_radius = match state {
        VectorShapeState::Rect(rect) => rect.corner_radius,
        _ => 0.0,
    };
    (size, line_from, line_to, arc_angles, corner_radius)
}

/// Build a `VelloPath` for a vector shape via the primitive renderer.
#[allow(deprecated)]
pub fn build_vector_shape_vello_path(
    shape_type: ShapeType,
    state: &VectorShapeState,
    style: VectorShapeStyle,
) -> Option<VelloPath> {
    // Map shape type back to a primitive type name for lookup
    let type_name = match shape_type {
        ShapeType::Rect => "Rect",
        ShapeType::Ellipse => "Ellipse",
        ShapeType::Line => "Line",
        ShapeType::Polygon => "Polygon",
        ShapeType::Path => "Path",
        ShapeType::Arrow => "Arrow",
        _ => return None,
    };
    crate::primitives::find_primitive(type_name)
        .and_then(|primitive| {
            // Build-time helper (vector-path assignments): a thread-local scratch
            // shape tier provides the memo home. This runs inside `Timeline::build`
            // — allocating one per call showed up as a rebuild regression in the
            // 2026-09-04 full-suite gate. The memo is self-validating (keyed on
            // the sampled `KurboShape`), so cross-call reuse is safe. `render()`
            // reads only the memoized shape, never an entity identity.
            thread_local! {
                static SCRATCH_SHAPE: crate::timeline::animation_track::ShapeTracks =
                    crate::timeline::animation_track::ShapeTracks::default();
            }
            SCRATCH_SHAPE.with(|shape| {
                primitive.render(&crate::primitives::RenderCtx {
                    state,
                    style,
                    time_ms: 0,
                    id: crate::timeline::EntityId::INVALID,
                    shape: Some(shape),
                })
            })
        })
        .and_then(|paths| paths.into_iter().next())
}

/// Generate the vertex points for a regular polygon.
pub fn regular_polygon_points(sides: usize, radius: f32, rotation: f32) -> Vec<kurbo::Point> {
    let sides = sides.max(3);
    let radius = radius as f64;
    let angle_step = std::f64::consts::TAU / sides as f64;
    (0..sides)
        .map(|index| {
            let angle = -std::f64::consts::FRAC_PI_2 + rotation as f64 + angle_step * index as f64;
            kurbo::Point::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

/// Build a Bézier path for an arrow-tipped line.
pub fn build_arrow_path(
    line_from: [f32; 2],
    line_to: [f32; 2],
    tip_length: f32,
    tip_width: f32,
) -> kurbo::BezPath {
    let start = kurbo::Point::new(line_from[0] as f64, line_from[1] as f64);
    let tip = kurbo::Point::new(line_to[0] as f64, line_to[1] as f64);
    let dx = tip.x - start.x;
    let dy = tip.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();
    if length <= f64::EPSILON {
        path.move_to(tip);
        path.close_path();
        return path;
    }

    let dir_x = dx / length;
    let dir_y = dy / length;
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let tip_length = tip_length.max(1.0) as f64;
    let half_tip_width = (tip_width.max(1.0) as f64) / 2.0;
    let base = kurbo::Point::new(tip.x - dir_x * tip_length, tip.y - dir_y * tip_length);
    let left =
        kurbo::Point::new(base.x + perp_x * half_tip_width, base.y + perp_y * half_tip_width);
    let right =
        kurbo::Point::new(base.x - perp_x * half_tip_width, base.y - perp_y * half_tip_width);

    path.move_to(start);
    path.line_to(base);
    path.move_to(tip);
    path.line_to(left);
    path.line_to(right);
    path.close_path();
    path
}

/// Parse an AST list expression into a list of `kurbo::Point`s.
pub fn parse_point_list_expr(expr: &Expr, env: &Environment) -> Option<Vec<kurbo::Point>> {
    match expr {
        Expr::List(items) => {
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let [x, y] = parse_numeric_vec2(item, env)?;
                points.push(kurbo::Point::new(x as f64, y as f64));
            }
            Some(points)
        },
        _ => None,
    }
}

/// Parse an AST list of path commands (e.g. `move_to`, `line_to`) into a `kurbo::BezPath`.
pub fn parse_path_commands_expr(expr: &Expr, env: &Environment) -> Option<kurbo::BezPath> {
    let Expr::List(items) = expr else {
        return None;
    };

    let mut path = kurbo::BezPath::new();

    for item in items {
        let Expr::Call(name, args) = item else {
            return None;
        };

        match name.as_str() {
            "move_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.move_to((x, y));
            },
            "line_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.line_to((x, y));
            },
            "quad_to" => {
                if args.len() != 4 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                path.quad_to((x1, y1), (x2, y2));
            },
            "curve_to" => {
                if args.len() != 6 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                let x3 = evaluate_expr(&args[4], env).ok()?.as_num();
                let y3 = evaluate_expr(&args[5], env).ok()?.as_num();
                path.curve_to((x1, y1), (x2, y2), (x3, y3));
            },
            "close" => {
                if !args.is_empty() {
                    return None;
                }
                path.close_path();
            },
            _ => return None,
        }
    }

    Some(path)
}

/// Build a `KurboShape` from the legacy flat parameters.
pub fn build_shape(
    shape_type: ShapeType,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
) -> KurboShape {
    match shape_type {
        ShapeType::Ellipse => {
            if arc_angles[1] != 0.0 {
                KurboShape::Arc {
                    center: kurbo::Point::new(0.0, 0.0),
                    radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
                    start_angle: arc_angles[0] as f64,
                    sweep_angle: arc_angles[1] as f64,
                    rotation: 0.0,
                }
            } else {
                KurboShape::Ellipse {
                    center: kurbo::Point::new(0.0, 0.0),
                    radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
                    rotation: 0.0,
                }
            }
        },
        ShapeType::Line => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        ShapeType::Arrow => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        _ => KurboShape::Rect {
            x0: -(size[0] as f64),
            y0: -(size[1] as f64),
            x1: size[0] as f64,
            y1: size[1] as f64,
        },
    }
}

/// Compute the fill colour for a shape, returning `None` when transparent.
pub fn shape_fill_color(
    shape_type: ShapeType,
    color: [f32; 4],
    fill_opacity: f32,
) -> Option<vello::peniko::Color> {
    if fill_opacity <= 0.0 {
        return None;
    }

    if shape_type == ShapeType::Line || shape_type == ShapeType::Arrow {
        return None;
    }

    Some(vello::peniko::Color::from_rgba8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0 * fill_opacity) as u8,
    ))
}

/// Compute the stroke colour and width, returning `None` when invisible.
pub fn shape_stroke(
    stroke_color: [f32; 4],
    stroke_width: f32,
) -> Option<(vello::peniko::Color, f32)> {
    if stroke_width <= 0.0 {
        return None;
    }

    Some((
        vello::peniko::Color::from_rgba8(
            (stroke_color[0] * 255.0) as u8,
            (stroke_color[1] * 255.0) as u8,
            (stroke_color[2] * 255.0) as u8,
            (stroke_color[3] * 255.0) as u8,
        ),
        stroke_width,
    ))
}

/// Build a `VelloPath` from a `BezPath` and the current render style.
///
/// This is the shared helper that eliminates the copy-pasted `build_vello_path`
/// in every primitive implementation.
///
/// Set `force_stroke` to `true` for stroke-only shapes (Line, Arc) that need
/// a visible stroke even when the user hasn't explicitly set one.
pub fn build_vello_path(
    path: std::sync::Arc<kurbo::BezPath>,
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    fill_opacity: f32,
    force_stroke: bool,
) -> VelloPath {
    VelloPath {
        path,
        fill: if fill_opacity > 0.0 {
            Some(vello::peniko::Color::from_rgba8(
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
                (color[3] * 255.0 * fill_opacity) as u8,
            ))
        } else {
            None
        },
        stroke: shape_stroke(stroke_color, stroke_width).or_else(|| {
            if force_stroke {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        (color[3] * 255.0) as u8,
                    ),
                    1.0,
                ))
            } else {
                None
            }
        }),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }
}

/// Build a `VelloPath` from the legacy flat parameters.
pub fn build_shape_vello_path(
    shape_type: ShapeType,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    let state = match shape_type {
        ShapeType::Rect => VectorShapeState::Rect(RectState {
            size,
            corner_radius: 0.0,
        }),
        ShapeType::Ellipse => VectorShapeState::Ellipse(EllipseState {
            size,
            arc_angles,
            rotation: 0.0,
        }),
        ShapeType::Line => VectorShapeState::Line(LineState {
            size: [0.0, 0.0],
            line_from,
            line_to,
        }),
        ShapeType::Polygon => VectorShapeState::Polygon(PolygonState {
            size,
            regular_polygon_sides: 0,
            regular_polygon_radius: size[0],
            custom_path: None,
            rotation: 0.0,
            points: Vec::new(),
        }),
        ShapeType::Path => VectorShapeState::Path(PathState {
            size,
            custom_path: None,
        }),
        ShapeType::Arrow => VectorShapeState::Arrow(ArrowState {
            from: line_from,
            to: line_to,
            head_size: 10.0,
        }),
        _ => {
            return VelloPath {
                path: std::sync::Arc::new(
                    build_shape(shape_type, size, line_from, line_to, arc_angles).to_path_default(),
                ),
                fill: shape_fill_color(shape_type, color, fill_opacity),
                stroke: shape_stroke(stroke_color, stroke_width),
                line_cap: 0,
                line_join: 0,
                dash_pattern: None,
                dash_offset: 0.0,
                fill_gradient: None,
                stroke_gradient: None,
            };
        },
    };
    build_vector_shape_vello_path(
        shape_type,
        &state,
        VectorShapeStyle {
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
            line_cap: 0,
            line_join: 0,
        },
    )
    .unwrap_or_else(|| VelloPath {
        path: std::sync::Arc::new(
            build_shape(shape_type, size, line_from, line_to, arc_angles).to_path_default(),
        ),
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    })
}
