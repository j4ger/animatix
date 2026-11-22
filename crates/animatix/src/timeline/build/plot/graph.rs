//! Graph axes, grid/ticks, NumberPlane paths and the `map`/`map_inverse`
//! native functions exposed to authored expressions.

use super::PlotActorProps;
use super::ticks::{format_axis_tick_label, normalize_axis};
use super::*;

/// Format a number for use as a bar label (strip trailing zeros).
pub(super) fn format_float(n: f64) -> String {
    format_axis_tick_label(n)
}

/// Build graph axis VelloPaths (X and Y axes, optional grid and ticks).
/// Omits an axis entirely when zero is not in its domain.
pub(crate) fn build_graph_axis_paths(
    size: [f32; 2],
    axis_color: [f32; 4],
    frame: &crate::timeline::CoordinateFrameComponent,
) -> Vec<VelloPath> {
    let [x_domain, y_domain] = [frame.x_domain, frame.y_domain];
    let [x_scale, y_scale] = [frame.x_scale, frame.y_scale];
    let padding = frame.padding;
    let (x_ticks, y_ticks) = if frame.grid || frame.ticks {
        super::ticks::graph_ticks(size, frame)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut paths = Vec::new();
    let mut axis_path = kurbo::BezPath::new();

    // Padding components and derived plot-area extents.
    let (pad_left, pad_right, pad_top, pad_bottom) =
        (padding[0], padding[1], padding[2], padding[3]);
    let hw = size[0] as f64; // graph half-width
    let hh = size[1] as f64; // graph half-height
    // Full-width/height of the padded plot area.
    let plot_fw = 2.0 * hw - pad_left - pad_right;
    let plot_fh = 2.0 * hh - pad_top - pad_bottom;
    // Screen-space shift of the padded plot center from the graph center.
    let shift_x = (pad_left - pad_right) / 2.0;
    let shift_y = (pad_top - pad_bottom) / 2.0;
    // Padded edge positions in local screen space.
    let left_edge = -hw + pad_left;
    let right_edge = hw - pad_right;
    let top_edge = -hh + pad_top;
    let bot_edge = hh - pad_bottom;

    // Helper: map a normalised coordinate to a padded screen position.
    let norm_to_screen_x = |norm: f64| shift_x + (norm - 0.5) * plot_fw;
    let norm_to_screen_y = |norm: f64| shift_y + (0.5 - norm) * plot_fh;
    let math_x_to_screen =
        |mx: f64| norm_to_screen_x(normalize_axis(mx, x_domain[0], x_domain[1], x_scale));
    let math_y_to_screen =
        |my: f64| norm_to_screen_y(normalize_axis(my, y_domain[0], y_domain[1], y_scale));

    // X-axis: drawn at y=0 when inside y_domain (linear scale), or clamped to bottom/top
    let x_axis_y = if !y_scale.is_log() && y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let y = math_y_to_screen(0.0);
        axis_path.move_to((left_edge, y));
        axis_path.line_to((right_edge, y));
        // X-axis arrow at positive end (right_edge)
        let arrow_sz = 6.0;
        axis_path.move_to((right_edge - arrow_sz, y - arrow_sz * 0.6));
        axis_path.line_to((right_edge, y));
        axis_path.line_to((right_edge - arrow_sz, y + arrow_sz * 0.6));
        Some(y)
    } else {
        // Fallback to bottom edge (min y in standard plot coords)
        let y = bot_edge;
        axis_path.move_to((left_edge, y));
        axis_path.line_to((right_edge, y));
        let arrow_sz = 6.0;
        axis_path.move_to((right_edge - arrow_sz, y - arrow_sz * 0.6));
        axis_path.line_to((right_edge, y));
        axis_path.line_to((right_edge - arrow_sz, y + arrow_sz * 0.6));
        Some(y)
    };

    // Y-axis: drawn at x=0 when inside x_domain (linear scale), or clamped to left edge
    let y_axis_x = if !x_scale.is_log() && x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let x = math_x_to_screen(0.0);
        axis_path.move_to((x, bot_edge));
        axis_path.line_to((x, top_edge));
        // Y-axis arrow at positive end (top_edge in screen coords, which is math top)
        let arrow_sz = 6.0;
        axis_path.move_to((x - arrow_sz * 0.6, top_edge + arrow_sz));
        axis_path.line_to((x, top_edge));
        axis_path.line_to((x + arrow_sz * 0.6, top_edge + arrow_sz));
        Some(x)
    } else {
        // Fallback to left edge (min x in standard plot coords)
        let x = left_edge;
        axis_path.move_to((x, bot_edge));
        axis_path.line_to((x, top_edge));
        let arrow_sz = 6.0;
        axis_path.move_to((x - arrow_sz * 0.6, top_edge + arrow_sz));
        axis_path.line_to((x, top_edge));
        axis_path.line_to((x + arrow_sz * 0.6, top_edge + arrow_sz));
        Some(x)
    };

    paths.push(graph_stroke_path(axis_path, axis_color, 2.0, 1));

    // Grid lines
    if frame.grid {
        let mut grid_path = kurbo::BezPath::new();

        // Vertical grid lines (constant x)
        for x in &x_ticks {
            let screen_x = math_x_to_screen(*x);
            grid_path.move_to((screen_x, top_edge));
            grid_path.line_to((screen_x, bot_edge));
        }

        // Horizontal grid lines (constant y)
        for y in &y_ticks {
            let screen_y = math_y_to_screen(*y);
            grid_path.move_to((left_edge, screen_y));
            grid_path.line_to((right_edge, screen_y));
        }

        if !grid_path.elements().is_empty() {
            let alpha_divisor = if frame.axis_options.label_size.is_some() {
                8
            } else {
                4
            };
            paths.push(graph_stroke_path(grid_path, axis_color, 1.0, alpha_divisor));
        }
    }

    // Tick marks
    if frame.ticks {
        let mut tick_path = kurbo::BezPath::new();
        let tick_len = 4.0;

        if let Some(y) = x_axis_y {
            for x in &x_ticks {
                let screen_x = math_x_to_screen(*x);
                tick_path.move_to((screen_x, y - tick_len));
                tick_path.line_to((screen_x, y + tick_len));
            }
        }

        if let Some(x) = y_axis_x {
            for y in &y_ticks {
                let screen_y = math_y_to_screen(*y);
                tick_path.move_to((x - tick_len, screen_y));
                tick_path.line_to((x + tick_len, screen_y));
            }
        }

        if !tick_path.elements().is_empty() {
            paths.push(graph_stroke_path(tick_path, axis_color, 1.5, 1));
        }
    }

    paths
}

fn graph_stroke_path(
    path: kurbo::BezPath,
    color: [f32; 4],
    width: f32,
    alpha_divisor: u8,
) -> VelloPath {
    let [r, g, b, a] = color.map(|component| (component * 255.0) as u8);
    VelloPath {
        path: std::sync::Arc::new(path),
        fill: None,
        stroke: Some((peniko::Color::from_rgba8(r, g, b, a / alpha_divisor), width)),
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }
}

/// Build NumberPlane VelloPaths: axes, grid lines, tick marks.
///
/// Uses `x_range` and `y_range` (min, max, step) for grid/ticks placement,
/// mapped to screen coordinates via `x_domain`/`y_domain` and `size`.
pub(crate) fn build_number_plane_paths(
    size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    x_range: [f64; 3],
    y_range: [f64; 3],
    axis_color: [f32; 4],
) -> Vec<VelloPath> {
    let mut paths = Vec::new();
    let full_w = size[0] as f64 * 2.0;
    let full_h = size[1] as f64 * 2.0;

    // Helper: math coords → screen coords (local to plot center)
    let math_to_screen = |mx: f64, my: f64| -> (f64, f64) {
        let sx = -(full_w / 2.0) + full_w * ((mx - x_domain[0]) / (x_domain[1] - x_domain[0]));
        let sy = (full_h / 2.0) - full_h * ((my - y_domain[0]) / (y_domain[1] - y_domain[0]));
        (sx, sy)
    };

    let [x_min, x_max, x_step] = x_range;
    let [y_min, y_max, y_step] = y_range;
    let step_x = x_step.max(0.001);
    let step_y = y_step.max(0.001);

    let axis_c = peniko::Color::from_rgba8(
        (axis_color[0] * 255.0) as u8,
        (axis_color[1] * 255.0) as u8,
        (axis_color[2] * 255.0) as u8,
        (axis_color[3] * 255.0) as u8,
    );
    let grid_c = peniko::Color::from_rgba8(
        (axis_color[0] * 255.0) as u8,
        (axis_color[1] * 255.0) as u8,
        (axis_color[2] * 255.0) as u8,
        (axis_color[3] * 255.0) as u8 / 4,
    );

    // ── Grid lines ──
    let mut grid_path = kurbo::BezPath::new();

    // Vertical grid lines at each x step
    let mut x = (x_min / step_x).ceil() * step_x;
    while x <= x_max {
        let (sx, _) = math_to_screen(x, 0.0);
        grid_path.move_to((sx, -(full_h / 2.0)));
        grid_path.line_to((sx, full_h / 2.0));
        x += step_x;
    }

    // Horizontal grid lines at each y step
    let mut y = (y_min / step_y).ceil() * step_y;
    while y <= y_max {
        let (_, sy) = math_to_screen(0.0, y);
        grid_path.move_to((-(full_w / 2.0), sy));
        grid_path.line_to((full_w / 2.0, sy));
        y += step_y;
    }

    if !grid_path.elements().is_empty() {
        paths.push(VelloPath {
            path: std::sync::Arc::new(grid_path),
            fill: None,
            stroke: Some((grid_c, 1.0)),
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        });
    }

    // ── Axes (X-axis at y=0, Y-axis at x=0) ──
    let mut axis_path = kurbo::BezPath::new();
    let mut has_axis = false;

    // X-axis: y=0 in math coords
    if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let (sx0, sy) = math_to_screen(x_domain[0], 0.0);
        let (sx1, _) = math_to_screen(x_domain[1], 0.0);
        axis_path.move_to((sx0, sy));
        axis_path.line_to((sx1, sy));
        has_axis = true;
    }

    // Y-axis: x=0 in math coords
    if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let (sx, sy0) = math_to_screen(0.0, y_domain[0]);
        let (_, sy1) = math_to_screen(0.0, y_domain[1]);
        axis_path.move_to((sx, sy0));
        axis_path.line_to((sx, sy1));
        has_axis = true;
    }

    if has_axis {
        paths.push(VelloPath {
            path: std::sync::Arc::new(axis_path),
            fill: None,
            stroke: Some((axis_c, 2.0)),
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        });
    }

    // ── Tick marks (4px perpendicular lines at each step on axes) ──
    let mut tick_path = kurbo::BezPath::new();
    let tick_len = 4.0;

    // Ticks on X-axis (at each x step, if axis is visible)
    if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let (_, axis_y) = math_to_screen(0.0, 0.0);
        let mut tx = (x_min / step_x).ceil() * step_x;
        while tx <= x_max {
            if tx != 0.0 {
                let (sx, _) = math_to_screen(tx, 0.0);
                tick_path.move_to((sx, axis_y - tick_len));
                tick_path.line_to((sx, axis_y + tick_len));
            }
            tx += step_x;
        }
    }

    // Ticks on Y-axis (at each y step, if axis is visible)
    if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let (axis_x, _) = math_to_screen(0.0, 0.0);
        let mut ty = (y_min / step_y).ceil() * step_y;
        while ty <= y_max {
            if ty != 0.0 {
                let (_, sy) = math_to_screen(0.0, ty);
                tick_path.move_to((axis_x - tick_len, sy));
                tick_path.line_to((axis_x + tick_len, sy));
            }
            ty += step_y;
        }
    }

    if !tick_path.elements().is_empty() {
        paths.push(VelloPath {
            path: std::sync::Arc::new(tick_path),
            fill: None,
            stroke: Some((axis_c, 1.5)),
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        });
    }

    paths
}

/// Create a `Value::NativeFn` for `{label}.map_inverse(screen_x, screen_y)` → math coords.
///
/// Captures the host's domain, padding and scales from the coordinate frame
/// component (static), and reads `size` and `at` from the runtime environment
/// to support animation of graph size/position.
fn make_graph_map_inverse_fn(
    label: String,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    padding: [f64; 4],
    x_scale: super::super::utils::ScaleType,
    y_scale: super::super::utils::ScaleType,
) -> Value {
    Value::NativeFn(std::sync::Arc::new(
        move |args: &[Value],
              env: &crate::timeline::Environment|
              -> Result<Value, crate::timeline::EvalError> {
            if args.len() != 2 {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "graph.map_inverse expects 2 arguments: (screen_x, screen_y)".to_string(),
                ));
            }
            let sx = match &args[0] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map_inverse: first argument 'screen_x' must be a number".to_string(),
                    ));
                },
            };
            let sy = match &args[1] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map_inverse: second argument 'screen_y' must be a number"
                            .to_string(),
                    ));
                },
            };

            // Read size and at from the runtime environment for animation
            // support; padding is fixed at declaration time and captured above.
            let size = env
                .get(crate::timeline::env_keys::side_channel(&label, "size").as_str())
                .and_then(|v| match v {
                    Value::Vec2(s) => Some(s),
                    _ => None,
                })
                .unwrap_or([500.0, 500.0]);
            let at = env
                .get(&format!("{}.at", label))
                .and_then(|v| match v {
                    Value::Vec2(a) => Some(a),
                    _ => None,
                })
                .unwrap_or([0.0, 0.0]);

            let scale =
                super::super::utils::GraphScaleConfig::new(x_domain, y_domain, x_scale, y_scale);
            let geo = super::super::utils::GraphGeometry::new(size, at, padding);
            let [mx, my] = super::super::utils::graph_screen_to_math(sx, sy, &scale, &geo);
            Ok(Value::Vec2([mx, my]))
        },
    ))
}

/// Create a `Value::NativeFn` for `{label}.map(math_x, math_y)` → screen coords.
///
/// Captures `x_domain`/`y_domain`/`padding`/`x_scale`/`y_scale` (static), reads `size` and
/// `at` from the runtime environment to support animation of graph size/position.
fn make_graph_map_fn(
    label: String,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    padding: [f64; 4],
    x_scale: super::super::utils::ScaleType,
    y_scale: super::super::utils::ScaleType,
) -> Value {
    Value::NativeFn(std::sync::Arc::new(
        move |args: &[Value],
              env: &crate::timeline::Environment|
              -> Result<Value, crate::timeline::EvalError> {
            if args.len() != 2 {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "graph.map expects 2 arguments: (math_x, math_y)".to_string(),
                ));
            }
            let mx = match &args[0] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map: first argument 'math_x' must be a number".to_string(),
                    ));
                },
            };
            let my = match &args[1] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map: second argument 'math_y' must be a number".to_string(),
                    ));
                },
            };

            // Read size and at from runtime env for animation support.
            // `size` must come from the side channel, exactly as `map_inverse`
            // reads it: the dotted `{label}.size` key carries the HALF-size
            // track (`initial_size` is assigned `w/2` upstream and only the
            // side channel multiplies it back), so `map()` returned screen
            // coords at half the px/unit the plotted curve actually uses and
            // an actor tracking `map(f(t))` rode beside its own trail.
            let size = env
                .get(crate::timeline::env_keys::side_channel(&label, "size").as_str())
                .and_then(|v| match v {
                    Value::Vec2(s) => Some(s),
                    _ => None,
                })
                .unwrap_or([500.0, 500.0]);
            let at = env
                .get(&format!("{}.at", label))
                .and_then(|v| match v {
                    Value::Vec2(a) => Some(a),
                    _ => None,
                })
                .unwrap_or([0.0, 0.0]);

            let scale =
                super::super::utils::GraphScaleConfig::new(x_domain, y_domain, x_scale, y_scale);
            let geo = super::super::utils::GraphGeometry::new(size, at, padding);
            let [sx, sy] = super::super::utils::graph_math_to_screen(mx, my, &scale, &geo, false);
            Ok(Value::Vec2([sx, sy]))
        },
    ))
}

impl PlotActorProps {
    pub(super) fn coordinate_frame(&self) -> crate::timeline::CoordinateFrameComponent {
        crate::timeline::CoordinateFrameComponent {
            x_domain: self.x_domain,
            y_domain: self.y_domain,
            x_scale: self.x_scale,
            y_scale: self.y_scale,
            padding: self.graph_padding,
            grid: self.grid,
            ticks: self.ticks,
            tick_labels: self.tick_labels.clone(),
            axis_options: self.axis_options.clone(),
        }
    }
}

impl Timeline {
    /// Convert between local graph data and scene coordinates before projection.
    pub(crate) fn map_graph_point(
        &self,
        id: crate::timeline::EntityId,
        method: &str,
        args: &[(Option<String>, Value)],
        frame: &crate::primitives::RenderFrame<'_>,
    ) -> Option<Result<Value, crate::timeline::EvalError>> {
        use super::super::utils::{
            GraphGeometry, GraphScaleConfig, graph_math_to_screen, graph_screen_to_math,
        };

        let graph = self.world.coordinate_frame(id)?;
        let [
            (None, Value::Num(x)),
            (None, Value::Num(y)),
            (Some(option), Value::Str(space)),
        ] = args
        else {
            return Some(Err(crate::timeline::EvalError::TypeMismatch(format!(
                "graph.{method} expects two positional numbers followed by space: \"scene\""
            ))));
        };
        if option != "space" || space != "scene" {
            return Some(Err(crate::timeline::EvalError::TypeMismatch(format!(
                "graph.{method} expects space: \"scene\""
            ))));
        }
        let (transform, half_size) = self.actor_point_transform(id, frame, false);
        let scale =
            GraphScaleConfig::new(graph.x_domain, graph.y_domain, graph.x_scale, graph.y_scale);
        let local =
            GraphGeometry::new(half_size.map(|v| 2.0 * f64::from(v)), [0.0; 2], graph.padding);
        let point = if method == "map_inverse" {
            if transform.determinant() == 0.0 {
                return Some(Err(crate::timeline::EvalError::TypeMismatch(
                    "graph.map_inverse requires an invertible graph transform".to_string(),
                )));
            }
            let p = transform.inverse() * kurbo::Point::new(*x, *y);
            graph_screen_to_math(p.x, p.y, &scale, &local)
        } else {
            let [x, y] = graph_math_to_screen(*x, *y, &scale, &local, true);
            let p = transform * kurbo::Point::new(x, y);
            [p.x, p.y]
        };
        Some(Ok(Value::Vec2(point)))
    }

    /// Register a graph host's coordinate frame and the `{label}_*` env entries
    /// authored expressions read, plus the `map`/`map_inverse` native functions.
    pub(super) fn register_graph_host_env(&mut self, label: &str, parsed: &PlotActorProps) {
        // ECS registration: the frame component is the only source for the
        // hosted children's coordinate resolution below.
        let frame = parsed.coordinate_frame();
        if let Some(host_id) = self.world.tracks.get_id(label) {
            self.set_coordinate_frame(host_id, frame.clone());
        }

        // The `{label}_*` env entries are authored expression variables
        // (e.g. `g_x_domain` in an `at:` expression). They are derived from
        // the registered frame, never a second source of truth.
        self.env.set(&format!("{}_x_domain", label), Value::Vec2(frame.x_domain));
        self.env.set(&format!("{}_y_domain", label), Value::Vec2(frame.y_domain));
        // Store the DECLARED (full-pixel) size. Every consumer of this env
        // key (math_to_screen_padded, hosted BarChart math mode,
        // .map()/.map_inverse via GraphGeometry) treats it as full size;
        // seeding the half-size here made hosted plots occupy only the
        // central half of the axis box.
        self.env.set(
            &crate::timeline::env_keys::side_channel(label, "size"),
            Value::Vec2([
                (parsed.initial_size[0] * 2.0) as f64,
                (parsed.initial_size[1] * 2.0) as f64,
            ]),
        );
        // Store graph axis settings for authored expressions.
        self.env.set(&format!("{}_grid", label), Value::Bool(frame.grid));
        self.env.set(&format!("{}_ticks", label), Value::Bool(frame.ticks));
        self.env
            .set(&format!("{}_tick_labels", label), Value::Str(frame.tick_labels.clone()));
        self.env.set(&format!("{}_padding", label), Value::Vec4(frame.padding));
        let x_scale_str = if frame.x_scale.is_log() {
            "log"
        } else {
            "linear"
        };
        let y_scale_str = if frame.y_scale.is_log() {
            "log"
        } else {
            "linear"
        };
        self.env.set(&format!("{}_x_scale", label), Value::Str(x_scale_str.to_string()));
        self.env.set(&format!("{}_y_scale", label), Value::Str(y_scale_str.to_string()));

        // Inject {label}.map as a NativeFn that converts math coords to screen coords.
        // Captures x_domain/y_domain/graph_padding/scales (static), reads size/at from runtime
        // env.
        let map_label = label.to_string();
        let nf = make_graph_map_fn(
            map_label.clone(),
            parsed.x_domain,
            parsed.y_domain,
            parsed.graph_padding,
            parsed.x_scale,
            parsed.y_scale,
        );
        self.env.set(&crate::timeline::env_keys::native_fn(label, "map"), nf);

        // Inject {label}.map_inverse as a NativeFn that converts screen coords to math coords.
        let nf_inv = make_graph_map_inverse_fn(
            map_label,
            parsed.x_domain,
            parsed.y_domain,
            parsed.graph_padding,
            parsed.x_scale,
            parsed.y_scale,
        );
        self.env
            .set(&crate::timeline::env_keys::native_fn(label, "map_inverse"), nf_inv);
    }
}
