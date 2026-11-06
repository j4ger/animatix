//! Implicit plot marching squares contour generation.

use super::adaptive::math_to_screen_padded;
use super::procedural::{FastExprKind, FastPlotProgram};
use super::{Environment, FuncSource, Value, flatten_blend};
use crate::timeline::modifier_runtime::ir::{
    CompiledExpr, compile_fast_scalar, evaluate_compiled_expr,
};

pub fn implicit_intersection(
    p0: (f64, f64, f64),
    p1: (f64, f64, f64),
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
) -> kurbo::Point {
    let (x0, y0, v0) = p0;
    let (x1, y1, v1) = p1;
    let t = if (v1 - v0).abs() <= f64::EPSILON {
        0.5
    } else {
        (-v0 / (v1 - v0)).clamp(0.0, 1.0)
    };
    let x = x0 + (x1 - x0) * t;
    let y = y0 + (y1 - y0) * t;
    let (screen_x, screen_y) = math_to_screen_padded(x, y, p_x_domain, p_y_domain, p_size, padding);
    kurbo::Point::new(screen_x, screen_y)
}

struct FastImplicitLeaf {
    weight: f64,
    program: std::sync::Arc<FastPlotProgram>,
    y_slot: Option<usize>,
    slots: Vec<f64>,
}

/// High-throughput unboxed implicit scalar field evaluator.
pub struct FastImplicitEvaluator {
    leaves: Vec<FastImplicitLeaf>,
}

impl FastImplicitEvaluator {
    pub fn try_compile(source: &FuncSource, env: &Environment) -> Option<Self> {
        Self::try_compile_with_cache(source, env, None)
    }

    pub fn try_compile_with_cache(
        source: &FuncSource,
        env: &Environment,
        cache: Option<&crate::timeline::EvalCaches>,
    ) -> Option<Self> {
        let flat = flatten_blend(source);
        let mut leaves = Vec::with_capacity(flat.len());
        for (weight, src) in flat {
            match src {
                FuncSource::Compiled(args, body, captures) => {
                    let arg_x = args.first().map(String::as_str).unwrap_or("x");
                    let arg_y = args.get(1).map(String::as_str).unwrap_or("y");
                    let body_ptr = &**body as *const CompiledExpr as usize;
                    let key = (body_ptr, 2);

                    let program = if let Some(cache) = cache {
                        cache.get_or_compile_fast_plot(key, || {
                            let mut slot_names_raw = Vec::with_capacity(8);
                            let expr = compile_fast_scalar(body, arg_x, &mut slot_names_raw)?;
                            Some(FastPlotProgram {
                                expr: FastExprKind::Scalar(expr),
                                slot_names: slot_names_raw
                                    .into_iter()
                                    .map(str::to_string)
                                    .collect(),
                            })
                        })?
                    } else {
                        let mut slot_names_raw = Vec::with_capacity(8);
                        let expr = compile_fast_scalar(body, arg_x, &mut slot_names_raw)?;
                        std::sync::Arc::new(FastPlotProgram {
                            expr: FastExprKind::Scalar(expr),
                            slot_names: slot_names_raw.into_iter().map(str::to_string).collect(),
                        })
                    };

                    let y_slot = program.slot_names.iter().position(|name| name == arg_y);
                    let mut slots = Vec::with_capacity(program.slot_names.len());
                    for (i, name) in program.slot_names.iter().enumerate() {
                        if Some(i) == y_slot {
                            slots.push(0.0);
                        } else if let Some(Value::Num(n)) = env.get_path(name) {
                            slots.push(n);
                        } else if let Some(Value::Num(n)) = captures.0.get(name) {
                            slots.push(*n);
                        } else {
                            return None;
                        }
                    }

                    leaves.push(FastImplicitLeaf {
                        weight,
                        program,
                        y_slot,
                        slots,
                    });
                },
                FuncSource::Blend { .. } => return None,
            }
        }
        Some(FastImplicitEvaluator { leaves })
    }

    #[inline(always)]
    pub fn eval(&mut self, x: f64, y: f64) -> f64 {
        let mut sum = 0.0;
        for leaf in &mut self.leaves {
            if let Some(y_slot) = leaf.y_slot
                && let Some(slot) = leaf.slots.get_mut(y_slot)
            {
                *slot = y;
            }
            if let FastExprKind::Scalar(ref expr) = leaf.program.expr {
                sum += leaf.weight * expr.eval(x, &leaf.slots);
            }
        }
        sum
    }
}

/// Evaluate a `FuncSource` at (x, y) coordinates, returning a scalar value
/// for implicit contour detection. Supports blended transitions via recursive
/// evaluation of `Blend` nodes.
pub fn eval_implicit_source(source: &FuncSource, env: &mut Environment, x: f64, y: f64) -> f64 {
    if let Some(mut fast) = FastImplicitEvaluator::try_compile(source, env) {
        return fast.eval(x, y);
    }
    eval_implicit_source_slow(source, env, x, y)
}

fn eval_implicit_source_slow(source: &FuncSource, env: &mut Environment, x: f64, y: f64) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            // Merge captured variables into the environment
            let inserted = captures.merge_missing_into(env);
            // Bind both arguments
            if args.len() >= 2 {
                env.set(&args[0], Value::Num(x));
                env.set(&args[1], Value::Num(y));
            }
            // Evaluate and return scalar
            let result = match evaluate_compiled_expr(body, env) {
                Ok(Value::Num(n)) => n,
                _ => f64::NAN,
            };
            for key in inserted {
                env.overrides.remove(&key);
                env.mark_mutated();
            }
            result
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                sum += weight * eval_implicit_source_slow(src, env, x, y);
            }
            sum
        },
    }
}

/// Build an implicit plot contour path from a `FuncSource`, evaluating
/// the scalar field on a grid and extracting zero-contours via marching
/// squares. Supports function transitions via blend sources.
pub fn build_implicit_plot_path_from_source(
    env: &mut Environment,
    source: &FuncSource,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
) -> kurbo::BezPath {
    build_implicit_plot_path_from_source_cached(
        env, source, p_x_domain, p_y_domain, p_size, resolution, padding, None,
    )
}

/// Build an implicit plot contour path from a `FuncSource` with optional cache.
pub fn build_implicit_plot_path_from_source_cached(
    env: &mut Environment,
    source: &FuncSource,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
    cache: Option<&crate::timeline::EvalCaches>,
) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    let x_cells = resolution.max(8);
    let aspect = if p_size[0] <= f64::EPSILON {
        1.0
    } else {
        p_size[1] / p_size[0]
    };
    let y_cells = ((x_cells as f64) * aspect).round().max(8.0) as usize;
    let dx = (p_x_domain[1] - p_x_domain[0]) / x_cells as f64;
    let dy = (p_y_domain[1] - p_y_domain[0]) / y_cells as f64;

    let mut fast_evaluator = FastImplicitEvaluator::try_compile_with_cache(source, env, cache);
    let mut grid = vec![vec![f64::NAN; x_cells + 1]; y_cells + 1];

    if let Some(ref mut fast) = fast_evaluator {
        for (yi, row) in grid.iter_mut().enumerate() {
            let y = p_y_domain[0] + yi as f64 * dy;
            for (xi, val) in row.iter_mut().enumerate() {
                let x = p_x_domain[0] + xi as f64 * dx;
                *val = fast.eval(x, y);
            }
        }
    } else {
        crate::timeline::utils::disable_eval_cache();
        for (yi, row) in grid.iter_mut().enumerate() {
            let y = p_y_domain[0] + yi as f64 * dy;
            for (xi, val) in row.iter_mut().enumerate() {
                let x = p_x_domain[0] + xi as f64 * dx;
                *val = eval_implicit_source(source, env, x, y);
            }
        }
        crate::timeline::utils::enable_eval_cache();
    }

    for yi in 0..y_cells {
        let y0 = p_y_domain[0] + yi as f64 * dy;
        let y1 = y0 + dy;
        for xi in 0..x_cells {
            let x0 = p_x_domain[0] + xi as f64 * dx;
            let x1 = x0 + dx;

            let bl = (x0, y0, grid[yi][xi]);
            let br = (x1, y0, grid[yi][xi + 1]);
            let tr = (x1, y1, grid[yi + 1][xi + 1]);
            let tl = (x0, y1, grid[yi + 1][xi]);

            if [bl.2, br.2, tr.2, tl.2].iter().any(|v| !v.is_finite()) {
                continue;
            }

            let bl_in = bl.2 >= 0.0;
            let br_in = br.2 >= 0.0;
            let tr_in = tr.2 >= 0.0;
            let tl_in = tl.2 >= 0.0;

            let mut intersections = Vec::new();
            if bl_in != br_in {
                intersections.push((
                    0,
                    implicit_intersection(bl, br, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if br_in != tr_in {
                intersections.push((
                    1,
                    implicit_intersection(br, tr, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tr_in != tl_in {
                intersections.push((
                    2,
                    implicit_intersection(tr, tl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tl_in != bl_in {
                intersections.push((
                    3,
                    implicit_intersection(tl, bl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }

            match intersections.len() {
                2 => {
                    path.move_to(intersections[0].1);
                    path.line_to(intersections[1].1);
                },
                4 => {
                    let center = if let Some(ref mut fast) = fast_evaluator {
                        fast.eval((x0 + x1) * 0.5, (y0 + y1) * 0.5)
                    } else {
                        eval_implicit_source(source, env, (x0 + x1) * 0.5, (y0 + y1) * 0.5)
                    };
                    let center_positive = center >= 0.0;
                    let edge = |idx: usize| {
                        intersections
                            .iter()
                            .find(|(edge_idx, _)| *edge_idx == idx)
                            .map(|(_, pt)| *pt)
                    };
                    if bl_in == tr_in && br_in == tl_in {
                        let first_pair = if center_positive == bl_in {
                            (0, 3)
                        } else {
                            (0, 1)
                        };
                        let second_pair = if center_positive == bl_in {
                            (1, 2)
                        } else {
                            (2, 3)
                        };
                        if let (Some(a), Some(b)) = (edge(first_pair.0), edge(first_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                        if let (Some(a), Some(b)) = (edge(second_pair.0), edge(second_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                    }
                },
                0 => {},
                1 | 3 => {
                    tracing::debug!(
                        "Implicit plot: degenerate cell with {} intersections at ({}, {})",
                        intersections.len(),
                        xi,
                        yi
                    );
                },
                n => {
                    tracing::warn!(
                        "Implicit plot: unexpected {} intersections in cell ({}, {})",
                        n,
                        xi,
                        yi
                    );
                },
            }
        }
    }

    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, Expr};
    use crate::timeline::modifier_runtime::ir::compile_expr;
    use crate::timeline::{CapturedEnv, EvalCaches};

    fn make_circle_src() -> FuncSource {
        let expr = Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Ident("x".to_string())),
                )),
                BinaryOp::Add,
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("y".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Ident("y".to_string())),
                )),
            )),
            BinaryOp::Sub,
            Box::new(Expr::Num(1.0)),
        );
        let compiled = compile_expr(&expr).expect("compile expr");
        FuncSource::Compiled(
            vec!["x".to_string(), "y".to_string()],
            Box::new(compiled),
            CapturedEnv::default(),
        )
    }

    fn make_radius_src(captures: CapturedEnv) -> FuncSource {
        let expr = Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Ident("x".to_string())),
                )),
                BinaryOp::Add,
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("y".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Ident("y".to_string())),
                )),
            )),
            BinaryOp::Sub,
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("r".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Ident("r".to_string())),
            )),
        );
        let compiled = compile_expr(&expr).expect("compile expr");
        FuncSource::Compiled(vec!["x".to_string(), "y".to_string()], Box::new(compiled), captures)
    }

    #[test]
    fn test_fast_implicit_evaluator_circle() {
        let env = Environment::new();
        let src = make_circle_src();

        let mut fast =
            FastImplicitEvaluator::try_compile(&src, &env).expect("compile fast implicit");
        assert_eq!(fast.eval(0.0, 0.0), -1.0);
        assert_eq!(fast.eval(1.0, 0.0), 0.0);
        assert_eq!(fast.eval(0.0, 1.0), 0.0);
        assert_eq!(fast.eval(2.0, 0.0), 3.0);
    }

    #[test]
    fn test_fast_implicit_evaluator_with_captures_and_caching() {
        let env = Environment::new();
        let mut captures = CapturedEnv::default();
        captures.0.insert("r".to_string(), Value::Num(2.0));

        let src = make_radius_src(captures);
        let caches = EvalCaches::default();

        // First compilation: populates cache
        let mut fast1 = FastImplicitEvaluator::try_compile_with_cache(&src, &env, Some(&caches))
            .expect("compile fast implicit 1");
        assert_eq!(fast1.eval(0.0, 0.0), -4.0);
        assert_eq!(fast1.eval(2.0, 0.0), 0.0);

        assert_eq!(caches.fast_plot_programs.borrow().len(), 1);

        // Second compilation: hits cache
        let mut fast2 = FastImplicitEvaluator::try_compile_with_cache(&src, &env, Some(&caches))
            .expect("compile fast implicit 2");
        assert_eq!(fast2.eval(0.0, 2.0), 0.0);
        assert_eq!(caches.fast_plot_programs.borrow().len(), 1);

        // Invalidate cache:
        caches.clear_fast_plot_programs_if_stale();
        // Since content_epoch didn't change, still 1:
        assert_eq!(caches.fast_plot_programs.borrow().len(), 1);

        // Now advance content epoch:
        caches.fast_plot_epoch.set(999);
        caches.clear_fast_plot_programs_if_stale();
        assert_eq!(caches.fast_plot_programs.borrow().len(), 0);
    }
}
