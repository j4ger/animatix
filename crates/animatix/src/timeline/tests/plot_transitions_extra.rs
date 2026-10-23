//! Blend flattening and nested-blend evaluation tests (split from
//! `plot_transitions`).

use crate::ast::Expr;
use crate::timeline::plot::{FuncSource, flatten_blend, resolve_func_source};

use super::plot_transitions::{circle_expr, cos_x_expr, line_yx_expr, sin_x_expr, stdlib_env};
use super::*;

// ─────────────────────────────────────────────────────────────
// Test 15: flatten_blend_basic
// ─────────────────────────────────────────────────────────────

/// Verify that `flatten_blend` on a non-blend source returns `[(1.0, source)]`.
#[test]
fn flatten_blend_basic() {
    let raw = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let flat = flatten_blend(&raw);
    assert_eq!(flat.len(), 1, "Expected 1 entry for non-blend source");
    assert!(
        (flat[0].0 - 1.0).abs() < 1e-12,
        "Expected weight 1.0 for non-blend source, got {}",
        flat[0].0
    );
    // Should point to the original source
    assert!(std::ptr::eq(flat[0].1, &raw), "Should reference the original source");
}

// ─────────────────────────────────────────────────────────────
// Test 16: flatten_blend_single_level
// ─────────────────────────────────────────────────────────────

/// A single `FuncSource::Blend { from: A, to: B, frozen_progress: 0.4 }`
/// should flatten to `[(0.6, A), (0.4, B)]`.
#[test]
fn flatten_blend_single_level() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let blend = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(b.clone()),
        frozen_progress: 0.4,
    };

    let flat = flatten_blend(&blend);
    assert_eq!(flat.len(), 2, "Expected 2 entries for single-level blend");

    // Sort by pointer to have deterministic order
    let mut flat_sorted = flat.clone();
    flat_sorted.sort_by_key(|(_, ptr)| *ptr as *const _ as usize);

    // Weights should be (1-0.4)=0.6 for from and 0.4 for to
    for (w, _) in &flat {
        assert!((*w - 0.6).abs() < 1e-12 || (*w - 0.4).abs() < 1e-12, "Unexpected weight {}", w);
    }
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );
}

// ─────────────────────────────────────────────────────────────
// Test 17: flatten_blend_nested_depth_2
// ─────────────────────────────────────────────────────────────

/// A depth-2 blend `blend(A, blend(B, C, p), q)` should flatten to
/// 3 entries with weights matching the lerp tree.
/// Lerp formula: `from*(1-q) + to*q` where inner to = blend(B,C,p) = B*(1-p)+C*p
/// Result: `A*(1-q) + B*(1-p)*q + C*p*q`
#[test]
fn flatten_blend_nested_depth_2() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(0.5), CapturedEnv::default());

    // Inner: blend(B, C, p=0.3)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.3,
    };
    // Outer: blend(A, inner, q=0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.6,
    };

    let flat = flatten_blend(&outer);
    assert_eq!(flat.len(), 3, "Expected 3 entries for depth-2 blend");

    // Total weight should sum to 1.0
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );

    // Expected weights:
    // A: 1 - q = 0.4
    // B: (1 - p) * q = 0.7 * 0.6 = 0.42
    // C: p * q = 0.3 * 0.6 = 0.18
    let expected_weights = [0.4, 0.42, 0.18];
    let mut flat_weights: Vec<f64> = flat.iter().map(|(w, _)| *w).collect();
    flat_weights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected_sorted = expected_weights.to_vec();
    expected_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, expected) in flat_weights.iter().zip(expected_sorted.iter()) {
        assert!((got - expected).abs() < 1e-12, "Expected weight {}, got {}", expected, got);
    }
}

// ─────────────────────────────────────────────────────────────
// Test 18: flatten_blend_nested_depth_3
// ─────────────────────────────────────────────────────────────

/// A depth-3 blend `blend(A, blend(B, blend(C, D, r), q), p)` should
/// flatten to 4 entries.
#[test]
fn flatten_blend_nested_depth_3() {
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(0.5), CapturedEnv::default());
    let d = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(1.0), CapturedEnv::default());

    // Inner: blend(C, D, r=0.2)
    let inner = FuncSource::Blend {
        from: Box::new(c.clone()),
        to: Box::new(d.clone()),
        frozen_progress: 0.2,
    };
    // Middle: blend(B, inner, q=0.4)
    let middle = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.4,
    };
    // Outer: blend(A, middle, p=0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(middle.clone()),
        frozen_progress: 0.6,
    };

    let flat = flatten_blend(&outer);
    assert_eq!(flat.len(), 4, "Expected 4 entries for depth-3 blend");

    // Total weight should sum to 1.0
    let total_weight: f64 = flat.iter().map(|(w, _)| w).sum();
    assert!(
        (total_weight - 1.0).abs() < 1e-12,
        "Expected total weight 1.0, got {}",
        total_weight
    );

    // Expected weights:
    // A: 1 - p = 0.4
    // B: (1 - q) * p = 0.6 * 0.6 = 0.36
    // C: (1 - r) * q * p = 0.8 * 0.4 * 0.6 = 0.192
    // D: r * q * p = 0.2 * 0.4 * 0.6 = 0.048
    let expected_weights = [0.4, 0.36, 0.192, 0.048];
    let mut flat_weights: Vec<f64> = flat.iter().map(|(w, _)| *w).collect();
    flat_weights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected_sorted = expected_weights.to_vec();
    expected_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for (got, expected) in flat_weights.iter().zip(expected_sorted.iter()) {
        assert!((got - expected).abs() < 1e-12, "Expected weight {}, got {}", expected, got);
    }
}

// ─────────────────────────────────────────────────────────────
// Test 19: resolve_func_source_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that evaluating a depth-2 nested blend via `resolve_func_source`
/// yields the same result as the mathematical formula.
///
/// Tree: blend(A, blend(B, C, 0.3), 0.6)
/// where A(x)=sin(x), B(x)=cos(x), C(x)=0.5
///
/// Formula: A*(1-0.6) + B*(1-0.3)*0.6 + C*0.3*0.6
///        = 0.4*sin(x) + 0.42*cos(x) + 0.18*0.5
///        = 0.4*sin(x) + 0.42*cos(x) + 0.09
#[test]
fn resolve_func_source_nested_blend() {
    let env = stdlib_env();

    // Raw sources
    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c_raw = Expr::Num(0.5);
    let c = FuncSource::from_expr(vec!["x".to_string()], c_raw, CapturedEnv::default());

    // Inner: blend(B, C, 0.3)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.3,
    };
    // Outer: blend(A, inner, 0.6)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.6,
    };

    for x in [0.0, 0.5, 1.0, 1.5, 2.0] {
        let result = resolve_func_source(&outer, &env, "x", x)
            .expect("nested blend evaluation should succeed");
        let expected = 0.4 * x.sin() + 0.42 * x.cos() + 0.09;
        assert!(
            (result - expected).abs() < 1e-9,
            "At x={}: expected {}, got {}",
            x,
            expected,
            result
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 20: eval_source_scalar_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that the inner `eval_source_scalar` path (used by adaptive
/// sampling) produces the same result as the mathematical formula for
/// a depth-2 nested blend.
#[test]
fn eval_source_scalar_nested_blend() {
    use std::collections::HashMap;

    use crate::timeline::plot::eval_source_scalar;

    let mut env = stdlib_env();

    let a = FuncSource::from_expr(vec!["x".to_string()], sin_x_expr(), CapturedEnv::default());
    let b = FuncSource::from_expr(vec!["x".to_string()], cos_x_expr(), CapturedEnv::default());
    let c = FuncSource::from_expr(vec!["x".to_string()], Expr::Num(42.0), CapturedEnv::default());

    // Inner: blend(B, C, 0.25)
    let inner = FuncSource::Blend {
        from: Box::new(b.clone()),
        to: Box::new(c.clone()),
        frozen_progress: 0.25,
    };
    // Outer: blend(A, inner, 0.8)
    let outer = FuncSource::Blend {
        from: Box::new(a.clone()),
        to: Box::new(inner.clone()),
        frozen_progress: 0.8,
    };

    // Formula: A*(1-0.8) + B*(1-0.25)*0.8 + C*0.25*0.8
    //        = 0.2*sin(x) + 0.6*cos(x) + 0.2*42.0
    for x in [0.0, 0.5, 1.0, std::f64::consts::PI / 4.0] {
        let mut cache = HashMap::new();
        let result = eval_source_scalar(&outer, &mut env, "x", x, &mut cache);
        let expected = 0.2 * x.sin() + 0.6 * x.cos() + 0.2 * 42.0;
        assert!(
            (result - expected).abs() < 1e-9,
            "At x={}: expected {}, got {}",
            x,
            expected,
            result
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Test 21: eval_implicit_source_nested_blend
// ─────────────────────────────────────────────────────────────

/// Verify that the implicit source evaluation path produces correct
/// results for a nested blend.
#[test]
fn eval_implicit_source_nested_blend() {
    use crate::timeline::plot::eval_implicit_source;

    let mut env = stdlib_env();

    let circle = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        circle_expr(),
        CapturedEnv::default(),
    );
    let line = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        line_yx_expr(),
        CapturedEnv::default(),
    );
    // Third implicit source: x + y
    let sum_expr = Expr::Binary(
        Box::new(Expr::Ident("x".to_string())),
        crate::ast::BinaryOp::Add,
        Box::new(Expr::Ident("y".to_string())),
    );
    let sum_src = FuncSource::from_expr(
        vec!["x".to_string(), "y".to_string()],
        sum_expr,
        CapturedEnv::default(),
    );

    // Inner: blend(circle, line, 0.4)
    let inner = FuncSource::Blend {
        from: Box::new(circle.clone()),
        to: Box::new(line.clone()),
        frozen_progress: 0.4,
    };
    // Outer: blend(inner, sum_src, 0.7)
    let outer = FuncSource::Blend {
        from: Box::new(inner.clone()),
        to: Box::new(sum_src.clone()),
        frozen_progress: 0.7,
    };

    // At (0,0):
    // circle(0,0) = -1, line(0,0) = 0, sum(0,0) = 0
    // inner = (-1)*(1-0.4) + 0*0.4 = -0.6
    // outer = (-0.6)*0.3 + 0*0.7 = -0.18
    // Formula: circle*(1-0.4)*(1-0.7) + line*0.4*(1-0.7) + sum*0.7
    //        = -1*0.6*0.3 + 0*0.4*0.3 + 0*0.7 = -0.18
    let result = eval_implicit_source(&outer, &mut env, 0.0, 0.0);
    assert!((result - (-0.18)).abs() < 1e-9, "At (0,0): expected -0.18, got {}", result);

    // At (1, 1):
    // circle(1,1) = 1, line(1,1) = 0, sum(1,1) = 2
    // inner = 1*0.6 + 0*0.4 = 0.6
    // outer = 0.6*0.3 + 2*0.7 = 0.18 + 1.4 = 1.58
    // Formula: 1*0.6*0.3 + 0*0.4*0.3 + 2*0.7 = 0.18 + 1.4 = 1.58
    let result = eval_implicit_source(&outer, &mut env, 1.0, 1.0);
    assert!((result - 1.58).abs() < 1e-9, "At (1,1): expected 1.58, got {}", result);
}
