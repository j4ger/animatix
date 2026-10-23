//! Curve builtins (`curve`, `curve_smooth`).

use super::*;

use super::builtins::CurveSlice;

pub(super) fn eval_curve_at(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!(
            "curve_at requires 2 arguments (curve, t), got {}",
            args.len()
        )));
    }
    let curve = CurveSlice::new(&args[0])?;
    let t = value_to_f64(&args[1])?;
    Ok(Value::Num(curve_at_slice(curve, t)))
}

pub(crate) fn curve_at_slice(curve: CurveSlice, t: f64) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    if curve.len() == 1 {
        return curve.get(0).1;
    }
    let first = curve.first().unwrap();
    if t <= first.0 {
        return first.1;
    }
    let last = curve.last().unwrap();
    if t >= last.0 {
        return last.1;
    }
    match curve.binary_search_by_t(t) {
        Ok(idx) => curve.get(idx).1,
        Err(idx) => {
            if idx == 0 {
                curve.get(0).1
            } else if idx >= curve.len() {
                curve.last().unwrap().1
            } else {
                let (t0, v0) = curve.get(idx - 1);
                let (t1, v1) = curve.get(idx);
                let dt = t1 - t0;
                if dt <= 1e-12 {
                    v0
                } else {
                    let alpha = (t - t0) / dt;
                    v0 + alpha * (v1 - v0)
                }
            }
        },
    }
}

pub(super) fn eval_curve_smooth(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(format!(
            "curve_smooth requires 3 arguments (curve, t, tau), got {}",
            args.len()
        )));
    }
    let curve = CurveSlice::new(&args[0])?;
    let t = value_to_f64(&args[1])?;
    let tau = value_to_f64(&args[2])?;
    Ok(Value::Num(curve_smooth_slice(curve, t, tau)))
}

pub(crate) fn curve_smooth_slice(curve: CurveSlice, t: f64, tau: f64) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    if curve.len() == 1 || tau <= 0.0 {
        return curve_at_slice(curve, t);
    }

    let t0 = curve.first().unwrap().0;
    let t_last = curve.last().unwrap().0;
    let duration = t_last - t0;

    let w = 5.0 * tau;
    let u_start = t - w;

    let is_loop =
        duration > 0.0 && (curve.first().unwrap().1 - curve.last().unwrap().1).abs() < 1e-4;

    let integrate_segment = |u1: f64, u2: f64, v1: f64, v2: f64, target_t: f64| -> f64 {
        if u2 <= u1 {
            return 0.0;
        }
        let s1 = (target_t - u1) / tau;
        let s2 = (target_t - u2) / tau;
        let e1 = (-s1).exp();
        let e2 = (-s2).exp();
        let di0 = e2 - e1;
        let du = u2 - u1;
        if du <= 1e-12 {
            return v1 * di0;
        }
        let b = (v2 - v1) / du;
        let a = v1 - b * u1;
        let di1 = (u2 - tau) * e2 - (u1 - tau) * e1;
        a * di0 + b * di1
    };

    let mut total_integral = 0.0;

    let mut integrate_window = |win_start: f64, win_end: f64, target_t: f64| {
        if win_end <= win_start {
            return;
        }
        if win_start < t0 {
            let clamp_end = win_end.min(t0);
            let v0 = curve.first().unwrap().1;
            total_integral += integrate_segment(win_start, clamp_end, v0, v0, target_t);
        }
        let seg_start = win_start.max(t0);
        let seg_end = win_end.min(t_last);
        if seg_end > seg_start {
            let start_idx = match curve.binary_search_by_t(seg_start) {
                Ok(idx) => idx.saturating_sub(1),
                Err(idx) => idx.saturating_sub(1),
            };
            for i in start_idx..curve.len() - 1 {
                let (pt1, pv1) = curve.get(i);
                if pt1 >= seg_end {
                    break;
                }
                let (pt2, pv2) = curve.get(i + 1);
                let overlap_start = seg_start.max(pt1);
                let overlap_end = seg_end.min(pt2);
                if overlap_end > overlap_start {
                    let v_start = if pt2 - pt1 > 1e-12 {
                        pv1 + (overlap_start - pt1) / (pt2 - pt1) * (pv2 - pv1)
                    } else {
                        pv1
                    };
                    let v_end = if pt2 - pt1 > 1e-12 {
                        pv1 + (overlap_end - pt1) / (pt2 - pt1) * (pv2 - pv1)
                    } else {
                        pv2
                    };
                    total_integral +=
                        integrate_segment(overlap_start, overlap_end, v_start, v_end, target_t);
                }
            }
        }
        if win_end > t_last {
            let clamp_start = win_start.max(t_last);
            let v_last = curve.last().unwrap().1;
            total_integral += integrate_segment(clamp_start, win_end, v_last, v_last, target_t);
        }
    };

    if is_loop && u_start < t0 {
        integrate_window(t0, t, t);
        let wrapped_len = t0 - u_start;
        let wrap_start = (t_last - wrapped_len).max(t0);
        integrate_window(wrap_start, t_last, t + duration);
    } else {
        integrate_window(u_start, t, t);
    }

    let total_weight = 1.0 - (-5.0_f64).exp();
    if total_weight > 0.0 {
        total_integral / total_weight
    } else {
        total_integral
    }
}

// ─── Helper: unary math ───────────────────────────────────────────────────────
